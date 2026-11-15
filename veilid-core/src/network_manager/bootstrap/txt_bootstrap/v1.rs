use super::*;

pub const BOOTSTRAP_TXT_VERSION_1: u8 = 1;

impl_veilid_log_facility!("net");

impl NetworkManager {
    /// Parse v1 bootstrap record strings into BootstrapRecord structs
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", skip(self), ret, err, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn parse_bootstrap_v1(
        &self,
        record_strings: &[String],
    ) -> EyreResult<Vec<BootstrapRecord>> {
        veilid_log!(self debug "Parsing v1 bootstraps: {:?}", record_strings);

        let signing_keys = self.config().network.routing_table.bootstrap_keys.clone();
        if signing_keys.is_empty() {
            veilid_log!(self warn "No signing keys in config. Proceeding with UNVERIFIED bootstrap.");
        }

        // For each record string resolve into BootstrapRecord pairs
        let dial_info_converter = BootstrapDialInfoConverter::default();

        let mut bootstrap_records: Vec<BootstrapRecord> = Vec::new();
        for record_string in record_strings {
            let Some(bootstrap_record) = BootstrapRecord::try_new_from_v1_str(
                self,
                Some(&dial_info_converter),
                record_string,
                &signing_keys,
            )
            .await?
            else {
                continue;
            };

            bootstrap_records.push(bootstrap_record);
        }

        Ok(BootstrapRecord::merge_all(bootstrap_records))
    }
}
