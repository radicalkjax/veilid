use crate::routing_table::*;

// `secs` seconds past a base far enough from zero that nothing underflows; a new schedule
// counts as saved at its creation
fn ts(secs: u64) -> Timestamp {
    Timestamp::new(1_000_000_000u64 + secs * 1_000_000u64)
}

pub fn test_save_schedule_change_waits_to_settle() {
    info!("--- test_save_schedule_change_waits_to_settle ---");

    let mut schedule = SaveSchedule::new(ts(0), SaveTimings::default());
    schedule.changed(ts(20));
    assert!(schedule.is_dirty());
    assert_eq!(schedule.due(ts(24)), None);
    assert_eq!(schedule.due(ts(25)), Some(SaveReason::Changed));

    // Each change restarts the wait
    schedule.changed(ts(24));
    assert_eq!(schedule.due(ts(28)), None);
    assert_eq!(schedule.due(ts(29)), Some(SaveReason::Changed));
}

/// Changes that never settle hold back an urgent one only for the least interval between saves
pub fn test_save_schedule_urgent_change_skips_settle() {
    info!("--- test_save_schedule_urgent_change_skips_settle ---");

    let mut schedule = SaveSchedule::new(ts(0), SaveTimings::default());
    for secs in 1..=12 {
        schedule.changed(ts(secs));
        assert_eq!(
            schedule.due(ts(secs)),
            None,
            "a change every second never settles"
        );
    }
    schedule.urgent_change(ts(12));
    assert_eq!(schedule.due(ts(12)), Some(SaveReason::Changed));

    // The save covers it; what follows settles again
    schedule.saving(ts(12));
    schedule.changed(ts(20));
    assert_eq!(
        schedule.due(ts(22)),
        None,
        "a plain change after the save waits to settle"
    );
    assert_eq!(schedule.due(ts(25)), Some(SaveReason::Changed));
}

pub fn test_save_schedule_change_waits_for_min_interval() {
    info!("--- test_save_schedule_change_waits_for_min_interval ---");

    let mut schedule = SaveSchedule::new(ts(0), SaveTimings::default());
    schedule.changed(ts(1));
    // Settled at 6s, but the last save was at 0s
    assert_eq!(schedule.due(ts(6)), None);
    assert_eq!(schedule.due(ts(10)), Some(SaveReason::Changed));
}

pub fn test_save_schedule_periodic_without_changes() {
    info!("--- test_save_schedule_periodic_without_changes ---");

    let schedule = SaveSchedule::new(ts(0), SaveTimings::default());
    assert!(!schedule.is_dirty());
    assert_eq!(schedule.due(ts(299)), None);
    assert_eq!(schedule.due(ts(300)), Some(SaveReason::Periodic));
}

pub fn test_save_schedule_state_changes_add_up() {
    info!("--- test_save_schedule_state_changes_add_up ---");

    let mut schedule = SaveSchedule::new(ts(0), SaveTimings::default());
    for _ in 0..15 {
        schedule.state_changed(ts(20));
    }
    assert!(!schedule.is_dirty());
    assert_eq!(schedule.due(ts(29)), None);

    schedule.state_changed(ts(20));
    assert!(schedule.is_dirty());
    assert_eq!(schedule.due(ts(25)), Some(SaveReason::Changed));

    // A save starts the count over
    schedule.saving(ts(25));
    schedule.state_changed(ts(26));
    assert!(!schedule.is_dirty());
}

pub fn test_save_schedule_saving_covers_earlier_changes_only() {
    info!("--- test_save_schedule_saving_covers_earlier_changes_only ---");

    let mut schedule = SaveSchedule::new(ts(0), SaveTimings::default());
    schedule.changed(ts(20));
    schedule.saving(ts(25));
    assert!(!schedule.is_dirty());

    // A change landing while the save writes is saved again later
    schedule.changed(ts(25));
    assert!(schedule.is_dirty());
    assert_eq!(schedule.due(ts(35)), Some(SaveReason::Changed));
}

#[cfg(feature = "app-lifecycle")]
pub fn test_save_schedule_background_saves_at_once() {
    info!("--- test_save_schedule_background_saves_at_once ---");

    let mut schedule = SaveSchedule::new(ts(0), SaveTimings::default());
    schedule.set_background(true, ts(20));
    assert_eq!(schedule.due(ts(20)), Some(SaveReason::Background));
    schedule.saving(ts(20));
    assert_eq!(schedule.due(ts(21)), None);

    // A change in the background waits only for the background interval
    schedule.changed(ts(21));
    assert_eq!(schedule.due(ts(24)), None);
    assert_eq!(schedule.due(ts(25)), Some(SaveReason::Background));

    // Staying there is not another change
    schedule.saving(ts(25));
    schedule.set_background(true, ts(26));
    assert!(!schedule.is_dirty());
}

#[cfg(feature = "app-lifecycle")]
pub fn test_save_schedule_return_from_background_restarts_period() {
    info!("--- test_save_schedule_return_from_background_restarts_period ---");

    let mut schedule = SaveSchedule::new(ts(0), SaveTimings::default());
    schedule.set_background(true, ts(20));
    schedule.saving(ts(20));

    // Frozen for five minutes, then back: nothing new to save yet
    schedule.set_background(false, ts(320));
    assert!(!schedule.is_dirty());
    assert_eq!(schedule.due(ts(320)), None);
    assert_eq!(schedule.due(ts(619)), None);
    assert_eq!(schedule.due(ts(620)), Some(SaveReason::Periodic));

    // Back in the foreground, changes settle as usual
    schedule.changed(ts(321));
    assert_eq!(schedule.due(ts(325)), None);
    assert_eq!(schedule.due(ts(326)), Some(SaveReason::Changed));
}
