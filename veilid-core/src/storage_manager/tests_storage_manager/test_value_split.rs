//! Set and get fanouts over scripted storage nodes. A set lane applies `inbound_set_value`'s
//! sequence rule to the node's copy, a get lane answers with the copy unchanged, and each answer
//! goes through the classifier the real operation uses, so the real fanout runs with no network.
//! A run that has not returned within `RUN_BOUND_MS` counts as never returning.

use super::*;
use crate::crypto::tests_crypto::*;
use crate::routing_table::tests_routing_table::mocks::*;
use crate::tests::*;

/// Fanout timeout, short so a run that honors it returns well inside the bound
const FANOUT_TIMEOUT_MS: u64 = 1_000;
/// How long a run may take before it counts as never returning
const RUN_BOUND_MS: u32 = 10_000;
/// Answer delay for nodes that answer first
const FAST_MS: u32 = 10;
/// Answer delay between the fast and slow nodes
const MID_MS: u32 = 20;
/// Answer delay for nodes that answer after the fast ones
const SLOW_MS: u32 = 50;
/// A run that ends this fast never waited out a deadline
const QUICK: TimestampDuration = TimestampDuration::new_ms(FANOUT_TIMEOUT_MS / 2);
/// Adoptions shown in a report
const TRAIL_LEN: usize = 8;
/// A sequence number with successors left
const ORDINARY_SEQ: u32 = 7;

/// How a scripted node answers
#[derive(Clone, Copy)]
enum Answer {
    /// Applies the storage rule to its copy and answers
    Store,
    /// Not close enough to the record to store it
    Reject,
    /// The call times out
    Unreachable,
}

/// One scripted storage node
#[derive(Clone)]
struct NodeScript {
    /// Value the node holds when the fanout starts
    initial: Option<Arc<SignedValueData>>,
    /// Delay before the node answers
    delay_ms: u32,
    /// How the node answers
    answer: Answer,
}

impl NodeScript {
    fn holding(value: &Arc<SignedValueData>, delay_ms: u32) -> Self {
        Self {
            initial: Some(value.clone()),
            delay_ms,
            answer: Answer::Store,
        }
    }

    fn empty(delay_ms: u32) -> Self {
        Self {
            initial: None,
            delay_ms,
            answer: Answer::Store,
        }
    }

    fn far() -> Self {
        Self {
            initial: None,
            delay_ms: FAST_MS,
            answer: Answer::Reject,
        }
    }

    fn unreachable(delay_ms: u32) -> Self {
        Self {
            initial: None,
            delay_ms,
            answer: Answer::Unreachable,
        }
    }
}

/// How a scripted run ended
enum RunOutcome {
    /// The fanout returned
    Returned {
        /// Result kind
        kind: FanoutResultKind,
        /// Number of consensus nodes
        consensus: usize,
        /// Consensus nodes holding something other than the context's final value
        mismatched: usize,
        /// Times the context adopted a returned value
        adoptions: usize,
        /// The first values adopted, in order
        trail: String,
        /// The context's value when the fanout returned
        final_value: Arc<SignedValueData>,
        /// Whether some node held another value at the final value's sequence number
        conflicted: bool,
        /// Most times any one node was asked
        max_asks: usize,
        /// Time the run took
        elapsed: TimestampDuration,
    },
    /// The bound fired before the fanout returned
    StillRunning {
        /// Times the context adopted a returned value
        adoptions: usize,
        /// The first values adopted, in order
        trail: String,
    },
}

impl RunOutcome {
    fn describe(&self) -> String {
        match self {
            RunOutcome::Returned {
                kind,
                consensus,
                mismatched,
                adoptions,
                trail,
                elapsed,
                conflicted,
                max_asks,
                ..
            } => format!(
                "returned {:?} after {:#} with {} consensus nodes, {} holding another value, {} adoptions [{}], conflicted={}, max_asks={}",
                kind, elapsed, consensus, mismatched, adoptions, trail, conflicted, max_asks
            ),
            RunOutcome::StillRunning { adoptions, trail } => format!(
                "still running after {} ms, {} adoptions [{}]",
                RUN_BOUND_MS, adoptions, trail
            ),
        }
    }
}

pub(super) fn make_value(data: &[u8], seq: ValueSeqNum) -> Arc<SignedValueData> {
    let keypair = mock_keypair();
    Arc::new(SignedValueData::new(
        EncryptedValueData::try_new(
            seq,
            Bytes::from(data.to_vec()),
            keypair.key(),
            Some(fake_nonce()),
        )
        .expect("should make value data"),
        Signature::new(keypair.kind(), fake_bare_signature()),
    ))
}

