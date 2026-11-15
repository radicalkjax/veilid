//! A change inspection remembers what asked for it. A reconcile that changed the watch's nodes
//! asks for one with fresh routes already in place, so finding changes then must not force
//! another reconcile, or a backlog turns into one inspect-and-reconcile round per subkey.

use super::*;

fn make_record_key() -> RecordKey {
    RecordKey::from_opaque(
        OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[7u8; 32])),
        None,
    )
}

fn make_params() -> OutboundWatchParameters {
    OutboundWatchParameters {
        expiration: Timestamp::new(0),
        count: u32::MAX,
        subkeys: ValueSubkeyRangeSet::full(),
        opt_watcher: None,
        safety_selection: SafetySelection::Unsafe(Sequencing::PreferOrdered),
    }
}

/// Only the watch's own schedule may force a reconcile
pub fn test_reconcile_never_forces_a_reconcile() {
    assert!(ChangeInspectTrigger::Watch.may_force_reconcile());
    assert!(!ChangeInspectTrigger::Reconcile.may_force_reconcile());
}

/// Two requests for one record merge into the one that may still force a reconcile, whichever
/// came first
pub fn test_merged_requests_keep_the_watch_trigger() {
    let now = Timestamp::new(1_000_000_000);
    for (first, second, expected) in [
        (
            ChangeInspectTrigger::Reconcile,
            ChangeInspectTrigger::Watch,
            ChangeInspectTrigger::Watch,
        ),
        (
            ChangeInspectTrigger::Watch,
            ChangeInspectTrigger::Reconcile,
            ChangeInspectTrigger::Watch,
        ),
        (
            ChangeInspectTrigger::Reconcile,
            ChangeInspectTrigger::Reconcile,
            ChangeInspectTrigger::Reconcile,
        ),
    ] {
        let mut owm = OutboundWatchManager::new();
        let record_key = make_record_key();
        owm.set_desired_watch(record_key.clone(), Some(make_params()));
        owm.enqueue_change_inspect_after(
            record_key.clone(),
            ValueSubkeyRangeSet::single(1),
            now,
            first,
        );
        owm.enqueue_change_inspect_after(
            record_key.clone(),
            ValueSubkeyRangeSet::single(2),
            now,
            second,
        );
        let due = owm.take_due_change_inspections(now);
        assert_eq!(due.len(), 1, "one request per record");
        let (key, subkeys, trigger) = &due[0];
        assert_eq!(key, &record_key);
        assert_eq!(subkeys, &ValueSubkeyRangeSet::single_range(1, 2));
        assert_eq!(*trigger, expected, "{first:?} then {second:?}");
    }
}

pub fn test_all() {
    test_reconcile_never_forces_a_reconcile();
    test_merged_requests_keep_the_watch_trigger();
}
