use super::*;

impl StorageManager {
    /// Builds the record key for a given schema and owner
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all)
    )]
    pub async fn get_record_key(
        &self,
        schema: DHTSchema,
        owner_key: &PublicKey,
        encryption_key: Option<SharedSecret>,
    ) -> VeilidAPIResult<RecordKey> {
        dht_record_key(&self.crypto(), schema, owner_key, encryption_key).await
    }

    /// Validate a record key
    pub fn check_record_key(&self, record_key: &RecordKey) -> VeilidAPIResult<()> {
        check_dht_record_key(&self.crypto(), record_key)
    }
}
