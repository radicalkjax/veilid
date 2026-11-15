//! Additive-increase / multiplicative-decrease (AIMD) rate control.
//!
//! AIMD is the feedback scheme behind TCP congestion avoidance. A sender
//! cannot know the capacity of a shared resource, so it probes: while
//! traffic succeeds it adds a small constant to its rate, and on congestion
//! evidence it cuts the rate by a multiplicative factor. Cuts outweigh
//! growth under contention, so independent senders converge toward equal
//! shares without coordinating, and the linear probe re-finds capacity when
//! it frees up. Terse treatment in the TCP context:
//! <https://en.wikipedia.org/wiki/Additive_increase/multiplicative_decrease>
//!
//! Two refinements follow reference implementations. TCP NewReno's recovery
//! period (quinn's `congestion/new_reno.rs`): congestion evidence from work
//! admitted before the last cut reflects the old rate and must not trigger
//! another cut, so after a decrease all changes are suppressed for
//! [`AimdParams::md_cooldown_us`]. Netflix concurrency-limits' `AIMDLimit`:
//! the limit grows only when something waited on it, so an unloaded limit
//! stays put.
//!
//! One deliberate deviation from TCP: no slow start. A consumer whose
//! starting limit is known-safe starts at the ceiling and descends on
//! evidence, the opposite of a sender probing upward from a tiny window.
//! If loss-based detection proves too coarse, latency-based control
//! (Gradient/Vegas style) is the standard evolution.
//!
//! [`AimdController`] holds the authoritative limit; consumers mirror it
//! into their enforcement mechanism (a semaphore or a window) after each
//! [`step`](AimdController::step) or [`reset`](AimdController::reset).
//! Feed it per-operation outcomes
//! ([`record_outcome`](AimdController::record_outcome)) and waits on the
//! limit ([`record_demand`](AimdController::record_demand)), then call
//! `step` once per control interval. Callers pass `now_us`, so there is no
//! clock dependency; units are caller-chosen, bytes per second and job
//! counts both work.
//!
//! veilid-core's storage manager operation gate
//! (`storage_manager/gated_operation.rs`) uses this to bound in-flight DHT
//! operation bandwidth.
use core::sync::atomic::{AtomicU64, Ordering};

/// Tuning for [`AimdController`]. All limits and rates share one
/// caller-chosen unit. Callers keep `floor <= initial`.
#[derive(Debug, Clone)]
pub struct AimdParams {
    /// Starting limit, restored by `reset()`
    pub initial: u64,
    /// Lower bound no decrease goes below
    pub floor: u64,
    /// A congested window scales the limit by `md_scale_num / md_scale_den`
    /// (1/2 is TCP-style halving, 9/10 the gentler Netflix default)
    pub md_scale_num: u64,
    /// Denominator of the decrease ratio
    pub md_scale_den: u64,
    /// Additive increase per step under demand
    pub ai_step: u64,
    /// A window is congested when `congested >= 1` and
    /// `congested/completed >= congested_num/congested_den`
    pub congested_num: u64,
    /// Denominator of the congested-fraction threshold
    pub congested_den: u64,
    /// The limit never rises past `max(achieved_rate * headroom_factor, initial)`
    pub headroom_factor: u64,
    /// Recovery period after a decrease; suppresses all changes until it passes
    pub md_cooldown_us: u64,
}

/// One control step's outcome. The controller has already applied the new
/// limit when this is returned; consumers enforce and log it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AimdDecision {
    /// No change
    Hold,
    /// Multiplicative decrease to this limit. Reported even when already
    /// pinned at the floor so consumers can measure time spent there.
    Lower(u64),
    /// Limit pulled down to the growth ceiling
    Clamp(u64),
    /// Additive increase to this limit
    Raise(u64),
}

/// What one step decided, with the window it consumed for observability.
#[derive(Copy, Clone, Debug)]
pub struct AimdStepResult {
    /// The change applied, if any
    pub decision: AimdDecision,
    /// Operations completed in the consumed window
    pub completed: u64,
    /// Congested operations in the consumed window
    pub congested: u64,
    /// Waits on the limit recorded in the consumed window
    pub demand_events: u64,
}

