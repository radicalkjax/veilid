use crate::*;

pub fn test_get_random_u64() {
    info!("testing random number generator for u64");
    let t1 = get_raw_timestamp();
    let count = 10000;
    for _ in 0..count {
        let _ = get_random_u64();
    }
    let t2 = get_raw_timestamp();
    let tdiff = ((t2 - t1) as f64) / 1000000.0f64;
    info!(
        "running get_random_u64 with {} iterations took {} seconds",
        count, tdiff
    );
}

pub fn test_get_random_u32() {
    info!("testing random number generator for u32");
    let t1 = get_raw_timestamp();
    let count = 10000;
    for _ in 0..count {
        let _ = get_random_u32();
    }
    let t2 = get_raw_timestamp();
    let tdiff = ((t2 - t1) as f64) / 1000000.0f64;
    info!(
        "running get_random_u32 with {} iterations took {} seconds",
        count, tdiff
    );
}