/// `inbound_set_value`'s sequence rule: store a newer value, otherwise answer with the stored one
fn apply_inbound_set(
    stored: &mut Option<Arc<SignedValueData>>,
    sent: &Arc<SignedValueData>,
) -> Option<SignedValueData> {
    if let Some(last) = stored.as_ref() {
        let sent_seq = sent.value_data().seq();
        let last_seq = last.value_data().seq();
        if sent_seq < last_seq || (sent_seq == last_seq && sent.value_data() != last.value_data()) {
            return Some((**last).clone());
        }
        if sent_seq == last_seq {
            return None;
        }
    }
    *stored = Some(sent.clone());
    None
}

fn label(value: &SignedValueData) -> String {
    String::from_utf8_lossy(&value.value_data().data()).into_owned()
}

fn adoption_trail(adopted: &[String]) -> String {
    let mut out = adopted
        .iter()
        .take(TRAIL_LEN)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if adopted.len() > TRAIL_LEN {
        out += " ...";
    }
    out
}

fn rank_of(ranks: &HashMap<NodeId, usize>, node_ref: &NodeRef) -> usize {
    node_ref
        .node_ids()
        .get(CRYPTO_KIND_VLD0)
        .and_then(|id| ranks.get(&id).copied())
        .unwrap_or_log()
}

/// Answers one call to a scripted node, given its rank, its id and its script
type Lane = Arc<
    dyn Fn(usize, NodeId, NodeScript) -> PinBoxFutureStatic<FanoutCallDisposition> + Send + Sync,
>;

/// Nodes scripted by distance rank to the target, closest first, with the copy each one holds
struct ScriptedNodes {
    /// The record key the fanout runs toward
    target: HashCoordinate,
    /// Every node, in the order the routing table gave them
    node_refs: Vec<NodeRef>,
    /// Each node's distance rank
    ranks: Arc<HashMap<NodeId, usize>>,
    /// How each rank answers
    scripts: Arc<Vec<NodeScript>>,
    /// The copy each rank holds
    stores: Arc<Vec<Mutex<Option<Arc<SignedValueData>>>>>,
    /// Times each rank was asked
    asks: Arc<Vec<AtomicUsize>>,
}

impl ScriptedNodes {
    fn new(registry: &VeilidComponentRegistry, script: impl Fn(usize) -> NodeScript) -> Self {
        let routing_table = registry.routing_table();
        let node_count = registry.config().internal().network.dht.max_find_node_count as usize;
        let node_refs = seed_nodes(&routing_table, node_count);
        let target = fix_typed_node_id(CRYPTO_KIND_VLD0, 0).to_hash_coordinate();
        let ranks = Arc::new(rank_map(&node_refs, &target));
        let scripts: Arc<Vec<NodeScript>> = Arc::new((0..node_count).map(script).collect());
        let stores = Arc::new(
            scripts
                .iter()
                .map(|s| Mutex::new(s.initial.clone()))
                .collect(),
        );
        let asks = Arc::new((0..node_count).map(|_| AtomicUsize::new(0)).collect());
        Self {
            target,
            node_refs,
            ranks,
            scripts,
            stores,
            asks,
        }
    }
}

/// How a node that does not store answers, if this one does not
fn refusal(script: &NodeScript) -> Option<FanoutCallDisposition> {
    match script.answer {
        Answer::Store => None,
        Answer::Reject => Some(FanoutCallDisposition::Rejected),
        Answer::Unreachable => Some(FanoutCallDisposition::Timeout),
    }
}

