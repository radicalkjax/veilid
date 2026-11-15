use super::*;

impl StorageManager {
    /// Produce member id from writer public key
    pub fn generate_member_id(&self, writer_key: &PublicKey) -> VeilidAPIResult<MemberId> {
        generate_member_id(&self.crypto(), writer_key)
    }

    /// Check a subkey value data against the schema
    pub fn check_subkey_value_data(
        &self,
        schema: &DHTSchema,
        owner: &PublicKey,
        subkey: ValueSubkey,
        value_data: &EncryptedValueData,
    ) -> VeilidAPIResult<()> {
        check_subkey_value_data(
            &self.crypto(),
            schema,
            owner,
            subkey,
            &value_data.writer(),
            value_data.data_size(),
        )
    }
}
