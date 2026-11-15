//! Rehydration failures are counted per record: its subkeys share their nodes,
//! and a run of failures on one record is what earns the warning.

use super::*;

fn key(n: u8) -> OpaqueRecordKey {
    OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[n; 32]))
}

/// Each failure extends the record's run; records do not share runs.
fn test_failures_extend_the_record_run() {
    let mut runs = RehydrationFailureRuns::new(4);
    assert_eq!(runs.note(key(1)), 1);
    assert_eq!(runs.note(key(1)), 2);
    assert_eq!(runs.note(key(2)), 1);
    assert_eq!(runs.note(key(1)), 3);
    assert_eq!(runs.run(&key(1)), 3);
    assert_eq!(runs.run(&key(2)), 1);
}

/// A success ends the run, and the next failure starts a fresh one.
fn test_success_ends_the_run() {
    let mut runs = RehydrationFailureRuns::new(4);
    runs.note(key(3));
    runs.note(key(3));
    runs.clear(&key(3));
    assert_eq!(runs.run(&key(3)), 0);
    assert_eq!(runs.note(key(3)), 1);
}

pub fn test_all() {
    test_failures_extend_the_record_run();
    test_success_ends_the_run();
}