/// Run one fanout over `nodes`, each call answered by `lane`, until it returns or the bound fires.
/// `final_state` reads the context's settled value and whether it was contested.
async fn run_fanout(
    registry: &VeilidComponentRegistry,
    name: &str,
    nodes: ScriptedNodes,
    (consensus_count, fanout_tasks): (usize, usize),
    lane: Lane,
    adopted: Arc<Mutex<Vec<String>>>,
    final_state: impl FnOnce() -> (Arc<SignedValueData>, bool),
) -> RunOutcome {
    let routing_table = registry.routing_table();
    let config = registry.config();
    let node_count = config.internal().network.dht.max_find_node_count as usize;
    let consensus_width = config.internal().network.dht.consensus_width as usize;

    let call_routine: FanoutCallRoutine = {
        let ranks = nodes.ranks.clone();
        let scripts = nodes.scripts.clone();
        let asks = nodes.asks.clone();
        Arc::new(move |node_ref: NodeRef| {
            let rank = rank_of(&ranks, &node_ref);
            asks[rank].fetch_add(1, Ordering::Relaxed);
            let node_id = node_ref.node_ids().get(CRYPTO_KIND_VLD0).unwrap_or_log();
            let answer = lane(rank, node_id, scripts[rank].clone());
            Box::pin(async move {
                Ok(FanoutCallOutput {
                    peer_info_list: vec![],
                    disposition: answer.await,
                })
            }) as PinBoxFutureStatic<FanoutCallResult>
        })
    };
    let check_done: FanoutCheckDone =
        Arc::new(|fanout_result: &FanoutResult| match fanout_result.kind {
            FanoutResultKind::Incomplete => FanoutDoneDisposition::NotDone,
            FanoutResultKind::Timeout
            | FanoutResultKind::Exhausted
            | FanoutResultKind::Consensus => FanoutDoneDisposition::DoneEarly,
        });

    let fanout_call = FanoutCall::new(
        &routing_table,
        FanoutCallParams {
            name: format!("value_split({})", name),
            hash_coordinate: nodes.target.clone(),
            node_count,
            fanout_tasks,
            consensus_count,
            consensus_width,
            timeout: TimestampDuration::new_ms(FANOUT_TIMEOUT_MS),
        },
        empty_fanout_peer_info_filter(),
        call_routine,
        check_done,
    );

    let stop_source = StopSource::new();
    let start_ts = Timestamp::now_non_decreasing();
    let run_result = timeout(
        RUN_BOUND_MS,
        fanout_call.run(
            nodes.node_refs.clone(),
            FanoutQueueMode::ThrottleAtConsensus,
            stop_source.token(),
        ),
    )
    .await;
    let adopted = adopted.lock().clone();
    let adoptions = adopted.len();
    let trail = adoption_trail(&adopted);
    match run_result {
        Err(_) => RunOutcome::StillRunning { adoptions, trail },
        Ok(result) => {
            let result = result.expect("fanout should run");
            let (final_value, conflicted) = final_state();
            let mismatched = result
                .consensus_nodes
                .iter()
                .filter(|node_ref| {
                    nodes.stores[rank_of(&nodes.ranks, node_ref)]
                        .lock()
                        .as_ref()
                        .is_none_or(|v| v.value_data() != final_value.value_data())
                })
                .count();
            let max_asks = nodes
                .asks
                .iter()
                .map(|asks| asks.load(Ordering::Relaxed))
                .max()
                .unwrap_or_default();
            RunOutcome::Returned {
                kind: result.kind,
                consensus: result.consensus_nodes.len(),
                mismatched,
                adoptions,
                trail,
                final_value,
                conflicted,
                max_asks,
                elapsed: Timestamp::now_non_decreasing().duration_since(start_ts),
            }
        }
    }
}

/// Run one set fanout of `ctx_value` against nodes scripted by distance rank, closest first
async fn run_scripted_set(
    name: &str,
    ctx_value: &Arc<SignedValueData>,
    script: impl Fn(usize) -> NodeScript,
) -> RunOutcome {
    let registry = mock_registry::init(format!("set_value_split_{}", name)).await;
    let outcome = {
        let config = registry.config();
        let counts = (
            config.internal().network.dht.set_value_count as usize,
            config.internal().network.dht.set_value_fanout as usize,
        );
        let nodes = ScriptedNodes::new(&registry, script);
        let context = Arc::new(Mutex::new(set_value::OutboundSetValueContext::new(
            ctx_value.clone(),
            nodes.target.clone(),
        )));
        let adopted: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let lane: Lane = {
            let registry = registry.clone();
            let stores = nodes.stores.clone();
            let context = context.clone();
            let adopted = adopted.clone();
            Arc::new(move |rank, node_id, script| {
                let registry = registry.clone();
                let stores = stores.clone();
                let context = context.clone();
                let adopted = adopted.clone();
                Box::pin(async move {
                    let sent_value = context.lock().value.clone();
                    sleep(script.delay_ms).await;
                    if let Some(disposition) = refusal(&script) {
                        return disposition;
                    }
                    let answer = apply_inbound_set(&mut stores[rank].lock(), &sent_value);
                    let mut ctx = context.lock();
                    let disposition =
                        ctx.classify_accepted_answer(&registry, &node_id, &sent_value, answer);
                    if matches!(
                        disposition,
                        FanoutCallDisposition::AcceptedNewerRestart
                            | FanoutCallDisposition::AcceptedSwitch
                    ) {
                        adopted.lock().push(label(&ctx.value));
                    }
                    disposition
                })
            })
        };

        run_fanout(&registry, name, nodes, counts, lane, adopted, || {
            let ctx = context.lock();
            (ctx.value.clone(), ctx.conflicted())
        })
        .await
    };
    mock_registry::terminate(registry).await;
    outcome
}

