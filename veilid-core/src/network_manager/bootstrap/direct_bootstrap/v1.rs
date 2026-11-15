use super::*;

impl_veilid_log_facility!("net");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BootV1Response {
    pub records: Vec<String>,
    pub peers: Vec<Arc<PeerInfo>>,
}

impl NetworkManager {
    /// Build the B01T response from the routing domain's last successful bootstrap.
    /// Answers an unauthenticated packet, so this touches no resolver and does no I/O:
    /// records come from the routing domain, peer info from the routing table.
    pub fn boot_v1_response(&self) -> Option<BootV1Response> {
        let routing_table = self.routing_table();
        let routing_domain = RoutingDomain::PublicInternet;
        let cache = routing_table.get_served_bootstrap_cache(routing_domain);

        // Served verbatim, including versions this node could not parse
        let records = cache.record_strings().to_vec();
        if records.is_empty() {
            return None;
        }

        // Add peer infos if we have them, only for the peers present in the records
        let peers: Vec<Arc<PeerInfo>> = cache
            .records()
            .iter()
            .filter_map(|bsrec| {
                if routing_table.matches_own_public_key(bsrec.public_keys()) {
                    routing_table.get_published_peer_info(routing_domain)
                } else {
                    let best_public_key = bsrec.public_keys().first()?;
                    let best_node_id = routing_table.generate_node_id(best_public_key).ok()?;
                    let nr = routing_table.lookup_node_id(best_node_id).ok().flatten()?;
                    nr.get_peer_info(routing_domain)
                }
            })
            .collect();

        Some(BootV1Response { records, peers })
    }

    /// Direct bootstrap v1 request handler
    /// This is a proxy mechanism to the TXT bootstrap mechanism
    /// that is intended for supporting WS/WSS nodes that can not perform DNS TXT lookups,
    /// however this does work over straight UDP and TCP protocols as well.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", skip(self), ret, err, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn handle_boot_v1_request(&self, flow: Flow) -> EyreResult<NetworkResult<()>> {
        let Some(bootv1response) = self.boot_v1_response() else {
            return Ok(NetworkResult::service_unavailable("no bootstrap records"));
        };

        // Serialize out bootstrap response
        let json_bytes: Bytes = serialize_json(bootv1response).into();
        veilid_log!(self trace "B01T reponse: {}", String::from_utf8_lossy(&json_bytes));

        // Reply with bootstrap records
        let net = self.net();
        match pin_future_closure!(net.send_data_to_existing_flow(
            flow,
            json_bytes,
            SAFE_TRANSPORT_MTU
        ))
        .await?
        {
            NetworkAttempt::Performed(NetworkResult::Value(_)) => Ok(NetworkResult::value(())),
            NetworkAttempt::Performed(NetworkResult::Timeout) => Ok(NetworkResult::timeout()),
            _ => Ok(NetworkResult::no_connection_other(
                "bootstrap reply could not be sent",
            )),
        }
    }
}
