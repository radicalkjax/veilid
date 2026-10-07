use crate::*;

use std::thread::sleep;
use std::thread::spawn;

pub fn test_simple_no_contention() {
    info!("test_simple_no_contention");

    let table = TagLockTable::new();

    let a1 = SocketAddr::new("1.2.3.4".parse().unwrap(), 1234);
    let a2 = SocketAddr::new("6.9.6.9".parse().unwrap(), 6969);

    {
        let g1 = table.lock_tag(a1);
        let g2 = table.lock_tag(a2);
        drop(g2);
        drop(g1);
    }

    {
        let g1 = table.lock_tag(a1);
        let g2 = table.lock_tag(a2);
        drop(g1);
        drop(g2);
    }

    assert_eq!(table.len(), 0);
}

pub fn test_simple_single_contention() {
    info!("test_simple_single_contention");

    let table = TagLockTable::new();

    let a1 = SocketAddr::new("1.2.3.4".parse().unwrap(), 1234);

    let table2 = table.clone();
    let t1 = spawn(move || {
        info!("locked");
        // move the guard into the task
        let _g1 = table2.lock_tag(a1);
        // hold the guard for a bit
        info!("waiting");
        sleep(Duration::from_secs(1));
        // release the guard
        info!("released");
    });

    sleep(Duration::from_millis(500));

    // wait to lock again, will contend until spawned task exits
    let _g1_b = table.lock_tag(a1);
    info!("locked");

    // Ensure task is joined
    t1.join().unwrap();

    assert_eq!(table.len(), 1);
}

pub fn test_simple_try() {
    info!("test_simple_try");

    let table = TagLockTable::new();

    let a1 = SocketAddr::new("1.2.3.4".parse().unwrap(), 1234);
    let a2 = SocketAddr::new("1.2.3.5".parse().unwrap(), 1235);

    {
        let _g1 = table.lock_tag(a1);

        let opt_g2 = table.try_lock_tag(a1);
        let opt_g3 = table.try_lock_tag(a2);

        assert!(opt_g2.is_none());
        assert!(opt_g3.is_some());
    }
    let opt_g4 = table.try_lock_tag(a1);
    assert!(opt_g4.is_some());

    assert_eq!(table.len(), 1);
}

pub fn test_simple_double_contention() {
    info!("test_simple_double_contention");

    let table = TagLockTable::new();

    let a1 = SocketAddr::new("1.2.3.4".parse().unwrap(), 1234);
    let a2 = SocketAddr::new("6.9.6.9".parse().unwrap(), 6969);

    let table2 = table.clone();
    let t1 = spawn(move || {
        let _g1 = table2.lock_tag(a1);
        // hold the guard for a bit
        info!("waiting");
        sleep(Duration::from_millis(1500));
        // release the guard
        info!("released");
    });

    let table2 = table.clone();
    let t2 = spawn(move || {
        let _g2 = table2.lock_tag(a2);
        // hold the guard for a bit
        info!("waiting");
        sleep(Duration::from_millis(1000));
        // release the guard
        info!("released");
    });

    info!("locked");

    sleep(Duration::from_millis(500));

    // wait to lock again, will contend until spawned task exits
    let _g1_b = table.lock_tag(a1);
    // wait to lock again, should complete immediately
    let _g2_b = table.lock_tag(a2);

    info!("locked");

    // Ensure tasks are joined
    t1.join().unwrap();
    t2.join().unwrap();

    assert_eq!(table.len(), 2);
}

pub fn test_parallel_single_contention() {
    info!("test_parallel_single_contention");

    let table = TagLockTable::new();

    let a1 = SocketAddr::new("1.2.3.4".parse().unwrap(), 1234);

    let table1 = table.clone();
    let t1 = spawn(move || {
        // lock the tag
        let _g = table1.lock_tag(a1);
        info!("locked t1");
        // hold the guard for a bit
        info!("waiting t1");
        sleep(Duration::from_millis(500));
        // release the guard
        info!("released t1");
    });

    let table2 = table.clone();
    let t2 = spawn(move || {
        // lock the tag
        let _g = table2.lock_tag(a1);
        info!("locked t2");
        // hold the guard for a bit
        info!("waiting t2");
        sleep(Duration::from_millis(500));
        // release the guard
        info!("released t2");
    });

    let table3 = table.clone();
    let t3 = spawn(move || {
        // lock the tag
        let _g = table3.lock_tag(a1);
        info!("locked t3");
        // hold the guard for a bit
        info!("waiting t3");
        sleep(Duration::from_millis(500));
        // release the guard
        info!("released t3");
    });

    // Ensure tasks are joined
    t1.join().unwrap();
    t2.join().unwrap();
    t3.join().unwrap();

    assert_eq!(table.len(), 0);
}

pub fn test_all() {
    test_simple_no_contention();
    test_simple_try();
    test_simple_single_contention();
    test_parallel_single_contention();
}