/// Run one get fanout, from the local copy `seed` if there is one, against nodes scripted by
/// distance rank, closest first
async fn run_scripted_get(
    name: &str,
    seed: Option<&Arc<SignedValueData>>,
    script: impl Fn(usize) -> NodeScript,
) -> RunOutcome {
    let registry = mock_registry::init(format!("get_value_split_{}", name)).await;
    let outcome = {
        let config = registry.config();
        let counts = (
            config.internal().network.dht.get_value_count as usize,
            config.internal().network.dht.get_value_fanout as usize,
        );
        let nodes = ScriptedNodes::new(&registry, script);
        let context = Arc::new(Mutex::new(
            get_value::OutboundGetValueContext::try_new(
                GetResult {
                    opt_value: seed.cloned(),
                    opt_descriptor: None,
                },
                nodes.target.clone(),
            )
            .expect("should make the get context"),
        ));
        let adopted: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let lane: Lane = {
            let registry = registry.clone();
            let stores = nodes.stores.clone();
            let context = context.clone();
            let adopted = adopted.clone();
            Arc::new(move |rank, node_id, script| {
                let registry = registry.clone();
                let stores = stores.clone();
                let context = context.clone();
                let adopted = adopted.clone();
                Box::pin(async move {
                    sleep(script.delay_ms).await;
                    if let Some(disposition) = refusal(&script) {
                        return disposition;
                    }
                    // The get routine answers a node with no copy before classifying
                    let Some(value) = stores[rank].lock().clone() else {
                        return FanoutCallDisposition::Stale;
                    };
                    let mut ctx = context.lock();
                    let disposition = ctx.classify_answer(&registry, &node_id, (*value).clone());
                    if matches!(
                        disposition,
                        FanoutCallDisposition::AcceptedNewer
                            | FanoutCallDisposition::AcceptedSwitch
                    ) {
                        adopted
                            .lock()
                            .push(label(ctx.value.as_ref().unwrap_or_log()));
                    }
                    disposition
                })
            })
        };

        run_fanout(&registry, name, nodes, counts, lane, adopted, || {
            let ctx = context.lock();
            (
                ctx.value.clone().expect("the get should settle a value"),
                ctx.conflicted(),
            )
        })
        .await
    };
    mock_registry::terminate(registry).await;
    outcome
}

/// Print the outcome and turn an unmet expectation into a failure line
fn report(scenario: &str, outcome: &RunOutcome, expected: bool) -> Result<(), String> {
    let line = format!("{}: {}", scenario, outcome.describe());
    println!("{}", line);
    if expected {
        Ok(())
    } else {
        Err(line)
    }
}

/// A write above every node's seq stores everywhere, however the older seq is split
async fn check_superseding_write_over_split() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let c = make_value(b"C", (ORDINARY_SEQ + 1).into());
    let outcome = run_scripted_set("superseding", &c, |rank| match rank {
        0..4 => NodeScript::holding(&a, FAST_MS),
        4..8 => NodeScript::holding(&b, SLOW_MS),
        8..10 => NodeScript::empty(FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            adoptions: 0,
            conflicted: false,
            ..
        }
    );
    report("superseding write over a split", &outcome, expected)
}

/// Our value conflicts at the network's seq, but the whole first wave holds the network's value
async fn check_conflict_with_holders_in_first_wave() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let c = make_value(b"C", ORDINARY_SEQ.into());
    let outcome = run_scripted_set("holders_first", &c, |rank| match rank {
        0..8 => NodeScript::holding(&a, FAST_MS),
        8..10 => NodeScript::empty(FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            adoptions: 1,
            final_value,
            conflicted: false,
            ..
        } if final_value.value_data() == a.value_data()
    );
    report(
        "equal-seq conflict, first wave all holders",
        &outcome,
        expected,
    )
}

/// Four nodes hold one value and four another at the top seq, and ours is a third
async fn check_split_at_top_seq() -> Result<(), String> {
    let a = make_value(b"A", ValueSeqNum::MAX);
    let b = make_value(b"B", ValueSeqNum::MAX);
    let c = make_value(b"C", ValueSeqNum::MAX);
    let outcome = run_scripted_set("split_at_top", &c, |rank| match rank {
        0..4 => NodeScript::holding(&a, FAST_MS),
        4..8 => NodeScript::holding(&b, SLOW_MS),
        _ => NodeScript::far(),
    })
    .await;
    // A 4-4 tie goes to the closer holder, and rank 0 holds A
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Exhausted,
            consensus: 4,
            mismatched: 0,
            final_value,
            elapsed,
            conflicted: true,
            ..
        } if final_value.value_data() == a.value_data() && *elapsed < QUICK
    );
    report("4/4 split at MAX", &outcome, expected)
}

/// No split beforehand: the first wave stores our value on the empty nodes while the holders
/// answer with theirs
async fn check_split_planted_by_first_wave() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let c = make_value(b"C", ORDINARY_SEQ.into());
    let outcome = run_scripted_set("planted", &c, |rank| match rank {
        0..8 if rank % 2 == 0 => NodeScript::holding(&a, SLOW_MS),
        0..8 => NodeScript::empty(FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    // Our C ties A at four holders each, and A's closest holder is closer
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Exhausted,
            consensus: 4,
            mismatched: 0,
            final_value,
            elapsed,
            conflicted: true,
            ..
        } if final_value.value_data() == a.value_data() && *elapsed < QUICK
    );
    report(
        "split planted by our own first wave, ordinary seq",
        &outcome,
        expected,
    )
}

