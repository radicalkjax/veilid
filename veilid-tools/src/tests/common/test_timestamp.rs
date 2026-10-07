use crate::*;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        use js_sys::*;
    } else {
        use std::time::{Duration, SystemTime};
    }
}

pub fn test_get_raw_timestamp() {
    info!("testing get_timestamp");
    let t1 = get_raw_timestamp();
    let t2 = get_raw_timestamp();
    assert!(t2 >= t1);
}

pub async fn test_sleep() {
    info!("testing sleep");
    cfg_if! {
        if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {

            let t1 = Date::now();
            sleep(1000).await;
            let t2 = Date::now();
            assert!((t2-t1) >= 1000.0);

        } else {

            let sys_time = SystemTime::now();
            let one_sec = Duration::from_secs(1);

            sleep(1000).await;
            assert!(sys_time.elapsed().unwrap() >= one_sec);
        }
    }
}
