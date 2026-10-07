use crate::*;

// TrackedMutex is a drop-in parking_lot::Mutex for the ordinary (non-deadlock) paths.
// These cover acquire/mutate/release and contention via try_lock. The deadlock-detection
// path calls std::process::exit and so is exercised by integration reproduction, not here.
// Runs under debug-locks on both native and wasm32-unknown-unknown.

pub fn test_lock_mutate_persist() {
    info!("test_lock_mutate_persist");

    let m = TrackedMutex::new(0u32);
    {
        let mut g = m.lock();
        *g += 1;
        assert_eq!(*g, 1);
    }
    // A fresh acquire after release sees the previous mutation.
    {
        let mut g = m.lock();
        *g += 41;
    }
    assert_eq!(*m.lock(), 42);
}

pub fn test_try_lock_contention() {
    info!("test_try_lock_contention");

    let m = TrackedMutex::new(7u32);

    let g = m.try_lock().expect("try_lock should succeed when free");
    assert_eq!(*g, 7);

    // While held, try_lock reports contention by returning None — never blocking or reporting.
    assert!(m.try_lock().is_none(), "try_lock must fail while held");

    drop(g);
    assert!(
        m.try_lock().is_some(),
        "try_lock must succeed after release"
    );
}

pub fn test_all() {
    test_lock_mutate_persist();
    test_try_lock_contention();
}
