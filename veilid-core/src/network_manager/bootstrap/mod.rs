mod bootstrap_record;
mod debug;
mod dial_info_converter;
mod direct_bootstrap;
mod txt_bootstrap;

use super::*;

impl_veilid_log_facility!("net");

/// Port a bootstrap host answers B01T on when a TXT name has to be reached directly
pub const BOOTSTRAP_DIRECT_PORT: u16 = 5150;

/// Ceiling on cached record strings. Far above any real bootstrap's output, and keeps a hostile
/// bootstrap from inflating the B01T response of every node that contacted it.
pub const MAX_BOOTSTRAP_RECORD_STRINGS: usize = 64;

pub use bootstrap_record::*;
pub use dial_info_converter::*;
pub use txt_bootstrap::*;

/// Result of one bootstrap resolution: peers to bootstrap with, the records they were parsed from,
/// and every record string the source returned, whatever its version
#[derive(Debug, Default)]
pub struct BootstrapPeers {
    pub peers: Vec<Arc<PeerInfo>>,
    pub records: Vec<BootstrapRecord>,
    pub record_strings: Vec<String>,
}

/// What a bootstrap recorded, kept across restarts so B01T can be answered without a lookup.
/// Strings are served verbatim; records exist so the peer list needs no parsing on the serve path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapCache {
    record_strings: Vec<String>,
    records: Vec<BootstrapRecord>,
}

impl BootstrapCache {
    pub fn new(record_strings: Vec<String>, records: Vec<BootstrapRecord>) -> Self {
        let mut this = Self {
            record_strings,
            records,
        };
        this.normalize();
        this
    }

    pub fn record_strings(&self) -> &[String] {
        &self.record_strings
    }

    pub fn records(&self) -> &[BootstrapRecord] {
        &self.records
    }

    pub fn is_empty(&self) -> bool {
        self.record_strings.is_empty()
    }

    /// Fold another cache in, keeping both sides
    pub fn union(&mut self, other: BootstrapCache) {
        self.record_strings.extend(other.record_strings);
        self.records.extend(other.records);
        self.normalize();
    }

    fn normalize(&mut self) {
        self.record_strings.sort_unstable();
        self.record_strings.dedup();
        self.record_strings.truncate(MAX_BOOTSTRAP_RECORD_STRINGS);
        self.records = BootstrapRecord::merge_all(core::mem::take(&mut self.records));
    }
}

impl NetworkManager {
    /// The oldest record version this node will bootstrap itself from.
    /// No keys to check signatures with allows v0; keys require at least v1.
    pub fn bootstrap_min_version(&self) -> u8 {
        if self
            .config()
            .network
            .routing_table
            .bootstrap_keys
            .is_empty()
        {
            BOOTSTRAP_TXT_VERSION_0
        } else {
            BOOTSTRAP_TXT_VERSION_1
        }
    }

    /// Turn bootstrap records into unsigned peer info to bootstrap against.
    /// A bootstrap is rejected later if a FindNodeQ can't produce its signed peer info.
    pub fn bootstrap_peers_from_records(&self, bsrecs: &[BootstrapRecord]) -> Vec<Arc<PeerInfo>> {
        let routing_table = self.routing_table();

        bsrecs
            .iter()
            .filter_map(|bsrec| {
                // Get crypto support from list of node ids
                let crypto_info_list: Vec<CryptoInfo> = bsrec.public_keys().iter().filter_map(|pk| {
                    match pk.kind() {
                        CRYPTO_KIND_VLD0 =>
                            Some(CryptoInfo::VLD0 { public_key: pk.value() }),

                        ck => {
                            veilid_log!(self warn "Ignoring unsupported bootstrap crypto kind: {}", ck);
                            None
                        }
                    }
                }).collect();

                // Make unsigned node info
                // Use the current time for the timestamp since any older bootstrap peer info
                // needs to be overwritten for this to be guaranteed to work
                let ni = NodeInfo::new(
                    Timestamp::now(),
                    bsrec.envelope_support().to_vec(), // Envelope support is as specified in the bootstrap list
                    crypto_info_list, // Crypto support is derived from list of node ids
                    vec![],           // Bootstrap needs no capabilities
                    ProtocolTypeSet::all(), // Bootstraps are always capable of all protocols
                    AddressTypeSet::all(), // Bootstraps are always IPV4 and IPV6 capable
                    bsrec.dial_info_details().to_vec(), // Dial info is as specified in the bootstrap list
                    vec![],                             // No relays for bootstrap nodes
                    TransportMtus::default(), // Their mtus arrive with their signed peer info
                );

                match PeerInfo::try_new_from_unsigned(
                    &routing_table,
                    RoutingDomain::PublicInternet,
                    ni,
                ) {
                    Ok(v) => Some(Arc::new(v)),
                    Err(e) => {
                        veilid_log!(self error "Bootstrap has invalid peer info: {}", e);
                        None
                    }
                }
            })
            .collect()
    }

