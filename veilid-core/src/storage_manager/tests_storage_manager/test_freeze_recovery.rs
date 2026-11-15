//! A freeze is a stall that began with the app in the background on a platform that freezes
//! it. One sweep per background period, when the app is active again; nothing for a foreground
//! stall, a desktop stall, or a second stall in the same period.

use super::*;

fn t(secs: u64) -> Timestamp {
    Timestamp::new(secs * 1_000_000)
}

/// The sweep waits for the foreground and runs once there
fn test_freeze_in_background_sweeps_on_return() {
    let mut recovery = FreezeRecovery::new(true);
    assert!(!recovery.note_lifecycle(true, t(10)));
    assert!(
        !recovery.note_stall(t(20), t(50)),
        "still in the background, the sweep waits"
    );
    assert!(
        recovery.note_lifecycle(false, t(60)),
        "back in the foreground, one sweep"
    );
    assert!(!recovery.note_lifecycle(false, t(61)), "and only one");
}

/// Two stalls in one background period are one sweep
fn test_two_stalls_one_sweep() {
    let mut recovery = FreezeRecovery::new(true);
    recovery.note_lifecycle(true, t(10));
    assert!(!recovery.note_stall(t(20), t(25)));
    assert!(!recovery.note_stall(t(40), t(45)));
    assert!(recovery.note_lifecycle(false, t(60)));
    assert!(
        !recovery.note_stall(t(30), t(35)),
        "a late report of the same period is spent"
    );
}

/// A stall reported after the move to the foreground, as the handlers may be ordered, sweeps
/// at once when it began while put away, and never when it began in the foreground
fn test_stall_reported_after_foreground() {
    let mut recovery = FreezeRecovery::new(true);
    recovery.note_lifecycle(true, t(10));
    assert!(!recovery.note_lifecycle(false, t(60)));
    assert!(recovery.note_stall(t(20), t(59)), "began while put away");
    assert!(
        !recovery.note_stall(t(25), t(30)),
        "same period, already swept"
    );

    let mut recovery = FreezeRecovery::new(true);
    recovery.note_lifecycle(true, t(10));
    recovery.note_lifecycle(false, t(60));
    assert!(!recovery.note_stall(t(70), t(75)), "a foreground stall");
}

/// Without a background period, or over before one, a stall is the executor's problem
fn test_foreground_stall_does_not_sweep() {
    let mut recovery = FreezeRecovery::new(true);
    assert!(!recovery.note_stall(t(20), t(22)));
    recovery.note_lifecycle(true, t(30));
    assert!(
        !recovery.note_stall(t(25), t(28)),
        "over before the move to the background"
    );
    assert!(!recovery.note_lifecycle(false, t(40)));
}

/// The freeze iOS applies a fraction of a second after the move to the background: the
/// watchdog dates the stall from its last on-time wake, before the period was even stamped
fn test_freeze_right_after_backgrounding() {
    let entered = Timestamp::new(10_375_000);

    let mut recovery = FreezeRecovery::new(true);
    recovery.note_lifecycle(true, entered);
    assert!(!recovery.note_stall(t(10), t(81)), "the sweep waits");
    assert!(recovery.note_lifecycle(false, t(81)), "and runs on return");

    // The handlers in the other order
    let mut recovery = FreezeRecovery::new(true);
    recovery.note_lifecycle(true, entered);
    assert!(!recovery.note_lifecycle(false, t(81)));
    assert!(recovery.note_stall(t(10), t(81)));
}

/// A desktop platform only throttles: no sweep, however the app moves
fn test_non_freezing_platform_never_sweeps() {
    let mut recovery = FreezeRecovery::new(false);
    recovery.note_lifecycle(true, t(10));
    assert!(!recovery.note_stall(t(20), t(50)));
    assert!(!recovery.note_lifecycle(false, t(60)));
}

/// Each background period is its own freeze
fn test_each_background_period_sweeps_again() {
    let mut recovery = FreezeRecovery::new(true);
    recovery.note_lifecycle(true, t(10));
    recovery.note_stall(t(20), t(50));
    assert!(recovery.note_lifecycle(false, t(60)));
    recovery.note_lifecycle(true, t(100));
    recovery.note_stall(t(120), t(150));
    assert!(recovery.note_lifecycle(false, t(160)));
}

pub fn test_all() {
    test_freeze_in_background_sweeps_on_return();
    test_two_stalls_one_sweep();
    test_stall_reported_after_foreground();
    test_foreground_stall_does_not_sweep();
    test_freeze_right_after_backgrounding();
    test_non_freezing_platform_never_sweeps();
    test_each_background_period_sweeps_again();
}
