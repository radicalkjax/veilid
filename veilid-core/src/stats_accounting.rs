use super::*;

// Latency entry is per round-trip packet (ping or data)
// - Size is number of entries
const ROLLING_LATENCIES_SIZE: usize = 50;

// Transfers entries are in bytes total for the interval
// - Size is number of entries
// - Interval is number of seconds in each entry
const ROLLING_TRANSFERS_SIZE: usize = 10;
pub const ROLLING_TRANSFERS_INTERVAL_SECS: u32 = 1;

// Per-local-route stats LRU bound. Sized for typical safety-route counts (4–8 per kind)
// with headroom; entries that age out by LRU are gone — we don't actively purge.
pub const PER_ROUTE_STATS_LRU_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransferCount {
    down: ByteCount,
    up: ByteCount,
}

#[derive(Debug, Clone, Default)]
pub struct TransferStatsAccounting {
    rolling_transfers: VecDeque<TransferCount>,
    current_transfer: TransferCount,
}

impl TransferStatsAccounting {
    pub fn new() -> Self {
        Self {
            rolling_transfers: VecDeque::new(),
            current_transfer: TransferCount::default(),
        }
    }

    pub fn add_down(&mut self, bytes: ByteCount) {
        self.current_transfer.down += bytes;
    }

    pub fn add_up(&mut self, bytes: ByteCount) {
        self.current_transfer.up += bytes;
    }

    pub fn roll_transfers(
        &mut self,
        last_ts: Timestamp,
        cur_ts: Timestamp,
        transfer_stats: &mut TransferStatsDownUp,
    ) {
        let dur_ms = cur_ts.duration_since(last_ts).as_u64() / 1000u64;
        while self.rolling_transfers.len() >= ROLLING_TRANSFERS_SIZE {
            self.rolling_transfers.pop_front();
        }
        self.rolling_transfers.push_back(self.current_transfer);

        transfer_stats.down.total += self.current_transfer.down;
        transfer_stats.up.total += self.current_transfer.up;

        self.current_transfer = TransferCount::default();

        transfer_stats.down.maximum = 0.into();
        transfer_stats.up.maximum = 0.into();
        transfer_stats.down.minimum = u64::MAX.into();
        transfer_stats.up.minimum = u64::MAX.into();
        transfer_stats.down.average = 0.into();
        transfer_stats.up.average = 0.into();
        for xfer in &self.rolling_transfers {
            let bpsd = xfer.down * 1000u64 / dur_ms;
            let bpsu = xfer.up * 1000u64 / dur_ms;
            transfer_stats.down.maximum.max_assign(bpsd);
            transfer_stats.up.maximum.max_assign(bpsu);
            transfer_stats.down.minimum.min_assign(bpsd);
            transfer_stats.up.minimum.min_assign(bpsu);
            transfer_stats.down.average += bpsd;
            transfer_stats.up.average += bpsu;
        }
        let len = self.rolling_transfers.len() as u64;
        if len > 0 {
            transfer_stats.down.average /= len;
            transfer_stats.up.average /= len;
        }
    }
}

/// Rolling sample-window accounting for latency measurements.
///
/// Recording is append-only by design: it runs on hot paths (every crypto op,
/// every StatsMutex section) while a lock is held, so computing the
/// [LatencyStats] output happens at read time instead. Readers that keep a
/// cached [LatencyStats] refresh it with [Self::take_latency_stats], which
/// costs nothing while no new samples arrive; rare on-demand snapshots use
/// [Self::latency_stats].
#[derive(Debug, Clone, Default)]
pub struct LatencyStatsAccounting {
    /// The latest samples, oldest first
    rolling_latencies: VecDeque<TimestampDuration>,
    /// A sample arrived since the last take
    dirty: bool,
    /// Samples recorded so far
    recorded: u64,
}

impl LatencyStatsAccounting {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_tm_n(sorted_latencies: &[TimestampDuration], n: usize) -> Option<TimestampDuration> {
        let tmcount = sorted_latencies.len() * n / 100;
        if tmcount == 0 {
            None
        } else {
            let mut tm = TimestampDuration::new(0);
            for l in &sorted_latencies[..tmcount] {
                tm = tm.saturating_add(*l);
            }
            tm = tm.div(u64::try_from(tmcount).ok()?);
            Some(tm)
        }
    }

    fn get_p_n(sorted_latencies: &[TimestampDuration], n: usize) -> TimestampDuration {
        let pindex = (sorted_latencies.len() * n / 100).saturating_sub(1);
        sorted_latencies[pindex]
    }

    pub fn record_latency(&mut self, latency: TimestampDuration) {
        while self.rolling_latencies.len() >= ROLLING_LATENCIES_SIZE {
            self.rolling_latencies.pop_front();
        }
        self.rolling_latencies.push_back(latency);
        self.dirty = true;
        self.recorded += 1;
    }

    pub fn recorded(&self) -> u64 {
        self.recorded
    }

    /// Whether a full window of samples arrived after the first `recorded`
    pub fn window_newer_than(&self, recorded: u64) -> bool {
        self.recorded >= recorded.saturating_add(ROLLING_LATENCIES_SIZE as u64)
    }

