/// Flapping detector: decaying-penalty counter (BGP-damping style) over an observed value.
/// `record` counts a transition whenever the value differs from the last observed one; the
/// penalty decays with `half_life_us` and crossing `threshold` means flapping.
/// Timestamps are raw microseconds (veilid-tools has no Timestamp type).
#[derive(Debug, Clone)]
pub struct FlapDetector<T: PartialEq> {
    penalty: f64,
    threshold: f64,
    half_life_us: f64,
    last_update_us: u64,
    flapping: bool,
    last_value: Option<T>,
}

impl<T: PartialEq> FlapDetector<T> {
    /// Creates a detector that trips at `threshold` accumulated penalty, decaying by half every `half_life_us`.
    #[must_use]
    pub fn new(threshold: f64, half_life_us: u64) -> Self {
        Self {
            penalty: 0.0,
            threshold,
            half_life_us: half_life_us as f64,
            last_update_us: 0,
            flapping: false,
            last_value: None,
        }
    }

    /// Convenience: half-life in whole seconds.
    #[must_use]
    pub fn new_secs(threshold: f64, half_life_secs: u64) -> Self {
        Self::new(threshold, half_life_secs * 1_000_000)
    }

    fn decay(&mut self, now_us: u64) {
        if self.last_update_us != 0 && self.half_life_us > 0.0 {
            let elapsed = now_us.saturating_sub(self.last_update_us) as f64;
            self.penalty *= 0.5_f64.powf(elapsed / self.half_life_us);
        }
        self.last_update_us = now_us;
    }

    /// Observe a value; counts a transition when it differs from the last. Returns `Some(penalty)`
    /// only on the transition that tips into flapping (re-arms after decay).
    pub fn record(&mut self, now_us: u64, value: T) -> Option<f64> {
        let changed = matches!(&self.last_value, Some(lv) if *lv != value);
        self.last_value = Some(value);
        if !changed {
            return None;
        }
        self.record_event(now_us)
    }

    /// Count a discrete flap event (e.g. a connection drop). Returns `Some(penalty)` only on the
    /// event that tips into flapping (re-arms after decay).
    pub fn record_event(&mut self, now_us: u64) -> Option<f64> {
        self.decay(now_us);
        if self.flapping && self.penalty < self.threshold {
            self.flapping = false;
        }
        self.penalty += 1.0;
        if self.penalty >= self.threshold && !self.flapping {
            self.flapping = true;
            return Some(self.penalty);
        }
        None
    }

    /// Clear accumulated penalty and history. Use after a deliberate offline period
    /// (e.g. detach) so it isn't counted as flapping; threshold/half-life are kept.
    pub fn reset(&mut self) {
        self.penalty = 0.0;
        self.last_update_us = 0;
        self.flapping = false;
        self.last_value = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SEC: u64 = 1_000_000;

    #[test]
    fn stable_value_never_flaps() {
        let mut fd = FlapDetector::new(3.0, 30 * SEC);
        for i in 0..10 {
            assert_eq!(fd.record(i * SEC, true), None);
        }
    }

    #[test]
    fn first_observation_is_not_a_transition() {
        let mut fd = FlapDetector::new(3.0, 30 * SEC);
        assert_eq!(fd.record(0, true), None);
    }

    #[test]
    fn sustained_flipping_trips_threshold() {
        let mut fd = FlapDetector::new(3.0, 30 * SEC);
        let t = 5 * SEC; // same instant within the burst => decay-free, deterministic
        assert_eq!(fd.record(t, true), None); // first observation
        assert_eq!(fd.record(t, false), None); // transition 1, penalty 1
        assert_eq!(fd.record(t, true), None); // transition 2, penalty 2
        assert!(fd.record(t, false).is_some()); // transition 3, penalty 3 >= 3 => trips
    }

    #[test]
    fn rearms_after_decay() {
        let mut fd = FlapDetector::new(3.0, SEC);
        let t = 10 * SEC;
        fd.record(t, true);
        fd.record(t, false);
        fd.record(t, true);
        assert!(fd.record(t, false).is_some()); // trips
                                                // a long quiet period decays the penalty well below threshold; a fresh burst re-arms
        let t2 = t + 1000 * SEC;
        assert_eq!(fd.record(t2, true), None);
        assert_eq!(fd.record(t2, false), None);
        assert!(fd.record(t2, true).is_some());
    }

    #[test]
    fn reset_clears_penalty_and_history() {
        let mut fd = FlapDetector::new(3.0, 30 * SEC);
        let t = 5 * SEC;
        fd.record(t, true);
        fd.record(t, false);
        fd.record(t, true); // penalty 2, just under threshold
        fd.reset();
        // After reset the next observation is a fresh baseline (no transition),
        // so it takes a full new burst to trip rather than one more flip.
        assert_eq!(fd.record(t, false), None); // first observation post-reset
        assert_eq!(fd.record(t, true), None); // transition 1, penalty 1
        assert_eq!(fd.record(t, false), None); // transition 2, penalty 2
        assert!(fd.record(t, true).is_some()); // transition 3, penalty 3 => trips
    }
}
