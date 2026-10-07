use crate::*;

pub async fn test_basic_acquire_release() {
    info!("test_basic_acquire_release");

    let sem = AsyncWeightedSemaphore::new(100);
    assert_eq!(sem.in_flight(), 0);

    let g = sem.acquire(40).await;
    assert_eq!(sem.in_flight(), 40);
    let g2 = sem.acquire(40).await;
    assert_eq!(sem.in_flight(), 80);

    drop(g);
    assert_eq!(sem.in_flight(), 40);
    drop(g2);
    assert_eq!(sem.in_flight(), 0);
}

pub fn test_try_acquire_respects_limit() {
    info!("test_try_acquire_respects_limit");

    let sem = AsyncWeightedSemaphore::new(10);
    let g1 = sem.try_acquire(8).expect("8 fits under 10");
    assert_eq!(sem.in_flight(), 8);
    assert!(sem.try_acquire(4).is_none(), "8+4 exceeds limit 10");
    let g2 = sem.try_acquire(2).expect("8+2 fits exactly");
    assert_eq!(sem.in_flight(), 10);
    drop(g1);
    drop(g2);
    assert_eq!(sem.in_flight(), 0);
}

pub async fn test_waits_until_release() {
    info!("test_waits_until_release");

    let sem = AsyncWeightedSemaphore::new(10);
    let g1 = sem.acquire(8).await;

    let sem2 = sem.clone();
    let t = spawn("waiter", async move {
        // 8+5 exceeds 10, so this blocks until g1 releases
        let _g = sem2.acquire(5).await;
    });

    // Let the waiter reach the blocking point
    sleep(100).await;
    assert_eq!(
        sem.in_flight(),
        8,
        "waiter must not acquire while over limit"
    );

    drop(g1);
    t.await;
    assert_eq!(sem.in_flight(), 0, "all weight released");
}

pub async fn test_lower_limit_backpressure() {
    info!("test_lower_limit_backpressure");

    let sem = AsyncWeightedSemaphore::new(100);
    let g1 = sem.acquire(60).await;
    assert_eq!(sem.in_flight(), 60);

    // Lower the limit below current in-flight: no new admission until it drains
    sem.set_limit(50);
    assert!(
        sem.try_acquire(1).is_none(),
        "nothing admitted while in-flight exceeds the lowered limit"
    );

    drop(g1);
    let g2 = sem.acquire(40).await;
    assert_eq!(sem.in_flight(), 40);
    drop(g2);
}

pub async fn test_raise_limit_wakes_waiter() {
    info!("test_raise_limit_wakes_waiter");

    let sem = AsyncWeightedSemaphore::new(10);
    let g1 = sem.acquire(10).await;

    let sem2 = sem.clone();
    let t = spawn("waiter", async move {
        // Blocked at the full limit until it is raised
        let _g = sem2.acquire(5).await;
    });

    sleep(100).await;
    assert_eq!(sem.in_flight(), 10, "waiter blocked at limit");

    sem.set_limit(20);
    t.await;

    drop(g1);
    assert_eq!(sem.in_flight(), 0);
}

pub async fn test_progress_guarantee_oversized() {
    info!("test_progress_guarantee_oversized");

    let sem = AsyncWeightedSemaphore::new(10);
    // Weight exceeds the limit, but nothing is in flight, so it is admitted
    let g = sem.acquire(50).await;
    assert_eq!(sem.in_flight(), 50);
    // A second op must now wait
    assert!(sem.try_acquire(1).is_none());
    drop(g);
    assert_eq!(sem.in_flight(), 0);
}

pub async fn test_zero_weight_never_blocks() {
    info!("test_zero_weight_never_blocks");

    // Even with a zero limit, a zero-weight acquire returns immediately
    let sem = AsyncWeightedSemaphore::new(0);
    let g = sem.acquire(0).await;
    assert_eq!(sem.in_flight(), 0);
    drop(g);
}

pub fn test_raise_limit_monotonic() {
    info!("test_raise_limit_monotonic");

    let sem = AsyncWeightedSemaphore::new(100);
    assert_eq!(
        sem.raise_limit(200),
        100,
        "raise_limit returns the previous limit"
    );
    assert_eq!(sem.limit(), 200);
    assert_eq!(
        sem.raise_limit(50),
        200,
        "a lower candidate never lowers the limit"
    );
    assert_eq!(sem.limit(), 200);
    assert_eq!(sem.raise_limit(200), 200, "an equal candidate is a no-op");
    assert_eq!(sem.limit(), 200);
}

pub async fn test_all() {
    test_basic_acquire_release().await;
    test_try_acquire_respects_limit();
    test_waits_until_release().await;
    test_lower_limit_backpressure().await;
    test_raise_limit_wakes_waiter().await;
    test_progress_guarantee_oversized().await;
    test_zero_weight_never_blocks().await;
    test_raise_limit_monotonic();
}
