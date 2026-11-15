use super::*;

impl StorageManager {
    /// Drop the server-side watches that have expired, returning when the next one is due
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all)
    )]
    pub(super) fn check_inbound_watches_timer_routine(
        &self,
        _cur_ts: Timestamp,
    ) -> Option<Timestamp> {
        self.get_remote_record_store()
            .ok()?
            .drop_expired_inbound_watches()
    }
}
