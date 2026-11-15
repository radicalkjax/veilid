pub mod test_api_log_queue;
pub mod test_log_throttle;

use super::*;

pub async fn test_all() {
    test_api_log_queue::test_all().await;
    test_log_throttle::test_all();
}
