//! Our own share of lost round trips, measured from the answers to direct questions.
//!
//! - Only questions over a path, a peer and sequence ordering, that answered another question
//!   within `RELIABLE_PING_INTERVAL_MAX` count, so a path that never answers (a dead peer still
//!   pinged, one behind our own NAT, a peer that refuses one transport) does not read as our loss
//! - Outcomes are binned by send time, and the window read ends one RPC timeout ago, when every
//!   question in it has its outcome
//! - The answered fraction is divided by a baseline learned while online, so the residual loss of
//!   a healthy network reads as none
//! - The decision moves only when the window bounds survival on one side of the threshold, so
//!   thin traffic holds it; entering also needs several paths and the median one to agree, so a
//!   few failing peers are not taken for our own loss

use super::*;

/// One-way loss on our link, in percent, at which the domain counts as degraded
const DEGRADED_ONE_WAY_LOSS_PERCENT: u32 = 10;
/// Standard score of the one-sided 95% bounds on the answered fraction
const SURVIVAL_BOUND_Z: f64 = 1.645;
/// Paths the median needs before it can tell our loss from theirs: the breadth offline needs
const MIN_ATTRIBUTION_PATHS: usize = OFFLINE_DETECTION_FAILURE_THRESHOLD_MESSAGE;

/// A peer as reached over one sequence ordering
type Path = (NodeId, SequenceOrdering);

/// Survival below which our link is degraded: a round trip crosses it twice
fn degraded_survival_threshold() -> f64 {
    let one_way = 1.0 - f64::from(DEGRADED_ONE_WAY_LOSS_PERCENT) / 100.0;
    one_way * one_way
}

/// Wilson score bounds on a binomial fraction
fn wilson_bounds(successes: u32, trials: u32) -> (f64, f64) {
    if trials == 0 {
        return (0.0, 1.0);
    }
    let n = f64::from(trials);
    let p = f64::from(successes) / n;
    let z2 = SURVIVAL_BOUND_Z * SURVIVAL_BOUND_Z;
    let den = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / den;
    let half = SURVIVAL_BOUND_Z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / den;
    ((center - half).max(0.0), (center + half).min(1.0))
}

/// Questions asked and answered
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct Tally {
    /// Questions asked
    asked: u32,
    /// Questions answered
    answered: u32,
}

impl Tally {
    fn add(&mut self, answered: bool) {
        self.asked += 1;
        if answered {
            self.answered += 1;
        }
    }

    fn merge(&mut self, other: &Tally) {
        self.asked += other.asked;
        self.answered += other.answered;
    }

    fn fraction(&self) -> Option<f64> {
        (self.asked > 0).then(|| f64::from(self.answered) / f64::from(self.asked))
    }
}

/// Eligible questions sent within one second
#[derive(Debug)]
struct SendSecond {
    /// Send time in whole seconds
    second: u64,
    /// Every eligible question sent in it
    all: Tally,
    /// The same per path
    per_path: HashMap<Path, Tally>,
}

/// What the window says about our own link
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct LossEstimate {
    /// Share of round trips our own link lets through, from 0 to 1
    pub survival: f64,
    /// Eligible questions in the window
    pub samples: u32,
    /// Whether loss on our link holds the domain degraded
    pub degraded: bool,
}

impl Default for LossEstimate {
    fn default() -> Self {
        Self {
            survival: 1.0,
            samples: 0,
            degraded: false,
        }
    }
}

impl LossEstimate {
    /// Whether the loss shows as at least one percent, the least worth a log line
    pub fn shows_loss(&self) -> bool {
        (1.0 - self.survival) * 100.0 >= 0.5
    }
}

impl fmt::Display for LossEstimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "round-trip loss {:.0}% over {} questions{}",
            (1.0 - self.survival) * 100.0,
            self.samples,
            if self.degraded { ", degraded" } else { "" }
        )
    }
}

/// Measures our own share of lost round trips for one routing domain
#[derive(Debug)]
pub(crate) struct LossEstimator {
    /// Last answer over each path, for eligibility
    last_answered: HashMap<Path, Timestamp>,
    /// Eligible outcomes by send second, oldest first
    seconds: VecDeque<SendSecond>,
    /// Answered fraction of a healthy network, learned while online
    baseline: f64,
    /// Second the baseline last learned in
    baseline_second: Option<u64>,
    /// Latest estimate
    estimate: LossEstimate,
    /// Second the latest estimate was made in
    estimate_second: Option<u64>,
}

impl Default for LossEstimator {
    fn default() -> Self {
        Self {
            last_answered: HashMap::new(),
            seconds: VecDeque::new(),
            baseline: 1.0,
            baseline_second: None,
            estimate: LossEstimate::default(),
            estimate_second: None,
        }
    }
}

