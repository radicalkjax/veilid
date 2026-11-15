//! Runs a family's mtu probes side by side, so a pass takes one peer's answer, not every
//! peer's in turn. The tick hands a pass over and reads its result later; nothing waits in a tick.

use super::*;
use futures_util::future::{select, Either};
use futures_util::stream::{FuturesUnordered, StreamExt};
use stop_token::future::FutureExt as _;

/// Probes one candidate and says what it showed
pub(crate) type MtuProber<C> = Arc<dyn Fn(C) -> PinBoxFutureStatic<MtuProbeOutcome> + Send + Sync>;

/// One family's probes to send at once
pub(crate) struct MtuProbePass<C> {
    /// The family the probes settle
    pub address_type: AddressType,
    /// The interface address generation the family started over at
    pub generation: u64,
    /// The peers to probe
    pub candidates: Vec<C>,
}

/// What a pass showed; one full round trip ends it before the rest answer
#[derive(Debug, Clone)]
pub(crate) struct MtuProbePassResult {
    /// The family the probes settle
    pub address_type: AddressType,
    /// The interface address generation the family started over at
    pub generation: u64,
    /// Every outcome in by the time the pass ended
    pub outcomes: Vec<MtuProbeOutcome>,
}

/// A pass whose peers are still answering
struct OpenPass {
    /// The family the probes settle
    address_type: AddressType,
    /// The interface address generation the family started over at
    generation: u64,
    /// What the peers said so far
    outcomes: Vec<MtuProbeOutcome>,
    /// Peers still to answer
    pending: usize,
    /// The result went out; the rest only drain
    reported: bool,
}

/// Runs passes as they arrive, every candidate at once, until the stop token fires or the
/// passes close
pub(crate) async fn mtu_probe_processor<C: Send + 'static>(
    prober: MtuProber<C>,
    stop_token: StopToken,
    passes: flume::Receiver<MtuProbePass<C>>,
    results: flume::Sender<MtuProbePassResult>,
) {
    let mut open = BTreeMap::<u64, OpenPass>::new();
    let mut next_pass_id = 0u64;
    let mut in_flight = FuturesUnordered::<PinBoxFutureStatic<(u64, MtuProbeOutcome)>>::new();

    loop {
        let event = if in_flight.is_empty() {
            match passes.recv_async().timeout_at(stop_token.clone()).await {
                Ok(Ok(pass)) => Either::Left(pass),
                _ => break,
            }
        } else {
            match select(passes.recv_async(), in_flight.next())
                .timeout_at(stop_token.clone())
                .await
            {
                Ok(Either::Left((Ok(pass), _))) => Either::Left(pass),
                Ok(Either::Right((Some(answered), _))) => Either::Right(answered),
                Ok(Either::Right((None, _))) => continue,
                _ => break,
            }
        };

        match event {
            Either::Left(pass) => {
                let pass_id = next_pass_id;
                next_pass_id += 1;
                if pass.candidates.is_empty() {
                    let empty = MtuProbePassResult {
                        address_type: pass.address_type,
                        generation: pass.generation,
                        outcomes: Vec::new(),
                    };
                    if results.send(empty).is_err() {
                        break;
                    }
                    continue;
                }
                open.insert(
                    pass_id,
                    OpenPass {
                        address_type: pass.address_type,
                        generation: pass.generation,
                        outcomes: Vec::new(),
                        pending: pass.candidates.len(),
                        reported: false,
                    },
                );
                for candidate in pass.candidates {
                    let probe = prober(candidate);
                    in_flight.push(Box::pin(async move { (pass_id, probe.await) }));
                }
            }
            Either::Right((pass_id, outcome)) => {
                let Some(open_pass) = open.get_mut(&pass_id) else {
                    continue;
                };
                open_pass.outcomes.push(outcome);
                open_pass.pending -= 1;
                if !open_pass.reported
                    && (outcome == MtuProbeOutcome::Full || open_pass.pending == 0)
                {
                    open_pass.reported = true;
                    let result = MtuProbePassResult {
                        address_type: open_pass.address_type,
                        generation: open_pass.generation,
                        outcomes: open_pass.outcomes.clone(),
                    };
                    if results.send(result).is_err() {
                        break;
                    }
                }
                if open_pass.pending == 0 {
                    open.remove(&pass_id);
                }
            }
        }
    }
}
