use super::*;

impl_veilid_log_facility!("test");

struct Emitter;
impl Emitter {
    fn log_key(&self) -> &'static str {
        ""
    }
}

/// Counts events reaching a subscriber, which is what the throttle exists to bound
struct CountingLayer(Arc<AtomicUsize>);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CountingLayer {
    fn on_event(
        &self,
        _event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn test_first_event_always_passes() {
    let t = LogThrottle::new_ms(60_000);
    assert_eq!(t.check(), Some(0));
}

pub fn test_repeats_inside_the_window_are_counted_not_emitted() {
    let t = LogThrottle::new_ms(60_000);
    assert_eq!(t.check(), Some(0));
    for _ in 0..1000 {
        assert_eq!(t.check(), None);
    }
    // A window this long cannot have elapsed, so the count is still accumulating
    assert_eq!(t.suppressed.load(Ordering::Relaxed), 1000);
}

pub fn test_window_elapsing_emits_and_reports_what_was_dropped() {
    let t = LogThrottle::new_ms(60_000);
    assert_eq!(t.check(), Some(0));
    for _ in 0..5 {
        assert_eq!(t.check(), None);
    }
    // Backdate the last emission rather than sleeping, so the boundary is exact and the test
    // needs no timer - it has to run on wasm too
    t.last_us.store(
        get_raw_timestamp().saturating_sub(61_000_000),
        Ordering::Relaxed,
    );
    assert_eq!(t.check(), Some(5));
    // Reported once and only once
    assert_eq!(t.check(), None);
}

pub fn test_a_forward_clock_step_does_not_silence_the_site() {
    let t = LogThrottle::new_ms(60_000);
    assert_eq!(t.check(), Some(0));
    assert_eq!(t.check(), None);
    // Stamp the future, as an NTP correction after a long suspend would
    t.last_us
        .store(get_raw_timestamp() + 3_600_000_000, Ordering::Relaxed);
    assert!(
        t.check().is_some(),
        "a clock jump forward must not mute the call site until real time catches up"
    );
}

pub fn test_suppressed_count_renders_nothing_when_none_were_dropped() {
    assert_eq!(format!("x{}", SuppressedCount(0)), "x");
    assert_eq!(format!("x{}", SuppressedCount(7)), "x [7 more suppressed]");
}

/// The macro wiring, not the primitive: arm ordering in `veilid_log!`, the appended count, and the
/// per-call-site `static`. A silent regression here would put the flood back.
pub fn test_throttle_option_on_veilid_log_bounds_a_hot_call_site() {
    use tracing_subscriber::layer::SubscriberExt as _;

    let count = Arc::new(AtomicUsize::new(0));
    let subscriber = tracing_subscriber::registry().with(CountingLayer(Arc::clone(&count)));
    let this = Emitter;

    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..10_000 {
            veilid_log!(this error throttle: 60_000, "spinning {}", 1);
        }
        // An unthrottled site emits 10000; this one emits the first and drops the rest
        assert_eq!(count.load(Ordering::Relaxed), 1);

        // A second site keeps its own counter and is not muted by the first
        for _ in 0..10 {
            veilid_log!(this error throttle: 60_000, target: "test", "other site");
        }
        assert_eq!(count.load(Ordering::Relaxed), 2);
    });
}

pub fn test_all() {
    test_first_event_always_passes();
    test_repeats_inside_the_window_are_counted_not_emitted();
    test_window_elapsing_emits_and_reports_what_was_dropped();
    test_a_forward_clock_step_does_not_silence_the_site();
    test_suppressed_count_renders_nothing_when_none_were_dropped();
    test_throttle_option_on_veilid_log_bounds_a_hot_call_site();
}
