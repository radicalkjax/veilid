use crate::*;

pub async fn test_interval() {
    info!("testing interval");

    let tick: Arc<Mutex<u32>> = Arc::new(Mutex::new(0u32));
    info!(
        "start non-immediate at {}",
        human_timestamp(get_raw_timestamp())
    );
    let stopper = interval("interval", 1000, false, move || {
        let tick = tick.clone();
        async move {
            let mut tick = tick.lock();
            info!("tick {} @ {}", tick, human_timestamp(get_raw_timestamp()));
            *tick += 1;
        }
    });
    sleep(3500).await;
    stopper.await;
    info!(
        "end non-immediate at {}",
        human_timestamp(get_raw_timestamp())
    );

    let tick: Arc<Mutex<u32>> = Arc::new(Mutex::new(0u32));
    info!(
        "start immediate at {}",
        human_timestamp(get_raw_timestamp())
    );
    let stopper = interval("interval", 1000, true, move || {
        let tick = tick.clone();
        async move {
            let mut tick = tick.lock();
            info!("tick {} @ {}", tick, human_timestamp(get_raw_timestamp()));
            *tick += 1;
        }
    });
    sleep(3500).await;
    stopper.await;
    info!("end immediate at {}", human_timestamp(get_raw_timestamp()));
}

#[allow(clippy::await_holding_lock)]
pub async fn test_timeout() {
    info!("testing timeout");

    let tick: Arc<Mutex<u32>> = Arc::new(Mutex::new(0u32));
    let tick_1 = tick.clone();
    assert!(
        timeout(2500, async move {
            let mut tick = tick_1.lock();
            trace!("tick {}", tick);
            sleep(1000).await;
            *tick += 1;
            trace!("tick {}", tick);
            sleep(1000).await;
            *tick += 1;
            trace!("tick {}", tick);
            sleep(1000).await;
            *tick += 1;
            trace!("tick {}", tick);
            sleep(1000).await;
            *tick += 1;
        })
        .await
        .is_err(),
        "should have timed out"
    );

    let ticks = *tick.lock();
    assert!(ticks <= 2);
}