impl LossEstimator {
    /// Record a direct question's outcome; `counted` false keeps it out of the window but still
    /// lets an answer vouch for its path
    pub fn record_outcome(
        &mut self,
        node_id: &NodeId,
        ordering: SequenceOrdering,
        sent_ts: Timestamp,
        answered: bool,
        counted: bool,
        now: Timestamp,
    ) {
        let path = (node_id.clone(), ordering);
        let since = sent_ts.earlier(RELIABLE_PING_INTERVAL_MAX);
        let eligible = self.last_answered.get(&path).is_some_and(|ts| *ts >= since);
        if answered {
            self.last_answered.insert(path.clone(), now);
        }
        if !eligible || !counted {
            return;
        }
        let second = sent_ts.as_u64() / 1_000_000;
        let pos = match self.seconds.binary_search_by_key(&second, |s| s.second) {
            Ok(pos) => pos,
            Err(pos) => {
                self.seconds.insert(
                    pos,
                    SendSecond {
                        second,
                        all: Tally::default(),
                        per_path: HashMap::new(),
                    },
                );
                pos
            }
        };
        let bin = &mut self.seconds[pos];
        bin.all.add(answered);
        bin.per_path.entry(path).or_default().add(answered);
    }

    /// Whether the peer answered one of our questions, over any ordering, at or after `since`
    pub fn answered_since(&self, node_id: &NodeId, since: Timestamp) -> bool {
        SequenceOrderingSet::all().iter().any(|ordering| {
            self.last_answered
                .get(&(node_id.clone(), ordering))
                .is_some_and(|ts| *ts >= since)
        })
    }

    /// The estimate as of now, recomputed at most once a second; `learn` lets a window that is
    /// not degraded teach the baseline
    pub fn evaluate(
        &mut self,
        timings: &OnlineDetectorTimings,
        learn: bool,
        now: Timestamp,
    ) -> LossEstimate {
        let now_second = now.as_u64() / 1_000_000;
        if self.estimate_second == Some(now_second) {
            return self.estimate;
        }
        self.estimate_second = Some(now_second);
        self.prune(timings, now);

        let secs = |d: TimestampDuration| d.as_u64() / 1_000_000;
        let end = now_second.saturating_sub(secs(timings.outcome_lag));
        let level_start = end.saturating_sub(secs(timings.degraded_evidence_window));
        let attribution_start = end.saturating_sub(secs(timings.loss_attribution_window));
        let mut level = Tally::default();
        let mut per_path = HashMap::<Path, Tally>::new();
        for bin in self.seconds.iter().take_while(|b| b.second < end) {
            if bin.second >= attribution_start {
                for (path, tally) in &bin.per_path {
                    per_path.entry(path.clone()).or_default().merge(tally);
                }
            }
            if bin.second >= level_start {
                level.merge(&bin.all);
            }
        }

        let threshold = degraded_survival_threshold();
        let survival = |fraction: f64| (fraction / self.baseline).clamp(0.0, 1.0);
        let (lo, hi) = wilson_bounds(level.answered, level.asked);
        let degraded = match level.fraction() {
            None => self.estimate.degraded,
            Some(_) if self.estimate.degraded => survival(lo) <= threshold,
            Some(_) => survival(hi) < threshold && self.median_agrees(&per_path, threshold),
        };
        self.estimate = LossEstimate {
            survival: level
                .fraction()
                .map(survival)
                .unwrap_or(self.estimate.survival),
            samples: level.asked,
            degraded,
        };

        if let (true, false, Some(fraction)) = (learn, degraded, level.fraction()) {
            self.learn_baseline(fraction, now_second);
        }
        self.estimate
    }

    /// The latest estimate, as of the last evaluation
    pub fn estimate(&self) -> LossEstimate {
        self.estimate
    }

    /// The window holds no evidence from before the current online period; the decision stays
    /// until the new period's evidence moves it, since thin traffic would not re-prove it soon
    pub fn clear_window(&mut self) {
        self.seconds.clear();
        self.estimate.samples = 0;
        self.estimate_second = None;
    }

    /// A new network: nothing learned about the old one applies
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Whether enough paths were asked and the median path's survival is below the threshold too
    fn median_agrees(&self, per_path: &HashMap<Path, Tally>, threshold: f64) -> bool {
        let mut fractions: Vec<f64> = per_path.values().filter_map(Tally::fraction).collect();
        if fractions.len() < MIN_ATTRIBUTION_PATHS {
            return false;
        }
        fractions.sort_by(f64::total_cmp);
        let mid = fractions.len() / 2;
        let median = if fractions.len().is_multiple_of(2) {
            (fractions[mid - 1] + fractions[mid]) / 2.0
        } else {
            fractions[mid]
        };
        median / self.baseline < threshold
    }

    /// Move the baseline toward the window's fraction with a time constant of
    /// `RELIABLE_PING_INTERVAL_MAX`
    fn learn_baseline(&mut self, fraction: f64, now_second: u64) {
        let elapsed = self
            .baseline_second
            .map_or(1, |s| now_second.saturating_sub(s).max(1));
        self.baseline_second = Some(now_second);
        let tau = RELIABLE_PING_INTERVAL_MAX.as_u64() as f64 / 1_000_000.0;
        let alpha = 1.0 - (-(elapsed as f64) / tau).exp();
        self.baseline += alpha * (fraction - self.baseline);
    }

    fn prune(&mut self, timings: &OnlineDetectorTimings, now: Timestamp) {
        let keep = timings
            .outcome_lag
            .saturating_add(timings.loss_attribution_window);
        let oldest = now.earlier(keep).as_u64() / 1_000_000;
        while self.seconds.front().is_some_and(|b| b.second < oldest) {
            self.seconds.pop_front();
        }
        let horizon = now.earlier(RELIABLE_PING_INTERVAL_MAX.saturating_add(timings.outcome_lag));
        self.last_answered.retain(|_, ts| *ts >= horizon);
    }
}
