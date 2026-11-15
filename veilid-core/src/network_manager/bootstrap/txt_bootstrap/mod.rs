mod v0;
mod v1;

use super::*;

pub use v0::*;
pub use v1::*;

pub const SUPPORTED_BOOTSTRAP_TXT_VERSIONS: [u8; 2] =
    [BOOTSTRAP_TXT_VERSION_0, BOOTSTRAP_TXT_VERSION_1];

impl_veilid_log_facility!("net");

impl NetworkManager {
    /// TXT bootstrap request
    /// Sends a bootstrap request via DNS for TXT records and bootstraps with them
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", err, skip(self))
    )]
    pub async fn txt_bootstrap(&self, hostname: String) -> EyreResult<BootstrapPeers> {
        // Get the minimum bootstrap version we are supporting
        let min_boot_version = self.bootstrap_min_version();

        // Resolve bootstrap servers and recurse their TXT entries
        // This only operates in the PublicInternet routing domain because DNS is inherently
        // for PublicInternet use. Other routing domains may use other mechanisms
        // such as LLMNR/MDNS/DNS-SD.
        let txt_strings =
            match pin_future!(self.resolve_bootstrap_txt_strings(hostname.clone())).await {
                Ok(v) => v,
                Err(e) => {
                    veilid_log!(self debug "Bootstrap resolution failure: {}", e);
                    return Err(e);
                }
            };

        // Cache every record string we were handed, whatever its version, so this node can serve
        // versions it cannot itself parse. A '|' delimited string is a record; a bare one is a
        // v0 sub-name, whose records only appear after the second-level lookup below.
        let mut record_strings: Vec<String> = txt_strings
            .iter()
            .filter(|x| x.contains('|'))
            .cloned()
            .collect();

        // Heuristic to determine which version to parse
        // Take the first record and see if there is a '|' delimited string in it
        // If so, the first field is a record version number.
        // If no '|' delimited string is found, then this is a v0 hostname record
        let mut opt_max_version: Option<u8> = None;

        for txt_string in &txt_strings {
            let v = match txt_string.split_once('|').map(|x| u8::from_str(x.0)) {
                Some(Err(e)) => {
                    // Parse error, skip it
                    veilid_log!(self debug "malformed txt record in bootstrap response: {}\n{}", txt_string, e);
                    continue;
                }
                Some(Ok(v)) => {
                    // Version
                    if SUPPORTED_BOOTSTRAP_TXT_VERSIONS.contains(&v) {
                        v
                    } else {
                        veilid_log!(self debug "unsupported bootstrap record version: {}", txt_string);
                        continue;
                    }
                }
                None => {
                    // No '|' means try version 0
                    0u8
                }
            };

            if v < min_boot_version {
                veilid_log!(self debug "ignoring older bootstrap record version: {}", txt_string);
                continue;
            }

            opt_max_version = opt_max_version.map(|x| x.max(v)).or(Some(v));
        }
        let Some(max_version) = opt_max_version else {
            veilid_log!(self debug "no suitable txt record in bootstrap response");
            return Ok(BootstrapPeers::default());
        };

        // Process the best version available
        let bsrecs = match max_version {
            BOOTSTRAP_TXT_VERSION_0 => {
                // Resolve second-level hostname
                let v0_record_strings = self.resolve_bootstrap_v0(hostname, txt_strings).await?;
                record_strings.extend(v0_record_strings.iter().cloned());

                // Parse v0 records
                let bsrecs = match self.parse_bootstrap_v0(&v0_record_strings).await {
                    Ok(v) => v,
                    Err(e) => {
                        veilid_log!(self debug "Bootstrap v0 parsing failure: {}", e);
                        return Err(e);
                    }
                };

                veilid_log!(self debug "Bootstrap v0 resolution: {:#?}", bsrecs);

                bsrecs
            }
            BOOTSTRAP_TXT_VERSION_1 => {
                // Parse v1 records
                let bsrecs = match self.parse_bootstrap_v1(&txt_strings).await {
                    Ok(v) => v,
                    Err(e) => {
                        veilid_log!(self debug "Bootstrap v1 parsing failure: {}", e);
                        return Err(e);
                    }
                };

                veilid_log!(self debug "Bootstrap v1 resolution: {:#?}", bsrecs);

                bsrecs
            }
            _ => {
                veilid_log!(self debug "unsupported bootstrap version");
                return Ok(BootstrapPeers::default());
            }
        };

        let peers = self.bootstrap_peers_from_records(&bsrecs);

        Ok(BootstrapPeers {
            peers,
            records: bsrecs,
            record_strings,
        })
    }

    /// When the earliest cached TXT answer for these names expires, or None if one has no answer
    /// cached
    pub fn txt_answers_expiration(&self, hostnames: &[String]) -> Option<Timestamp> {
        let cur_ts = Timestamp::now();
        let inner = self.inner.lock();
        let mut opt_expiration: Option<Timestamp> = None;
        for hostname in hostnames {
            let (exp_ts, _) = inner.txt_lookup_cache.peek(hostname)?;
            if *exp_ts <= cur_ts {
                return None;
            }
            opt_expiration = Some(opt_expiration.map_or(*exp_ts, |exp| exp.min(*exp_ts)));
        }
        opt_expiration
    }

    /// Bootstrap resolution from TXT into strings
    /// This is cached as to minimize the number of outbound network requests on bootstrap servers
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", skip(self), ret, err)
    )]
    pub async fn resolve_bootstrap_txt_strings(&self, hostname: String) -> EyreResult<Vec<String>> {
        // Lookup hostname in cache
        let cur_ts = Timestamp::now();
        if let Some(res) = self.inner.lock().txt_lookup_cache.get(&hostname) {
            // Ensure timestamp has not expired
            if cur_ts < res.0 {
                // Return cached strings
                let txt_strings = res.1.clone();
                veilid_log!(self debug " Cached TXT: {:?} => {:?}", hostname, txt_strings);
                return Ok(txt_strings);
            }
        }

        // Not in cache or cache expired
        veilid_log!(self debug "Resolving bootstrap TXT: {:?}", hostname);

        // Get TXT record for bootstrap (bootstrap.veilid.net, bootstrap-v1.veilid.net, or similar)
        let txt_strings = match Resolver::txt_lookup(&hostname).await {
            Ok(v) => v,
            Err(ResolverError::Unsupported) => {
                veilid_log!(self debug "No TXT resolver on this platform for '{}'", hostname);
                return Ok(vec![]);
            }
            Err(e) => {
                veilid_log!(self warn
                    "Network may be down. No bootstrap resolution for '{}': {}",
                    hostname, e
                );
                return Ok(vec![]);
            }
        };
        veilid_log!(self debug " TXT: {:?} => {:?}", hostname, txt_strings);

        // Cache result if we have one
        if !txt_strings.is_empty() {
            let exp_ts = cur_ts.later(TXT_LOOKUP_EXPIRATION);
            self.inner
                .lock()
                .txt_lookup_cache
                .insert(hostname, (exp_ts, txt_strings.clone()));
        }

        Ok(txt_strings)
    }
}
