use crate::*;

pub fn test_log() {
    info!("testing log");
}

pub fn test_tools() {
    info!("testing retry_falloff_log");
    let mut last_us = 0u64;
    for x in 0..1024 {
        let cur_us = x as u64 * 1000000u64;
        if retry_falloff_log(last_us, cur_us, 10_000_000u64, 6_000_000_000u64, 2.0f64) {
            info!("   retry at {} secs", timestamp_duration_to_secs(cur_us));
            last_us = cur_us;
        }
    }
}
