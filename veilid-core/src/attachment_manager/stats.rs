use super::*;

/// Cumulative time spent in and entries into each attachment state, plus a transition count.
#[derive(Debug, Default)]
pub(super) struct AttachmentStatsAccounting {
    current: Option<(AttachmentState, Timestamp)>,
    per_state: Vec<StateDuration>,
    transitions: u64,
}

#[derive(Debug, Clone, Serialize)]
struct StateDuration {
    state: AttachmentState,
    entries: u64,
    total: TimestampDuration,
}

impl AttachmentStatsAccounting {
    pub(super) fn record_state(&mut self, now: Timestamp, state: AttachmentState) {
        if let Some((cur, since)) = self.current {
            if cur == state {
                return;
            }
            if let Some(e) = self.per_state.iter_mut().find(|e| e.state == cur) {
                e.total = e.total.saturating_add(now.duration_since(since));
            }
            self.transitions += 1;
        }
        self.current = Some((state, now));
        match self.per_state.iter_mut().find(|e| e.state == state) {
            Some(e) => e.entries += 1,
            None => self.per_state.push(StateDuration {
                state,
                entries: 1,
                total: TimestampDuration::new(0),
            }),
        }
    }

    fn snapshot(&self, now: Timestamp) -> Vec<StateDuration> {
        let mut out = self.per_state.clone();
        if let Some((cur, since)) = self.current {
            if let Some(e) = out.iter_mut().find(|e| e.state == cur) {
                e.total = e.total.saturating_add(now.duration_since(since));
            }
        }
        out
    }
}

#[derive(Debug, Serialize)]
pub(super) struct AttachmentManagerStats {
    state: AttachmentState,
    uptime: TimestampDuration,
    attached_uptime: Option<TimestampDuration>,
    state_transitions: u64,
    time_in_state: Vec<StateDuration>,
}

impl AttachmentManager {
    pub(super) fn stats_impl(&self) -> Option<Box<dyn VeilidComponentStats>> {
        let inner = self.inner.lock();
        let now = Timestamp::now();
        Some(Box::new(AttachmentManagerStats {
            state: inner.attachment_state,
            uptime: now.duration_since(inner.started_ts),
            attached_uptime: inner.attach_ts.map(|ts| now.duration_since(ts)),
            state_transitions: inner.stats_accounting.transitions,
            time_in_state: inner.stats_accounting.snapshot(now),
        }))
    }
}
