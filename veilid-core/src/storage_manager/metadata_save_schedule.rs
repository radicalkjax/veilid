use super::*;

/// Why the storage metadata was saved
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataSaveReason {
    /// Nothing else saved it for a while
    Periodic,
    /// It changed while the app was in the background
    #[cfg(feature = "app-lifecycle")]
    Background,
}

/// When the storage metadata next goes to disk.
///
/// Periodically, and in the background as soon as it changes: there the platform may suspend the
/// process within seconds, and a kill while suspended loses what only the periodic save carries.
#[derive(Debug)]
pub(crate) struct MetadataSaveSchedule {
    /// Longest the metadata goes unsaved
    period: TimestampDuration,
    /// When the last save started
    last_save_ts: Timestamp,
    /// Whether the metadata changed since the last save started
    dirty: bool,
    /// Whether the app is in the background
    #[cfg(feature = "app-lifecycle")]
    background: bool,
}

impl MetadataSaveSchedule {
    pub fn new(cur_ts: Timestamp, period: TimestampDuration) -> Self {
        Self {
            period,
            last_save_ts: cur_ts,
            dirty: false,
            #[cfg(feature = "app-lifecycle")]
            background: false,
        }
    }

    /// The metadata changed
    pub fn changed(&mut self) {
        self.dirty = true;
    }

    /// The app moved in or out of the background
    #[cfg(feature = "app-lifecycle")]
    pub fn set_background(&mut self, background: bool) {
        self.background = background;
    }

    /// Whether the app is in the background
    #[cfg(feature = "app-lifecycle")]
    pub fn is_background(&self) -> bool {
        self.background
    }

    /// Why a save is due, if one is
    pub fn due(&self, cur_ts: Timestamp) -> Option<MetadataSaveReason> {
        #[cfg(feature = "app-lifecycle")]
        if self.background && self.dirty {
            return Some(MetadataSaveReason::Background);
        }
        (cur_ts.duration_since(self.last_save_ts) >= self.period)
            .then_some(MetadataSaveReason::Periodic)
    }

    /// A save is starting, and covers every change so far
    pub fn saving(&mut self, cur_ts: Timestamp) {
        self.last_save_ts = cur_ts;
        self.dirty = false;
    }
}
