use crate::*;

/// A wake sent before anyone waits is kept, wakes coalesce, and a wait ends at its deadline or
/// when stopped
pub async fn test_wakeup() {
    info!("testing wakeup");
    let stop_source = StopSource::new();
    let (sender, receiver) = wakeup();

    sender.wake();
    sender.wake();
    assert_eq!(
        receiver.wait(None, stop_source.token()).await,
        WakeOutcome::Woken
    );
    let deadline = get_raw_timestamp() + 50_000;
    assert_eq!(
        receiver.wait(Some(deadline), stop_source.token()).await,
        WakeOutcome::Deadline
    );
    assert!(get_raw_timestamp() >= deadline);

    let stopped = StopSource::new();
    let stop_token = stopped.token();
    drop(stopped);
    assert_eq!(receiver.wait(None, stop_token).await, WakeOutcome::Stopped);
}

/// The routine runs once the armed deadline passes, a sooner arm wakes the timer, the deadline the
/// routine returns arms it again, and stop waits for the timer task
pub async fn test_deadline_timer() {
    info!("testing deadline timer");
    let pending = Arc::new(Mutex::new(Vec::<u64>::new()));
    let runs = Arc::new(AtomicU64::new(0));
    let timer = DeadlineTimer::new("test deadline timer");
    timer.start({
        let pending = pending.clone();
        let runs = runs.clone();
        move |now, _stop_token| {
            runs.fetch_add(1, Ordering::AcqRel);
            let mut pending = pending.lock();
            pending.retain(|deadline| *deadline > now);
            let next = pending.iter().min().copied();
            Box::pin(async move { next })
        }
    });

    // Nothing armed: nothing runs
    sleep(50).await;
    assert_eq!(runs.load(Ordering::Acquire), 0);

    // A far deadline, then a sooner one
    let far = get_raw_timestamp() + 60_000_000;
    pending.lock().push(far);
    timer.arm(far);
    sleep(20).await;
    let soon = get_raw_timestamp() + 50_000;
    pending.lock().push(soon);
    timer.arm(soon);
    sleep(300).await;
    assert_eq!(runs.load(Ordering::Acquire), 1);
    assert_eq!(*pending.lock(), vec![far]);

    // The second deadline's own arm is superseded; the first run's return arms it
    let first = get_raw_timestamp() + 50_000;
    let second = first + 100_000;
    pending.lock().extend([first, second]);
    timer.arm(second);
    timer.arm(first);
    sleep(400).await;
    assert_eq!(runs.load(Ordering::Acquire), 3);
    assert_eq!(*pending.lock(), vec![far]);

    timer.stop().await;
    pending.lock().push(get_raw_timestamp());
    timer.arm(0);
    sleep(50).await;
    assert_eq!(runs.load(Ordering::Acquire), 3);
}

/// Join waits for the run in flight and leaves the task able to run again
pub async fn test_tick_task_join() {
    info!("testing tick task join");
    let finished = Arc::new(AtomicU64::new(0));
    let task: TickTask<()> = TickTask::new_ms("test tick task join", 60_000);
    task.set_routine({
        let finished = finished.clone();
        move |_stop_token, _last_ts, _cur_ts| {
            let finished = finished.clone();
            Box::pin(async move {
                sleep(100).await;
                finished.fetch_add(1, Ordering::AcqRel);
                Ok(())
            })
        }
    });

    // Nothing running: returns at once
    task.join().await.expect("idle join");

    assert!(task.try_tick_now().await.expect("first run"));
    task.join().await.expect("join first run");
    assert_eq!(finished.load(Ordering::Acquire), 1);

    assert!(task.try_tick_now().await.expect("second run"));
    task.join().await.expect("join second run");
    assert_eq!(finished.load(Ordering::Acquire), 2);

    task.stop().await.expect("stop");
}

pub async fn test_all() {
    test_wakeup().await;
    test_deadline_timer().await;
    test_tick_task_join().await;
}
