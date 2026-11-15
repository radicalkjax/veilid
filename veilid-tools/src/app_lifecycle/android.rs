use super::*;

/// Android's lowest oom adjustment for a cached process, `ProcessList.CACHED_APP_MIN_ADJ`:
/// the band the freezer and the low memory killer act on
const CACHED_APP_MIN_ADJ: i32 = 900;

/// Android writes each process's priority into the kernel as its oom adjustment, and a
/// process may read its own. Entering the cached band is the last change before the freezer
/// takes the process, some seconds later, so it marks the moment to write state out.
pub(super) fn app_lifecycle_state() -> Option<AppLifecycleState> {
    let adj: i32 = std::fs::read_to_string("/proc/self/oom_score_adj")
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(if adj >= CACHED_APP_MIN_ADJ {
        AppLifecycleState::Background
    } else {
        AppLifecycleState::Active
    })
}
