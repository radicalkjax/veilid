use super::*;

const ACTIVE: Option<VeilidAppLifecycleState> = Some(VeilidAppLifecycleState::Active);
const BACKGROUND: Option<VeilidAppLifecycleState> = Some(VeilidAppLifecycleState::Background);

pub fn test_detection_wins_where_it_reports() {
    assert_eq!(
        effective_app_lifecycle_state(ACTIVE, BACKGROUND, true),
        ACTIVE
    );
    assert_eq!(
        effective_app_lifecycle_state(BACKGROUND, ACTIVE, true),
        BACKGROUND
    );
}

pub fn test_report_fills_a_gap() {
    assert_eq!(
        effective_app_lifecycle_state(None, BACKGROUND, true),
        BACKGROUND
    );
    assert_eq!(effective_app_lifecycle_state(None, None, true), None);
}

pub fn test_report_alone_without_detection() {
    assert_eq!(
        effective_app_lifecycle_state(ACTIVE, BACKGROUND, false),
        BACKGROUND
    );
    assert_eq!(effective_app_lifecycle_state(BACKGROUND, None, false), None);
}