/// A slow empty node stores our value after the context adopted the network's, and its answer
/// still counts toward the network's value
async fn check_superseded_accept_not_counted() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let c = make_value(b"C", ORDINARY_SEQ.into());
    let outcome = run_scripted_set("superseded_accept", &c, |rank| match rank {
        0..4 => NodeScript::holding(&a, FAST_MS),
        4 => NodeScript::empty(SLOW_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Exhausted,
            consensus: 4,
            mismatched: 0,
            final_value,
            elapsed,
            conflicted: true,
            ..
        } if final_value.value_data() == a.value_data() && *elapsed < QUICK
    );
    report("accept of a superseded value", &outcome, expected)
}

/// One fast node holds a value that five closer, slower nodes contradict, and the majority's
/// value must win
async fn check_majority_beats_fast_minority() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let c = make_value(b"C", ORDINARY_SEQ.into());
    let outcome = run_scripted_set("fast_minority", &c, |rank| match rank {
        0..5 => NodeScript::holding(&b, SLOW_MS),
        5 => NodeScript::holding(&a, FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            elapsed,
            conflicted: true,
            ..
        } if final_value.value_data() == b.value_data() && *elapsed < QUICK
    );
    report("majority against a fast minority", &outcome, expected)
}

/// The closest node alone holds A and the next five hold B, so the majority beats closeness
async fn check_majority_beats_closer_holder() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let c = make_value(b"C", ORDINARY_SEQ.into());
    let outcome = run_scripted_set("closer_minority", &c, |rank| match rank {
        0 => NodeScript::holding(&a, FAST_MS),
        1..6 => NodeScript::holding(&b, SLOW_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            elapsed,
            conflicted: true,
            ..
        } if final_value.value_data() == b.value_data() && *elapsed < QUICK
    );
    report("majority against a closer holder", &outcome, expected)
}

