use crate::*;

pub async fn test_eventual() {
    info!("testing Eventual");
    {
        let e1 = Eventual::new();
        let i1 = e1.instance_clone(1u32);
        let i2 = e1.instance_clone(2u32);
        let i3 = e1.instance_clone(3u32);
        drop(i3);
        let i4 = e1.instance_clone(4u32);
        drop(i2);

        let jh = spawn("task", async move {
            sleep(1000).await;
            e1.resolve().await;
        });

        assert_eq!(i1.await, 1u32);
        assert_eq!(i4.await, 4u32);

        jh.await;
    }
    {
        let e1 = Eventual::new();
        let i1 = e1.instance_clone(1u32);
        let i2 = e1.instance_clone(2u32);
        let i3 = e1.instance_clone(3u32);
        let i4 = e1.instance_clone(4u32);
        let e1_c1 = e1.clone();
        let jh = spawn("task", async move {
            let i5 = e1.instance_clone(5u32);
            let i6 = e1.instance_clone(6u32);
            assert_eq!(i1.await, 1u32);
            assert_eq!(i5.await, 5u32);
            assert_eq!(i6.await, 6u32);
        });
        sleep(1000).await;
        let resolved = e1_c1.resolve();
        drop(i2);
        drop(i3);
        assert_eq!(i4.await, 4u32);
        resolved.await;
        jh.await;
    }
    {
        let e1 = Eventual::new();
        let i1 = e1.instance_clone(1u32);
        let i2 = e1.instance_clone(2u32);
        let e1_c1 = e1.clone();
        let jh = spawn("task", async move {
            assert_eq!(i1.await, 1u32);
            assert_eq!(i2.await, 2u32);
        });
        sleep(1000).await;
        e1_c1.resolve().await;

        jh.await;

        e1_c1.reset();
        //
        let j1 = e1.instance_clone(1u32);
        let j2 = e1.instance_clone(2u32);
        let jh = spawn("task", async move {
            assert_eq!(j1.await, 1u32);
            assert_eq!(j2.await, 2u32);
        });
        sleep(1000).await;
        e1_c1.resolve().await;

        jh.await;

        e1_c1.reset();
    }
}

pub async fn test_eventual_value() {
    info!("testing Eventual Value");
    {
        let e1 = EventualValue::<u32>::new();
        let i1 = e1.instance();
        let i2 = e1.instance();
        let i3 = e1.instance();
        drop(i3);
        let i4 = e1.instance();
        drop(i2);

        let e1_c1 = e1.clone();
        let jh = spawn("task", async move {
            sleep(1000).await;
            e1_c1.resolve(3u32);
        });

        i1.await;
        i4.await;
        jh.await;
        assert_eq!(e1.take_value(), Some(3u32));
    }
    {
        let e1 = EventualValue::new();
        let i1 = e1.instance();
        let i2 = e1.instance();
        let i3 = e1.instance();
        let i4 = e1.instance();
        let e1_c1 = e1.clone();
        let jh = spawn("task", async move {
            let i5 = e1.instance();
            let i6 = e1.instance();
            i1.await;
            i5.await;
            i6.await;
        });
        sleep(1000).await;
        let resolved = e1_c1.resolve(4u16);
        drop(i2);
        drop(i3);
        i4.await;
        resolved.await;
        jh.await;
        assert_eq!(e1_c1.take_value(), Some(4u16));
    }
    {
        let e1 = EventualValue::new();
        assert_eq!(e1.take_value(), None);
        let i1 = e1.instance();
        let i2 = e1.instance();
        let e1_c1 = e1.clone();
        let jh = spawn("task", async move {
            i1.await;
            i2.await;
        });
        sleep(1000).await;
        e1_c1.resolve(5u32).await;
        jh.await;
        assert_eq!(e1_c1.take_value(), Some(5u32));
        e1_c1.reset();
        assert_eq!(e1_c1.take_value(), None);
        //
        let j1 = e1.instance();
        let j2 = e1.instance();
        let jh = spawn("task", async move {
            j1.await;
            j2.await;
        });
        sleep(1000).await;
        e1_c1.resolve(6u32).await;
        jh.await;
        assert_eq!(e1_c1.take_value(), Some(6u32));
        e1_c1.reset();
        assert_eq!(e1_c1.take_value(), None);
    }
}

pub async fn test_eventual_value_clone() {
    info!("testing Eventual Value Clone");
    {
        let e1 = EventualValueClone::<u32>::new();
        let i1 = e1.instance();
        let i2 = e1.instance();
        let i3 = e1.instance();
        drop(i3);
        let i4 = e1.instance();
        drop(i2);

        let jh = spawn("task", async move {
            sleep(1000).await;
            e1.resolve(3u32);
        });

        assert_eq!(i1.await, 3);
        assert_eq!(i4.await, 3);

        jh.await;
    }

    {
        let e1 = EventualValueClone::new();
        let i1 = e1.instance();
        let i2 = e1.instance();
        let i3 = e1.instance();
        let i4 = e1.instance();
        let e1_c1 = e1.clone();
        let jh = spawn("task", async move {
            let i5 = e1.instance();
            let i6 = e1.instance();
            assert_eq!(i1.await, 4);
            assert_eq!(i5.await, 4);
            assert_eq!(i6.await, 4);
        });
        sleep(1000).await;
        let resolved = e1_c1.resolve(4u16);
        drop(i2);
        drop(i3);
        assert_eq!(i4.await, 4);
        resolved.await;
        jh.await;
    }

    {
        let e1 = EventualValueClone::new();
        let i1 = e1.instance();
        let i2 = e1.instance();
        let e1_c1 = e1.clone();
        let jh = spawn("task", async move {
            assert_eq!(i1.await, 5);
            assert_eq!(i2.await, 5);
        });
        sleep(1000).await;
        e1_c1.resolve(5u32).await;
        jh.await;
        e1_c1.reset();
        //
        let j1 = e1.instance();
        let j2 = e1.instance();
        let jh = spawn("task", async move {
            assert_eq!(j1.await, 6);
            assert_eq!(j2.await, 6);
        });
        sleep(1000).await;
        e1_c1.resolve(6u32).await;
        jh.await;
        e1_c1.reset();
    }
}
