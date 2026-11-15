//! Deliveries lost to an OS freeze. A platform that freezes a backgrounded process stops its
//! sockets with it: a ValueChanged sent meanwhile is gone, and the storing node's retry is
//! minutes out. The process watchdog measures the stall; the host's lifecycle says whether the
//! process was put away when it began. One sweep of every watch per background period, run once
//! the app is active again: a throttled hidden browser tab stalls every minute, and a background
//! wake the user never sees should not spend the inspection budget.

use super::*;

/// Whether the platform freezes a backgrounded process; without lifecycle reports nothing is
/// ever put away
pub(super) fn platform_may_freeze() -> bool {
    #[cfg(feature = "app-lifecycle")]
    let may_freeze = background_may_freeze();
    #[cfg(not(feature = "app-lifecycle"))]
    let may_freeze = false;
    may_freeze
}

/// One background period of the host application
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BackgroundPeriod {
    entered: Timestamp,
    left: Option<Timestamp>,
    /// A sweep already ran or is pending for this period
    swept: bool,
}

/// Decides when a freeze is worth a change inspection of every watch
#[derive(Debug)]
pub(super) struct FreezeRecovery {
    /// Whether the platform freezes a backgrounded process at all
    may_freeze: bool,
    /// The latest background period, open while `left` is None
    background: Option<BackgroundPeriod>,
    /// A stall began in the open background period; the sweep waits for the foreground
    sweep_pending: bool,
}

impl FreezeRecovery {
    pub fn new(may_freeze: bool) -> Self {
        Self {
            may_freeze,
            background: None,
            sweep_pending: false,
        }
    }

    /// The host moved to or from the background; true when a sweep is due now
    // Only lifecycle reports and the tests call this; the reports need the app-lifecycle feature
    #[cfg_attr(
        not(any(feature = "app-lifecycle", feature = "test-util", test)),
        expect(dead_code)
    )]
    pub fn note_lifecycle(&mut self, background: bool, now: Timestamp) -> bool {
        if background {
            if self.background.is_none_or(|period| period.left.is_some()) {
                self.background = Some(BackgroundPeriod {
                    entered: now,
                    left: None,
                    swept: false,
                });
            }
            return false;
        }
        if let Some(period) = &mut self.background {
            if period.left.is_none() {
                period.left = Some(now);
            }
        }
        core::mem::take(&mut self.sweep_pending)
    }

    /// A stall the process watchdog measured; true when a sweep is due now. A stall counts when
    /// it overlaps the background period: the watchdog dates it from its last on-time wake, up to
    /// a tick before the freeze, and the period starts when its report was handled, so a freeze
    /// right after the move to the background can begin "before" the period it froze
    pub fn note_stall(&mut self, began: Timestamp, ended: Timestamp) -> bool {
        if !self.may_freeze {
            return false;
        }
        let Some(period) = &mut self.background else {
            return false;
        };
        if period.swept || ended <= period.entered {
            return false;
        }
        match period.left {
            // Still put away: the sweep waits for the foreground
            None => {
                period.swept = true;
                self.sweep_pending = true;
                false
            }
            Some(left) => {
                if began >= left {
                    return false;
                }
                period.swept = true;
                true
            }
        }
    }
}
