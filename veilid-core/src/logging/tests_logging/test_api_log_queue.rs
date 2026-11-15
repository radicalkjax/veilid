use super::api_log_queue::{ApiLogQueue, QueuedLog};
use super::*;

/// Runs the collapse step over `(level, message)` pairs, since that is all these cases vary.
fn dedup(batch: Vec<(VeilidLogLevel, String)>) -> Vec<(VeilidLogLevel, String)> {
    let batch = batch
        .into_iter()
        .map(|(log_level, message)| QueuedLog {
            log_key: String::new(),
            log_level,
            message,
            backtrace: None,
        })
        .collect();
    ApiLogQueue::deduplicate(batch)
        .into_iter()
        .map(|log| (log.log_level, log.message))
        .collect()
}

pub fn test_fuzzy_key_collapses_varying_parts() {
    let k = ApiLogQueue::fuzzy_key;

    // A storm: identical but for the errno
    assert_eq!(
        k("No connection(Socket is not connected (os error 57)) at udp.rs@62:39"),
        k("No connection(Socket is not connected (os error 11)) at udp.rs@99:12")
    );

    // Addresses, ports and counts fold together
    assert_eq!(
        k("TCP connection from: 192.168.1.5:8080"),
        k("TCP connection from: 10.0.0.99:443")
    );
    assert_eq!(k("dropped 5 messages"), k("dropped 41231 messages"));

    // Genuinely different messages must not collapse
    assert_ne!(k("failed to send"), k("failed to receive"));
    assert_ne!(
        k("TCP connection from: 1.2.3.4:5"),
        k("UDP connection from: 1.2.3.4:5")
    );

    // Prose is left alone, so a message with no numbers is its own key
    assert_eq!(
        k("connection unable to receive"),
        "connection unable to receive"
    );
}

/// A storm: 30,000 copies of one line, differing only in a number.
pub fn test_dedup_collapses_a_storm_to_one_line() {
    let batch: Vec<(VeilidLogLevel, String)> = (0..30_000)
        .map(|i| {
            (
                VeilidLogLevel::Debug,
                format!("No connection(Socket is not connected (os error {i}))"),
            )
        })
        .collect();

    let out = dedup(batch);
    assert_eq!(out.len(), 1);
    assert!(
        out[0].1.ends_with(" [and 29999 more like it]"),
        "collapsed line must say what it stands for, got: {}",
        out[0].1
    );
}

pub fn test_dedup_keeps_distinct_shapes_and_levels_apart() {
    let batch = vec![
        (VeilidLogLevel::Debug, "alpha 1".to_owned()),
        (VeilidLogLevel::Debug, "alpha 2".to_owned()),
        (VeilidLogLevel::Debug, "beta 1".to_owned()),
        // Same words, different level: a different event
        (VeilidLogLevel::Error, "alpha 3".to_owned()),
    ];

    let out = dedup(batch);
    assert_eq!(out.len(), 3);
    // First of each shape is kept, in the order it arrived
    assert_eq!(out[0].0, VeilidLogLevel::Debug);
    assert!(out[0].1.starts_with("alpha 1"));
    assert!(out[0].1.ends_with(" [and 1 more like it]"));
    assert_eq!(out[1].1, "beta 1");
    assert_eq!(out[2].0, VeilidLogLevel::Error);
    assert_eq!(out[2].1, "alpha 3");
}

/// A queue with its own sink, which is what having a struct rather than a module of statics buys:
/// the rate limit, the collapse and the hand-off can be watched without a layer or a callback.
static TEST_QUEUE: ApiLogQueue = ApiLogQueue::new();

pub async fn test_queue_delivers_through_its_sink() {
    let delivered = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink_delivered = Arc::clone(&delivered);
    TEST_QUEUE.start(Arc::new(move |log: QueuedLog| {
        sink_delivered.lock().push(log.message);
    }));

    TEST_QUEUE.enqueue(QueuedLog {
        log_key: String::new(),
        log_level: VeilidLogLevel::Info,
        message: "through the sink".to_owned(),
        backtrace: None,
    });

    // The drain blocks on the channel, so the first event goes out without waiting an interval.
    // Polled rather than slept a fixed time so this is not a timing race.
    let mut waited_ms = 0u32;
    while delivered.lock().is_empty() && waited_ms < 5000 {
        sleep(20).await;
        waited_ms += 20;
    }
    let got = delivered.lock().clone();
    TEST_QUEUE.stop();

    assert_eq!(
        got,
        vec!["through the sink".to_owned()],
        "the queue did not hand its event to the sink within {waited_ms}ms"
    );
}

/// End to end through the real tracing layer: an event emitted the way veilid emits one has to
/// come out of a registered update callback. A queue that delivers nothing at all is the
/// regression that would matter most and show up least.
pub async fn test_emitted_events_reach_the_update_callback() {
    use tracing_subscriber::layer::SubscriberExt as _;

    let delivered = Arc::new(Mutex::new(Vec::<String>::new()));
    let log_key = "tests_logging".to_owned();

    let cb_delivered = Arc::clone(&delivered);
    let update_cb: UpdateCallback = Arc::new(move |update| {
        if let VeilidUpdate::Log(log) = update {
            cb_delivered.lock().push(log.message.clone());
        }
    });

    ApiTracingLayer::add_callback(log_key.clone(), update_cb).expect("should register");

    let subscriber = tracing_subscriber::registry().with(ApiTracingLayer::init());
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(__VEILID_LOG_KEY = log_key.as_str(), "emitted and delivered");
    });

    let mut waited_ms = 0u32;
    let found = loop {
        if delivered
            .lock()
            .iter()
            .any(|m| m.contains("emitted and delivered"))
        {
            break true;
        }
        if waited_ms >= 5000 {
            break false;
        }
        sleep(20).await;
        waited_ms += 20;
    };

    ApiTracingLayer::remove_callback(log_key).expect("should deregister");
    assert!(
        found,
        "an emitted event did not reach the update callback within {waited_ms}ms"
    );
}

pub async fn test_all() {
    test_fuzzy_key_collapses_varying_parts();
    test_dedup_collapses_a_storm_to_one_line();
    test_dedup_keeps_distinct_shapes_and_levels_apart();
    test_queue_delivers_through_its_sink().await;
    test_emitted_events_reach_the_update_callback().await;
}