    /// Stats over the sample window, or None if nothing has been recorded
    pub fn latency_stats(&self) -> Option<LatencyStats> {
        if self.rolling_latencies.is_empty() {
            return None;
        }
        let mut sorted: Vec<TimestampDuration> = self.rolling_latencies.iter().copied().collect();
        sorted.sort_unstable();

        let fastest = *sorted.first().unwrap();
        let slowest = *sorted.last().unwrap();

        let mut average = TimestampDuration::new(0u64);
        for rl in &sorted {
            average.saturating_add_assign(*rl);
        }
        let len = u64::try_from(sorted.len()).unwrap_or_default();
        if len > 0 {
            average.div_assign(len);
        }

        let tm90 = Self::get_tm_n(&sorted, 90).unwrap_or(average);
        let tm75 = Self::get_tm_n(&sorted, 75).unwrap_or(average);
        let p90 = Self::get_p_n(&sorted, 90);
        let p75 = Self::get_p_n(&sorted, 75);

        Some(LatencyStats {
            fastest,
            average,
            slowest,
            tm90,
            tm75,
            p90,
            p75,
        })
    }

    /// [Self::latency_stats], but None when no sample has arrived since the
    /// last take, so periodic refreshers skip windows that did not change
    pub fn take_latency_stats(&mut self) -> Option<LatencyStats> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        self.latency_stats()
    }
}

/// Per-local-route stats bundle, keyed in an LRU on BucketEntry / RouteStats.
#[derive(Debug, Clone, Default)]
pub struct PerRouteStats {
    /// Full RTT samples (not one-way latency); don't sum with other LatencyStats.
    /// Refreshed once per stats interval by [Self::update_latency_stats].
    pub round_trip: LatencyStats,
    /// Bytes flowing through this (target, local-route) pair.
    pub transfer: TransferStatsDownUp,
    /// Accounting for the round-trip latency
    round_trip_accounting: LatencyStatsAccounting,
    /// Accounting for the transfer statistics
    transfer_accounting: TransferStatsAccounting,
}

impl PerRouteStats {
    pub fn record_round_trip(&mut self, sample: TimestampDuration) {
        self.round_trip_accounting.record_latency(sample);
    }
    pub fn update_latency_stats(&mut self) {
        if let Some(l) = self.round_trip_accounting.take_latency_stats() {
            self.round_trip = l;
        }
    }
    pub fn add_up(&mut self, bytes: ByteCount) {
        self.transfer_accounting.add_up(bytes);
    }
    pub fn add_down(&mut self, bytes: ByteCount) {
        self.transfer_accounting.add_down(bytes);
    }
    pub fn roll_transfers(&mut self, last_ts: Timestamp, cur_ts: Timestamp) {
        self.transfer_accounting
            .roll_transfers(last_ts, cur_ts, &mut self.transfer);
    }
}

impl fmt::Display for PerRouteStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "round_trip: {}", f.to_string(&self.round_trip))?;
        write!(f, "transfer:   {}", f.to_string(&self.transfer))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stats must be what a sort of the rolling window gives.
    #[test]
    fn stats_match_a_full_sort() {
        let mut lsa = LatencyStatsAccounting::new();
        for i in 0..(ROLLING_LATENCIES_SIZE * 2) {
            // Spread, with repeats, so percentiles land on real boundaries
            let v = ((i * 37) % 23) as u64;
            lsa.record_latency(TimestampDuration::new(v));
        }
        assert_eq!(lsa.rolling_latencies.len(), ROLLING_LATENCIES_SIZE);
        let stats = lsa.latency_stats().expect("window has samples");

        let mut sorted: Vec<_> = lsa.rolling_latencies.iter().copied().collect();
        sorted.sort_unstable();
        let mut average = TimestampDuration::new(0u64);
        for rl in &sorted {
            average.saturating_add_assign(*rl);
        }
        average.div_assign(u64::try_from(sorted.len()).unwrap());

        assert_eq!(stats.fastest, *sorted.first().unwrap());
        assert_eq!(stats.slowest, *sorted.last().unwrap());
        assert_eq!(stats.average, average);
        assert_eq!(
            stats.tm90,
            LatencyStatsAccounting::get_tm_n(&sorted, 90).unwrap_or(average)
        );
        assert_eq!(
            stats.tm75,
            LatencyStatsAccounting::get_tm_n(&sorted, 75).unwrap_or(average)
        );
        assert_eq!(stats.p90, LatencyStatsAccounting::get_p_n(&sorted, 90));
        assert_eq!(stats.p75, LatencyStatsAccounting::get_p_n(&sorted, 75));
    }

    /// take_latency_stats yields only when new samples arrived since the last
    /// take, so periodic refreshers skip unchanged windows.
    #[test]
    fn take_skips_unchanged_windows() {
        let mut lsa = LatencyStatsAccounting::new();
        assert!(lsa.take_latency_stats().is_none());

        lsa.record_latency(TimestampDuration::new(5));
        let first = lsa.take_latency_stats().expect("dirty after record");
        assert_eq!(first.fastest, TimestampDuration::new(5));
        assert!(lsa.take_latency_stats().is_none());

        lsa.record_latency(TimestampDuration::new(7));
        assert!(lsa.take_latency_stats().is_some());
        // Non-taking reads never consume the dirty flag
        assert!(lsa.latency_stats().is_some());
        assert!(lsa.take_latency_stats().is_none());
    }

    #[test]
    fn window_turns_over_after_a_full_window_of_samples() {
        let mut lsa = LatencyStatsAccounting::new();
        for _ in 0..7 {
            lsa.record_latency(TimestampDuration::new(1));
        }
        let marked = lsa.recorded();
        assert_eq!(marked, 7);

        for _ in 1..ROLLING_LATENCIES_SIZE {
            lsa.record_latency(TimestampDuration::new(1));
        }
        assert!(
            !lsa.window_newer_than(marked),
            "one sample from before the mark remains"
        );
        lsa.record_latency(TimestampDuration::new(1));
        assert!(lsa.window_newer_than(marked));
    }
}