/// Two of the closest five answer with an older seq after a newer value was adopted and the next
/// five time out, so the empty nodes beyond them must still be asked
async fn check_stale_answers_do_not_stall_dispatch() -> Result<(), String> {
    let c = make_value(b"C", ORDINARY_SEQ.into());
    let e = make_value(b"E", (ORDINARY_SEQ + 1).into());
    let d = make_value(b"D", (ORDINARY_SEQ + 2).into());
    let outcome = run_scripted_set("stale_quota", &c, |rank| match rank {
        0 => NodeScript::holding(&d, FAST_MS),
        1..3 => NodeScript::holding(&e, MID_MS),
        3..5 => NodeScript::holding(&d, MID_MS),
        5..10 => NodeScript::unreachable(SLOW_MS),
        10..15 => NodeScript::empty(FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            elapsed,
            conflicted: false,
            ..
        } if final_value.value_data() == d.value_data() && *elapsed < QUICK
    );
    report(
        "stale answers filling the throttle quota",
        &outcome,
        expected,
    )
}

/// Nodes left on an older seq while E was current take F once it is adopted
async fn check_stale_nodes_take_newer_value() -> Result<(), String> {
    let c = make_value(b"C", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let e = make_value(b"E", (ORDINARY_SEQ + 1).into());
    let f = make_value(b"F", (ORDINARY_SEQ + 2).into());
    let outcome = run_scripted_set("stale_reask", &c, |rank| match rank {
        0 => NodeScript::holding(&e, FAST_MS),
        1 => NodeScript::holding(&b, MID_MS),
        2 => NodeScript::holding(&f, SLOW_MS),
        3..5 => NodeScript::empty(MID_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            consensus: 5,
            mismatched: 0,
            final_value,
            elapsed,
            conflicted: false,
            ..
        } if final_value.value_data() == f.value_data() && *elapsed < QUICK
    );
    report("stale nodes offered a newer value", &outcome, expected)
}

/// Our value lands on five nodes while two hold a rival at the same seq: we keep our value and
/// still report the contest
async fn check_contested_win_is_reported() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let c = make_value(b"C", ORDINARY_SEQ.into());
    let outcome = run_scripted_set("contested_win", &c, |rank| match rank {
        0 | 2 | 4 => NodeScript::empty(FAST_MS),
        1 | 3 => NodeScript::holding(&a, MID_MS),
        6 | 7 => NodeScript::empty(SLOW_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            elapsed,
            conflicted: true,
            ..
        } if final_value.value_data() == c.value_data() && *elapsed < QUICK
    );
    report("our value winning a contest", &outcome, expected)
}

fn classify(
    ctx: &mut set_value::OutboundSetValueContext,
    registry: &VeilidComponentRegistry,
    node_id: &NodeId,
    sent: &Arc<SignedValueData>,
    returned: Option<&Arc<SignedValueData>>,
) -> FanoutCallDisposition {
    ctx.classify_accepted_answer(registry, node_id, sent, returned.map(|v| (**v).clone()))
}

/// The classifier's rulings, one answer at a time, against nodes ranked by closeness
pub async fn test_classifier() {
    let registry = mock_registry::init("set_value_classifier").await;
    {
        let target = fix_typed_node_id(CRYPTO_KIND_VLD0, 0).to_hash_coordinate();
        let mut n: Vec<NodeId> = (1..=6u8)
            .map(|idx| fix_typed_node_id(CRYPTO_KIND_VLD0, idx))
            .collect();
        n.sort_unstable_by(RoutingTable::make_closest_node_id_sort(target.clone()));

        let c = make_value(b"C", ORDINARY_SEQ.into());
        let a = make_value(b"A", ORDINARY_SEQ.into());
        let b = make_value(b"B", ORDINARY_SEQ.into());
        let d = make_value(b"D", (ORDINARY_SEQ + 1).into());
        let e = make_value(b"E", (ORDINARY_SEQ + 1).into());
        let mut ctx = set_value::OutboundSetValueContext::new(c.clone(), target);
        let r = &registry;

        // Our value lands on n3
        let disp = classify(&mut ctx, r, &n[3], &c, None);
        assert!(matches!(disp, FanoutCallDisposition::Accepted));

        // A rival with as many holders but a farther one stays a conflict
        let disp = classify(&mut ctx, r, &n[4], &c, Some(&a));
        assert!(matches!(disp, FanoutCallDisposition::Conflict));

        // A second holder puts the rival ahead
        let disp = classify(&mut ctx, r, &n[1], &c, Some(&a));
        assert!(matches!(disp, FanoutCallDisposition::AcceptedSwitch));
        assert_eq!(ctx.value.value_data(), a.value_data());

        // A node that stored our earlier value holds it, tied with A but farther
        let disp = classify(&mut ctx, r, &n[2], &c, None);
        assert!(matches!(disp, FanoutCallDisposition::Conflict));

        // A third holder of our earlier value takes the lead back
        let disp = classify(&mut ctx, r, &n[0], &c, None);
        assert!(matches!(disp, FanoutCallDisposition::AcceptedSwitch));
        assert_eq!(ctx.value.value_data(), c.value_data());

        // A lone rival behind three holders is a conflict
        let disp = classify(&mut ctx, r, &n[5], &c, Some(&b));
        assert!(matches!(disp, FanoutCallDisposition::Conflict));

        // A newer seq restarts
        let disp = classify(&mut ctx, r, &n[5], &c, Some(&d));
        assert!(matches!(disp, FanoutCallDisposition::AcceptedNewerRestart));
        assert_eq!(ctx.value.value_data(), d.value_data());

        // A node that stored a value we have since moved past is behind
        let disp = classify(&mut ctx, r, &n[3], &c, None);
        assert!(matches!(disp, FanoutCallDisposition::Stale));

        // So is a node returning a value older than ours
        let disp = classify(&mut ctx, r, &n[4], &c, Some(&a));
        assert!(matches!(disp, FanoutCallDisposition::Stale));

        // Holders at the old seq no longer count: a closer rival at the new seq wins the tie
        let disp = classify(&mut ctx, r, &n[0], &c, Some(&e));
        assert!(matches!(disp, FanoutCallDisposition::AcceptedSwitch));
        assert_eq!(ctx.value.value_data(), e.value_data());
    }
    mock_registry::terminate(registry).await;
}

/// A fast node answers first with a value the slower majority contradicts, and the majority wins
async fn check_get_majority_beats_fast_minority() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let outcome = run_scripted_get("fast_minority", None, |rank| match rank {
        0..3 => NodeScript::holding(&b, SLOW_MS),
        3 => NodeScript::holding(&a, FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            conflicted: true,
            adoptions,
            max_asks,
            elapsed,
            ..
        } if final_value.value_data() == b.value_data()
            && *max_asks <= 1 + *adoptions
            && *elapsed < QUICK
    );
    report("get: majority against a fast minority", &outcome, expected)
}

/// The closest node alone holds A and the next three hold B: the majority wins, and the B holder
/// that answered before the switch is asked once more so it counts
async fn check_get_majority_beats_closer_holder() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let outcome = run_scripted_get("closer_minority", None, |rank| match rank {
        0 => NodeScript::holding(&a, FAST_MS),
        1..4 => NodeScript::holding(&b, SLOW_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            conflicted: true,
            max_asks: 2,
            elapsed,
            ..
        } if final_value.value_data() == b.value_data() && *elapsed < QUICK
    );
    report("get: majority against a closer holder", &outcome, expected)
}

/// Two nodes hold each value, and the value with the closer holder wins the tie
async fn check_get_tie_goes_to_closer_holder() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let outcome = run_scripted_get("tie", None, |rank| match rank {
        0 | 3 => NodeScript::holding(&a, SLOW_MS),
        1 | 2 => NodeScript::holding(&b, FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Exhausted,
            consensus: 2,
            mismatched: 0,
            final_value,
            conflicted: true,
            adoptions,
            max_asks,
            elapsed,
            ..
        } if final_value.value_data() == a.value_data()
            && *max_asks <= 1 + *adoptions
            && *elapsed < QUICK
    );
    report("get: 2-2 tie goes to the closer holder", &outcome, expected)
}

/// The local copy holds A and every node holds B: no node stands behind A, so B replaces it
async fn check_get_network_replaces_local_copy() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let outcome = run_scripted_get("replace_local", Some(&a), |rank| match rank {
        0..4 => NodeScript::holding(&b, FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            conflicted: false,
            adoptions: 1,
            max_asks: 1,
            elapsed,
            ..
        } if final_value.value_data() == b.value_data() && *elapsed < QUICK
    );
    report(
        "get: the network's value against a local copy",
        &outcome,
        expected,
    )
}

/// The local copy holds A, the closest node B and the next three A: A keeps its place
async fn check_get_local_copy_kept_by_majority() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let outcome = run_scripted_get("keep_local", Some(&a), |rank| match rank {
        0 => NodeScript::holding(&b, FAST_MS),
        1..4 => NodeScript::holding(&a, SLOW_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            conflicted: true,
            adoptions: 2,
            max_asks: 2,
            elapsed,
            ..
        } if final_value.value_data() == a.value_data() && *elapsed < QUICK
    );
    report("get: a local copy the majority holds", &outcome, expected)
}

/// Two nodes hold A and one B at one seq, and the nodes behind them D at the next: D wins
async fn check_get_newer_seq_beats_majority() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let b = make_value(b"B", ORDINARY_SEQ.into());
    let d = make_value(b"D", (ORDINARY_SEQ + 1).into());
    let outcome = run_scripted_get("newer_seq", None, |rank| match rank {
        0 | 1 => NodeScript::holding(&a, FAST_MS),
        2 => NodeScript::holding(&b, FAST_MS),
        3..7 => NodeScript::holding(&d, SLOW_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            conflicted: false,
            max_asks: 1,
            elapsed,
            ..
        } if final_value.value_data() == d.value_data() && *elapsed < QUICK
    );
    report(
        "get: a newer seq against a same-seq majority",
        &outcome,
        expected,
    )
}

/// The closest nodes hold nothing, and the value behind them is still found
async fn check_get_finds_value_past_empty_nodes() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let outcome = run_scripted_get("past_empty", None, |rank| match rank {
        0..4 => NodeScript::empty(FAST_MS),
        4..7 => NodeScript::holding(&a, SLOW_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            conflicted: false,
            adoptions: 0,
            elapsed,
            ..
        } if final_value.value_data() == a.value_data() && *elapsed < QUICK
    );
    report("get: a value behind empty nodes", &outcome, expected)
}

/// A first get with no local copy settles on the value every node holds
async fn check_get_first_get_settles() -> Result<(), String> {
    let a = make_value(b"A", ORDINARY_SEQ.into());
    let outcome = run_scripted_get("first_get", None, |rank| match rank {
        0..5 => NodeScript::holding(&a, FAST_MS),
        _ => NodeScript::far(),
    })
    .await;
    let expected = matches!(
        &outcome,
        RunOutcome::Returned {
            kind: FanoutResultKind::Consensus,
            mismatched: 0,
            final_value,
            conflicted: false,
            adoptions: 0,
            max_asks: 1,
            elapsed,
            ..
        } if final_value.value_data() == a.value_data() && *elapsed < QUICK
    );
    report("get: first get, no local copy", &outcome, expected)
}

fn settled(ctx: &get_value::OutboundGetValueContext) -> &EncryptedValueData {
    ctx.value.as_ref().unwrap_or_log().value_data()
}

/// The get classifier's rulings, one answer at a time, against nodes ranked by closeness
pub async fn test_get_classifier() {
    let registry = mock_registry::init("get_value_classifier").await;
    {
        let target = fix_typed_node_id(CRYPTO_KIND_VLD0, 0).to_hash_coordinate();
        let mut n: Vec<NodeId> = (1..=6u8)
            .map(|idx| fix_typed_node_id(CRYPTO_KIND_VLD0, idx))
            .collect();
        n.sort_unstable_by(RoutingTable::make_closest_node_id_sort(target.clone()));

        let c = make_value(b"C", ORDINARY_SEQ.into());
        let a = make_value(b"A", ORDINARY_SEQ.into());
        let o = make_value(b"O", (ORDINARY_SEQ - 1).into());
        let d = make_value(b"D", (ORDINARY_SEQ + 1).into());
        let e = make_value(b"E", (ORDINARY_SEQ + 1).into());
        let new_ctx = |seed: Option<&Arc<SignedValueData>>| {
            get_value::OutboundGetValueContext::try_new(
                GetResult {
                    opt_value: seed.cloned(),
                    opt_descriptor: None,
                },
                target.clone(),
            )
            .expect("should make the get context")
        };
        let r = &registry;
        let classify = |ctx: &mut get_value::OutboundGetValueContext,
                        node_id: &NodeId,
                        value: &Arc<SignedValueData>| {
            ctx.classify_answer(r, node_id, (**value).clone())
        };

        // With no local copy the first answer is the value so far
        let mut ctx = new_ctx(None);
        let disp = classify(&mut ctx, &n[3], &c);
        assert!(matches!(disp, FanoutCallDisposition::Accepted));

        // A rival with as many holders but a farther one stays a conflict
        let disp = classify(&mut ctx, &n[4], &a);
        assert!(matches!(disp, FanoutCallDisposition::Conflict));
        assert!(ctx.conflicted());

        // A second holder puts the rival ahead
        let disp = classify(&mut ctx, &n[1], &a);
        assert!(matches!(disp, FanoutCallDisposition::AcceptedSwitch));
        assert_eq!(settled(&ctx), a.value_data());

        // C ties A at two holders each, and A's closest holder is closer
        let disp = classify(&mut ctx, &n[2], &c);
        assert!(matches!(disp, FanoutCallDisposition::Conflict));

        // A third holder of C takes the lead back
        let disp = classify(&mut ctx, &n[0], &c);
        assert!(matches!(disp, FanoutCallDisposition::AcceptedSwitch));
        assert_eq!(settled(&ctx), c.value_data());

        // A node asked again does not count twice
        let disp = classify(&mut ctx, &n[4], &a);
        assert!(matches!(disp, FanoutCallDisposition::Conflict));

        // A newer seq starts the tally over
        let disp = classify(&mut ctx, &n[5], &d);
        assert!(matches!(disp, FanoutCallDisposition::AcceptedNewer));
        assert_eq!(settled(&ctx), d.value_data());
        assert!(!ctx.conflicted());

        // An older seq is behind
        let disp = classify(&mut ctx, &n[4], &a);
        assert!(matches!(disp, FanoutCallDisposition::Stale));

        // Holders at the old seq no longer count: a closer rival at the new seq wins the tie
        let disp = classify(&mut ctx, &n[0], &e);
        assert!(matches!(disp, FanoutCallDisposition::AcceptedSwitch));
        assert_eq!(settled(&ctx), e.value_data());

        // A local copy only sets the floor, so the first rival replaces it
        let mut ctx = new_ctx(Some(&a));
        let disp = classify(&mut ctx, &n[2], &c);
        assert!(matches!(disp, FanoutCallDisposition::AcceptedSwitch));
        assert_eq!(settled(&ctx), c.value_data());

        // A node holding the local copy's value ties C, with a farther holder
        let disp = classify(&mut ctx, &n[4], &a);
        assert!(matches!(disp, FanoutCallDisposition::Conflict));

        // A second holder brings it back
        let disp = classify(&mut ctx, &n[3], &a);
        assert!(matches!(disp, FanoutCallDisposition::AcceptedSwitch));
        assert_eq!(settled(&ctx), a.value_data());

        // A seq older than the local copy is behind
        let disp = classify(&mut ctx, &n[5], &o);
        assert!(matches!(disp, FanoutCallDisposition::Stale));

        // A node holding the local copy's value counts for it
        let mut ctx = new_ctx(Some(&a));
        let disp = classify(&mut ctx, &n[5], &a);
        assert!(matches!(disp, FanoutCallDisposition::Accepted));
        assert!(!ctx.conflicted());
    }
    mock_registry::terminate(registry).await;
}

pub async fn test_all() {
    test_classifier().await;
    test_get_classifier().await;

    let failures: Vec<String> = [
        check_superseding_write_over_split().await,
        check_conflict_with_holders_in_first_wave().await,
        check_split_at_top_seq().await,
        check_split_planted_by_first_wave().await,
        check_superseded_accept_not_counted().await,
        check_majority_beats_fast_minority().await,
        check_majority_beats_closer_holder().await,
        check_stale_answers_do_not_stall_dispatch().await,
        check_stale_nodes_take_newer_value().await,
        check_contested_win_is_reported().await,
        check_get_majority_beats_fast_minority().await,
        check_get_majority_beats_closer_holder().await,
        check_get_tie_goes_to_closer_holder().await,
        check_get_network_replaces_local_copy().await,
        check_get_local_copy_kept_by_majority().await,
        check_get_newer_seq_beats_majority().await,
        check_get_finds_value_past_empty_nodes().await,
        check_get_first_get_settles().await,
    ]
    .into_iter()
    .filter_map(Result::err)
    .collect();
    assert!(
        failures.is_empty(),
        "fanout scenarios failed:\n{}",
        failures.join("\n")
    );
}
