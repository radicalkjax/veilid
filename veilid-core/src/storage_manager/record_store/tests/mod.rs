mod test_inbound_expiry;

use super::*;

pub async fn test_record_store() {
    record_store_inner::tests::test_record_store_inner().await;
    test_inbound_expiry::test_all().await;
}
