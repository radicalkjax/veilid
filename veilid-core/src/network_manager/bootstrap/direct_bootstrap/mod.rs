mod v0;
mod v1;

use super::*;
use v1::*;

impl_veilid_log_facility!("net");

impl NetworkManager {
    /// Direct bootstrap request
    /// Sends a bootstrap request to a dialinfo and returns the list of peers to bootstrap with
    /// If no bootstrap keys are specified, uses the v0 mechanism, otherwise uses the v1 mechanism
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", err, skip(self), fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn direct_bootstrap(&self, dial_info: DialInfo) -> EyreResult<BootstrapPeers> {
        if self.routing_table().matches_own_dial_info(&dial_info) {
            veilid_log!(self debug "Ignoring own dial info in bootstrap list: {}", dial_info);
            return Ok(BootstrapPeers::default());
        }

        let direct_boot_version = if self
            .config()
            .network
            .routing_table
            .bootstrap_keys
            .is_empty()
        {
            0
        } else {
            1
        };

        if direct_boot_version == 0 {
            let peers = self.direct_bootstrap_v0(dial_info).await?;
            Ok(BootstrapPeers {
                peers,
                records: vec![],
                record_strings: vec![],
            })
        } else {
            self.direct_bootstrap_v1(dial_info).await
        }
    }

    /// A JSON peer is trusted only for its signed node info bytes; everything else in it is
    /// rebuilt from them, so a tampered or spoofed reply cannot attach its own dial info,
    /// node ids or timestamp to a bootstrap node's signature
    pub(crate) fn bind_bootstrap_peer(&self, peer_info: &PeerInfo) -> Option<Arc<PeerInfo>> {
        let routing_table = self.routing_table();
        let unverified = match UnverifiedPeerInfo::try_new_from_wire(
            &routing_table,
            peer_info.routing_domain(),
            peer_info.node_info_message(),
            peer_info.signatures().clone(),
        ) {
            Ok(Some(unverified)) => unverified,
            Ok(None) => {
                veilid_log!(self debug "Dropping bootstrap peer {} with no usable signature or routing domain", peer_info.node_ids());
                return None;
            }
            Err(e) => {
                veilid_log!(self debug "Dropping bootstrap peer {}: {}", peer_info.node_ids(), e);
                return None;
            }
        };
        match unverified.verify_and_extract(&routing_table) {
            Ok(peer_info) => Some(Arc::new(peer_info)),
            Err(e) => {
                veilid_log!(self debug "Dropping bootstrap peer {}: {}", peer_info.node_ids(), e);
                None
            }
        }
    }

    /// Uses the bootstrap v0 (BOOT) mechanism
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", err, skip(self), fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    async fn direct_bootstrap_v0(&self, dial_info: DialInfo) -> EyreResult<Vec<Arc<PeerInfo>>> {
        let timeout_ms = self.config().internal().network.rpc.timeout_ms;
        // Send boot magic to requested peer address
        let data = Bytes::copy_from_slice(BOOT_MAGIC);

        // Our own transport is suppressed, so this says nothing about the bootstrap node
        let NetworkAttempt::Performed(attempt) = self
            .net()
            .send_recv_data_unbound_to_dial_info(
                dial_info.clone(),
                data.clone(),
                SAFE_TRANSPORT_MTU,
                timeout_ms,
            )
            .await?
        else {
            return Ok(Vec::new());
        };
        let out_data = network_result_value_or_log!(self attempt => [ format!(": dial_info={}, data.len={}", dial_info, data.len()) ]
        {
            return Ok(Vec::new());
        });

        let bootstrap_peerinfo_str =
            std::str::from_utf8(&out_data).wrap_err("bad utf8 in boot peerinfo")?;

        let bootstrap_peerinfo: Vec<PeerInfo> = match deserialize_json(bootstrap_peerinfo_str) {
            Ok(v) => v,
            Err(e) => {
                error!("{}", e);
                return Err(e).wrap_err("failed to deserialize peerinfo");
            }
        };

        Ok(bootstrap_peerinfo
            .iter()
            .filter_map(|pi| self.bind_bootstrap_peer(pi))
            .collect())
    }

    /// Uses the bootstrap v1 (B01T) mechanism
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", err, skip(self), fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    async fn direct_bootstrap_v1(&self, dial_info: DialInfo) -> EyreResult<BootstrapPeers> {
        let timeout_ms = self.config().internal().network.rpc.timeout_ms;

        // Send boot magic to requested peer address
        let data = Bytes::copy_from_slice(B01T_MAGIC);

        // Our own transport is suppressed, so this says nothing about the bootstrap node
        let NetworkAttempt::Performed(attempt) = self
            .net()
            .send_recv_data_unbound_to_dial_info(
                dial_info.clone(),
                data.clone(),
                SAFE_TRANSPORT_MTU,
                timeout_ms,
            )
            .await?
        else {
            return Ok(BootstrapPeers::default());
        };
        let out_data = network_result_value_or_log!(self attempt => [ format!(": dial_info={}, data.len={}", dial_info, data.len()) ]
        {
            return Ok(BootstrapPeers::default());
        });

        let bootv1response_str =
            std::str::from_utf8(&out_data).wrap_err("bad utf8 in bootstrap v1 records")?;

        veilid_log!(self debug "Direct bootstrap v1 response: {}", bootv1response_str);

        let bootv1response: BootV1Response = match deserialize_json(bootv1response_str) {
            Ok(v) => v,
            Err(e) => {
                error!("{}", e);
                return Err(e).wrap_err("failed to deserialize bootstrap v1 response");
            }
        };

        // Parse v1 records
        let bsrecs = match self.parse_bootstrap_v1(&bootv1response.records).await {
            Ok(v) => v,
            Err(e) => {
                veilid_log!(self debug "Direct bootstrap v1 parsing failure: {}", e);
                return Err(e);
            }
        };

        veilid_log!(self debug "Direct bootstrap v1 resolution: {:#?}", bsrecs);

        // Prefer the signed peer info the bootstrap returned for a record.
        // This is important for browser websocket bootstrapping where the dialinfo in the
        // bootstrap record has an unspecified IP address, and as such a routing domain can not
        // be determined for it by the code that receives the FindNodeA result.
        // The rest fall back to unsigned peer info built from the record itself.
        let signed_peers: Vec<Arc<PeerInfo>> = bootv1response
            .peers
            .iter()
            .filter_map(|pi| self.bind_bootstrap_peer(pi))
            .collect();
        let mut peers = Vec::<Arc<PeerInfo>>::new();
        let mut unsigned_bsrecs = Vec::<BootstrapRecord>::new();
        for bsrec in bsrecs.iter() {
            match signed_peers.iter().find(|pi| {
                pi.node_info()
                    .public_keys()
                    .contains_any_from_iter(bsrec.public_keys().iter())
            }) {
                Some(pi) => peers.push(pi.clone()),
                None => unsigned_bsrecs.push(bsrec.clone()),
            }
        }
        peers.extend(self.bootstrap_peers_from_records(&unsigned_bsrecs));

        Ok(BootstrapPeers {
            peers,
            records: bsrecs,
            record_strings: bootv1response.records,
        })
    }
}
