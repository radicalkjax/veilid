use super::*;

/// A peer the test prober answers for, after a delay
#[derive(Debug, Clone, Copy)]
struct ScriptedPeer {
    /// How long the peer takes to answer
    delay_ms: u32,
    /// What its probe shows
    outcome: MtuProbeOutcome,
}

fn scripted_prober() -> MtuProber<ScriptedPeer> {
    Arc::new(|peer: ScriptedPeer| {
        Box::pin(async move {
            sleep(peer.delay_ms).await;
            peer.outcome
        })
    })
}

/// Runs one pass through the processor and times it from hand-over to result
async fn run_pass(peers: Vec<ScriptedPeer>) -> (MtuProbePassResult, TimestampDuration) {
    let (pass_tx, pass_rx) = flume::unbounded();
    let (result_tx, result_rx) = flume::unbounded();
    let stop_source = StopSource::new();
    let jh = spawn(
        "test mtu probe processor",
        mtu_probe_processor(scripted_prober(), stop_source.token(), pass_rx, result_tx),
    );

    let started = Timestamp::now();
    pass_tx
        .send(MtuProbePass {
            address_type: AddressType::IPV4,
            generation: 1,
            candidates: peers,
        })
        .expect("processor should take the pass");
    let result = result_rx.recv_async().await.expect("pass should end");
    let elapsed = Timestamp::now().duration_since(started);

    drop(stop_source);
    jh.await;
    (result, elapsed)
}

fn peer(delay_ms: u32, outcome: MtuProbeOutcome) -> ScriptedPeer {
    ScriptedPeer { delay_ms, outcome }
}

/// A pass asks every peer at once: four peers that each take 300 ms are in well before the
/// 1200 ms asking them in turn would take
pub async fn test_pass_probes_every_peer_at_once() {
    let peers = vec![peer(300, MtuProbeOutcome::Inconclusive); 4];
    let serial = TimestampDuration::new_ms(peers.iter().map(|p| u64::from(p.delay_ms)).sum());

    let (result, elapsed) = run_pass(peers).await;
    info!(
        "pass of 4 peers at 300 ms each took {:#} (in turn: {:#})",
        elapsed, serial
    );
    assert_eq!(result.outcomes.len(), 4, "every peer gets its word in");
    assert_eq!(probe_verdict(&result.outcomes), None);
    assert!(
        elapsed >= TimestampDuration::new_ms(300),
        "no peer is skipped: {elapsed:#}"
    );
    assert!(
        elapsed < TimestampDuration::new_ms(900),
        "a pass takes one peer's answer, not the sum of them: {elapsed:#}"
    );
}

/// One full round trip settles the family the moment it arrives; peers that say nothing for
/// as long as a dead one would do not hold the pass
pub async fn test_full_ends_a_pass_before_the_rest_answer() {
    let peers = vec![
        peer(1500, MtuProbeOutcome::Inconclusive),
        peer(100, MtuProbeOutcome::Full),
        peer(1500, MtuProbeOutcome::Inconclusive),
        peer(1500, MtuProbeOutcome::Lost),
    ];

    let (result, elapsed) = run_pass(peers).await;
    info!("pass settled Full in {:#}", elapsed);
    assert_eq!(result.outcomes, vec![MtuProbeOutcome::Full]);
    assert_eq!(probe_verdict(&result.outcomes), Some(FamilyMtuState::Full));
    assert!(
        elapsed < TimestampDuration::new_ms(1000),
        "the dead peers did not hold the pass: {elapsed:#}"
    );
}

/// Losses wait on every peer's word, and a black hole takes two of them
pub async fn test_losses_wait_for_the_whole_pass() {
    let peers = vec![
        peer(100, MtuProbeOutcome::Lost),
        peer(300, MtuProbeOutcome::Inconclusive),
        peer(200, MtuProbeOutcome::Lost),
    ];

    let (result, elapsed) = run_pass(peers).await;
    assert_eq!(result.outcomes.len(), 3);
    assert_eq!(
        probe_verdict(&result.outcomes),
        Some(FamilyMtuState::BlackHole)
    );
    assert!(elapsed >= TimestampDuration::new_ms(300));
}

/// A pass with no peers ends at once, so the family can settle without one
pub async fn test_a_pass_with_no_peers_ends_at_once() {
    let (result, elapsed) = run_pass(Vec::new()).await;
    assert!(result.outcomes.is_empty());
    assert_eq!(result.generation, 1);
    assert!(elapsed < TimestampDuration::new_ms(100));
}

pub async fn test_all() {
    test_pass_probes_every_peer_at_once().await;
    test_full_ends_a_pass_before_the_rest_answer().await;
    test_losses_wait_for_the_whole_pass().await;
    test_a_pass_with_no_peers_ends_at_once().await;
}
