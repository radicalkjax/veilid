use crate::*;

pub async fn test_must_join_single_future() {
    info!("testing must join single future");
    let sf = MustJoinSingleFuture::<u32>::new();
    assert_eq!(sf.check().await, Ok(None));
    assert_eq!(
        sf.single_spawn("t1", || async {
            sleep(2000).await;
            69
        })
        .await,
        Ok((None, true))
    );
    assert_eq!(sf.check().await, Ok(None));
    assert_eq!(
        sf.single_spawn("t2", || async { panic!() }).await,
        Ok((None, false))
    );
    assert_eq!(sf.join().await, Ok(Some(69)));
    assert_eq!(
        sf.single_spawn("t3", || async {
            sleep(1000).await;
            37
        })
        .await,
        Ok((None, true))
    );
    sleep(2000).await;
    assert_eq!(
        sf.single_spawn("t4", || async {
            sleep(1000).await;
            27
        })
        .await,
        Ok((Some(37), true))
    );
    sleep(2000).await;
    assert_eq!(sf.join().await, Ok(Some(27)));
    assert_eq!(sf.check().await, Ok(None));
}
