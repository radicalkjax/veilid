use crate::*;

fn test_phase_split() {
    let t = YieldTiming {
        enqueued_us: 1_000_000,
        started_us: 1_400_000,
        finished_us: 1_450_000,
        woken_us: 2_100_000,
    };
    assert_eq!(t.wait_us(), 400_000);
    assert_eq!(t.run_us(), 50_000);
    assert_eq!(t.wake_us(), 650_000);
    assert_eq!(t.total_us(), 1_100_000);
    assert!(t.is_slow());

    // Clock going backwards reads as zero, never as a huge phase
    let backwards = YieldTiming {
        enqueued_us: 5,
        started_us: 4,
        finished_us: 3,
        woken_us: 2,
    };
    assert_eq!(backwards.wait_us(), 0);
    assert_eq!(backwards.run_us(), 0);
    assert_eq!(backwards.wake_us(), 0);
    assert_eq!(backwards.total_us(), 0);
    assert!(!backwards.is_slow());
}

async fn test_yields_return_and_drain() {
    let before = get_raw_timestamp();
    let v = cpu_yielding(|| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        7u32
    })
    .await;
    assert_eq!(v, 7);
    assert!(get_raw_timestamp().saturating_sub(before) >= 50_000);
    assert_eq!(cpu_pool_queue_depth(), 0);

    let v = blocking_yielding(|| 11u32).await;
    assert_eq!(v, 11);
}

pub async fn test_all() {
    test_phase_split();
    test_yields_return_and_drain().await;
}