    /// The bootstrap records supplied directly in the config, verified but never resolved.
    /// Every supported version is kept, so this node serves what it was configured with
    /// whether or not it could bootstrap from it. No lookup of any kind.
    pub async fn config_bootstrap_cache(&self) -> BootstrapCache {
        let (record_strings, records) = self
            .parse_config_bootstrap_records(None, &SUPPORTED_BOOTSTRAP_TXT_VERSIONS)
            .await;
        BootstrapCache::new(record_strings, records)
    }

    /// The config bootstrap records as a bootstrap source. This is the only path that resolves
    /// them, and it runs only when this node is bootstrapping itself. Bootstrapping
    /// takes the newest version this node will accept, the way a TXT bootstrap does.
    pub async fn config_bootstrap(&self) -> EyreResult<BootstrapPeers> {
        let record_strings = self
            .config()
            .network
            .routing_table
            .bootstrap_records
            .clone();
        let min_version = self.bootstrap_min_version();
        let Some(max_version) = record_strings
            .iter()
            .filter_map(|x| Self::bootstrap_record_version(x))
            .filter(|v| *v >= min_version)
            .max()
        else {
            veilid_log!(self debug "no usable version among the configured bootstrap records");
            return Ok(BootstrapPeers::default());
        };

        let dial_info_converter = BootstrapDialInfoConverter::default();
        let (_, records) = self
            .parse_config_bootstrap_records(Some(&dial_info_converter), &[max_version])
            .await;
        let peers = self.bootstrap_peers_from_records(&records);

        // Serving these is the seeding path's job, so no record strings are reported here
        Ok(BootstrapPeers {
            peers,
            records,
            record_strings: vec![],
        })
    }

    /// The version a record string declares, if this build supports it
    fn bootstrap_record_version(record_string: &str) -> Option<u8> {
        let version = record_string
            .trim()
            .split_once('|')
            .and_then(|x| u8::from_str(x.0).ok())?;
        SUPPORTED_BOOTSTRAP_TXT_VERSIONS
            .contains(&version)
            .then_some(version)
    }

    /// Parse the configured bootstrap records one at a time so one bad entry doesn't discard
    /// the rest. Returns the strings that parsed and the records they parsed to.
    async fn parse_config_bootstrap_records(
        &self,
        opt_dial_info_converter: Option<&dyn DialInfoConverter>,
        versions: &[u8],
    ) -> (Vec<String>, Vec<BootstrapRecord>) {
        let (record_strings, signing_keys) = {
            let cfg = self.config();
            let rt = &cfg.network.routing_table;
            (rt.bootstrap_records.clone(), rt.bootstrap_keys.clone())
        };

        let mut kept_strings = Vec::new();
        let mut records = Vec::new();
        for record_string in &record_strings {
            let Some(version) = Self::bootstrap_record_version(record_string) else {
                veilid_log!(self warn "Ignoring unsupported configured bootstrap record: {}", record_string);
                continue;
            };
            if !versions.contains(&version) {
                continue;
            }

            let result = if version == BOOTSTRAP_TXT_VERSION_0 {
                BootstrapRecord::try_new_from_v0_str(self, opt_dial_info_converter, record_string)
                    .await
            } else {
                BootstrapRecord::try_new_from_v1_str(
                    self,
                    opt_dial_info_converter,
                    record_string,
                    &signing_keys,
                )
                .await
            };

            match result {
                Ok(Some(record)) => {
                    kept_strings.push(record_string.trim().to_owned());
                    records.push(record);
                }
                Ok(None) => {
                    veilid_log!(self debug "Configured bootstrap record has no reachable dial info: {}", record_string);
                }
                Err(e) => {
                    veilid_log!(self warn "Ignoring invalid configured bootstrap record: {}: {}", record_string, e);
                }
            }
        }

        (kept_strings, BootstrapRecord::merge_all(records))
    }
}
