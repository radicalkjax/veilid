//! A watch keeps to the get-consensus count of nodes closest to its record, since every extra node
//! sends every change again. Extras and strays are cancelled at their nodes through one queue.

use super::*;

fn make_params() -> OutboundWatchParameters {
    OutboundWatchParameters {
        expiration: Timestamp::new(0),
        count: u32::MAX,
        subkeys: ValueSubkeyRangeSet::single(0),
        opt_watcher: None,
        safety_selection: SafetySelection::Unsafe(Sequencing::PreferOrdered),
    }
}

fn make_record_key(byte: u8) -> RecordKey {
    RecordKey::from_opaque(
        OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[byte; 32])),
        None,
    )
}

fn node_key(record_key: &RecordKey, n: u8) -> PerNodeKey {
    PerNodeKey {
        record_key: record_key.clone(),
        node_id: NodeId::new(CRYPTO_KIND_VLD0, BareNodeId::new(&[n; 32])),
    }
}

fn per_node_state(watch_id: u64, expiration: Timestamp) -> OutboundWatchPerNodeState {
    OutboundWatchPerNodeState {
        watch_id,
        safety_selection: SafetySelection::Unsafe(Sequencing::PreferOrdered),
        opt_watcher: None,
        expiration,
        count: u32::MAX,
        watch_node_ref: None,
        opt_value_changed_route: None,
    }
}

/// A manager watching `record_key` on `nodes`, watch ids counting from 1
fn make_manager(record_key: &RecordKey, nodes: &[PerNodeKey]) -> OutboundWatchManager {
    let mut owm = OutboundWatchManager::new();
    let expiration = Timestamp::now().later(TimestampDuration::new_secs(600));
    for (watch_id, pnk) in (1u64..).zip(nodes) {
        owm.per_node_states
            .insert(pnk.clone(), per_node_state(watch_id, expiration));
    }
    let mut ow = OutboundWatch::new(record_key.clone(), make_params());
    ow.state_mut_or_create(make_params)
        .edit(&owm.per_node_states, |editor| {
            editor.add_nodes(nodes.iter().cloned());
        });
    owm.outbound_watches.insert(record_key.opaque(), ow);
    owm
}

/// A watch on five nodes keeps the three closest to its record and queues the other two
fn test_trim_keeps_the_closest() {
    let record_key = make_record_key(0x5a);
    let nodes: Vec<PerNodeKey> = (1..=5).map(|n| node_key(&record_key, n * 37)).collect();
    let mut owm = make_manager(&record_key, &nodes);

    let coordinate = record_key.to_hash_coordinate();
    let mut by_distance = nodes.clone();
    by_distance.sort_by_cached_key(|pnk| coordinate.distance(&pnk.node_id.to_hash_coordinate()));

    let cur_ts = Timestamp::now();
    let dropped = owm.trim_watch_nodes(&record_key, 3, cur_ts);
    let kept: BTreeSet<PerNodeKey> = owm.outbound_watches[&record_key.opaque()]
        .state()
        .unwrap()
        .nodes()
        .iter()
        .cloned()
        .collect();
    assert_eq!(kept, by_distance[..3].iter().cloned().collect());
    assert_eq!(
        dropped.iter().cloned().collect::<BTreeSet<_>>(),
        by_distance[3..].iter().cloned().collect()
    );
    for pnk in &dropped {
        assert!(
            !owm.per_node_states.contains_key(pnk),
            "a dropped node's watch is no longer ours"
        );
    }
    assert_eq!(owm.take_due_watch_cancels(cur_ts).len(), 2);
    assert!(
        owm.trim_watch_nodes(&record_key, 3, cur_ts).is_empty(),
        "a watch at consensus keeps its nodes"
    );
}

/// A queued cancel waits for its time and goes out once while in flight; one whose watch has
/// become ours meanwhile, or whose record is no longer watched, is dropped
fn test_watch_cancel_queue() {
    let record_key = make_record_key(0x5a);
    let mut owm = make_manager(&record_key, &[]);
    let pnk = node_key(&record_key, 1);
    let now = Timestamp::now();
    let later = now.later(TimestampDuration::new_secs(10));

    owm.queue_watch_cancel(pnk.clone(), 7, later);
    assert!(
        owm.take_due_watch_cancels(now).is_empty(),
        "not before its time"
    );
    let taken = owm.take_due_watch_cancels(later);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].watch_id, 7);
    owm.queue_watch_cancel(pnk.clone(), 7, later);
    assert!(
        owm.take_due_watch_cancels(later).is_empty(),
        "one cancel on the wire at a time"
    );
    owm.finish_watch_cancel(&pnk, 7);
    owm.queue_watch_cancel(pnk.clone(), 7, later);
    assert_eq!(
        owm.take_due_watch_cancels(later).len(),
        1,
        "a stray that pushes again is cancelled again"
    );
    owm.finish_watch_cancel(&pnk, 7);

    owm.per_node_states
        .insert(pnk.clone(), per_node_state(8, later));
    owm.queue_watch_cancel(pnk.clone(), 8, now);
    assert!(
        owm.take_due_watch_cancels(now).is_empty(),
        "a fanout that claimed the watch keeps it"
    );

    let unwatched = node_key(&make_record_key(0x77), 2);
    owm.queue_watch_cancel(unwatched, 9, now);
    assert!(
        owm.take_due_watch_cancels(now).is_empty(),
        "nothing signs a cancel for a record no longer watched"
    );
}

pub fn test_all() {
    test_trim_keeps_the_closest();
    test_watch_cancel_queue();
}