/// AIMD rate controller; the module docs describe the control law.
///
/// Interior atomics: all methods take `&self`. An outcome recorded while
/// `step` runs may land in the next window, misattributing at most one
/// operation.
#[derive(Debug)]
pub struct AimdController {
    /// Tuning fixed at construction
    params: AimdParams,
    /// The authoritative limit
    limit: AtomicU64,
    /// Operations completed in the current window
    completed: AtomicU64,
    /// Congested operations in the current window
    congested: AtomicU64,
    /// Waits on the limit in the current window
    demand_events: AtomicU64,
    /// Microsecond timestamp of the last decrease; 0 = never
    last_md_ts_us: AtomicU64,
}

impl AimdController {
    /// Create a controller at `params.initial`.
    #[must_use]
    pub fn new(params: AimdParams) -> Self {
        let limit = AtomicU64::new(params.initial);
        Self {
            params,
            limit,
            completed: AtomicU64::new(0),
            congested: AtomicU64::new(0),
            demand_events: AtomicU64::new(0),
            last_md_ts_us: AtomicU64::new(0),
        }
    }

    /// Current limit
    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Acquire)
    }

    /// Record one completed operation and whether it showed congestion.
    /// Do not record operations that failed for reasons unrelated to the
    /// resource.
    pub fn record_outcome(&self, congested: bool) {
        self.completed.fetch_add(1, Ordering::AcqRel);
        if congested {
            self.congested.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// Record an acquisition that had to wait on the limit. The limit grows
    /// only in windows where this was called.
    pub fn record_demand(&self) {
        self.demand_events.fetch_add(1, Ordering::AcqRel);
    }

    /// Restore the initial limit and clear window and recovery state.
    pub fn reset(&self) {
        self.limit.store(self.params.initial, Ordering::Release);
        self.completed.store(0, Ordering::Release);
        self.congested.store(0, Ordering::Release);
        self.demand_events.store(0, Ordering::Release);
        self.last_md_ts_us.store(0, Ordering::Release);
    }

    /// Run one control interval: consume the accumulated window and apply at
    /// most one limit change. `achieved_rate` is recently measured
    /// throughput in the limit's units; `max(achieved_rate *
    /// headroom_factor, initial)` caps growth and pulls an inflated limit
    /// back down.
    pub fn step(&self, now_us: u64, achieved_rate: u64) -> AimdStepResult {
        let completed = self.completed.swap(0, Ordering::AcqRel);
        let congested = self.congested.swap(0, Ordering::AcqRel);
        let demand_events = self.demand_events.swap(0, Ordering::AcqRel);

        let decision = self.decide(now_us, achieved_rate, completed, congested, demand_events);
        AimdStepResult {
            decision,
            completed,
            congested,
            demand_events,
        }
    }

    fn decide(
        &self,
        now_us: u64,
        achieved_rate: u64,
        completed: u64,
        congested: u64,
        demand_events: u64,
    ) -> AimdDecision {
        let limit = self.limit.load(Ordering::Acquire);
        let clamp = achieved_rate
            .saturating_mul(self.params.headroom_factor)
            .max(self.params.initial);

        let last_md = self.last_md_ts_us.load(Ordering::Acquire);
        let in_recovery =
            last_md != 0 && now_us.saturating_sub(last_md) < self.params.md_cooldown_us;

        // congested >= 1 keeps a racy congested-only window well-defined
        let window_congested = congested >= 1
            && congested.saturating_mul(self.params.congested_den)
                >= completed.saturating_mul(self.params.congested_num);

        if window_congested && !in_recovery {
            let scaled =
                limit.saturating_mul(self.params.md_scale_num) / self.params.md_scale_den.max(1);
            let new_limit = scaled.max(self.params.floor);
            self.limit.store(new_limit, Ordering::Release);
            // now_us of 0 would read as "never decreased"
            self.last_md_ts_us.store(now_us.max(1), Ordering::Release);
            return AimdDecision::Lower(new_limit);
        }
        if limit > clamp {
            self.limit.store(clamp, Ordering::Release);
            return AimdDecision::Clamp(clamp);
        }
        if demand_events >= 1 && !in_recovery && limit < clamp {
            let new_limit = limit.saturating_add(self.params.ai_step).min(clamp);
            self.limit.store(new_limit, Ordering::Release);
            return AimdDecision::Raise(new_limit);
        }
        AimdDecision::Hold
    }
}
