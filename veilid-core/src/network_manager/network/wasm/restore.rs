//! What the PublicInternet's published state rests on in a browser, and putting it back at
//! startup. A browser shows no interfaces, so the requirement is what config allows; the
//! address families a restored state names are probed again in the background.

use super::*;

/// What a saved PublicInternet state needs unchanged to be published again
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PublicInternetValidity {
    /// Outbound protocols, allowed address families and capabilities, from config
    protocol_config: ProtocolConfig,
}

impl WasmNetwork {
    /// What the PublicInternet's published state rests on now
    pub(super) fn routing_domain_restore_state(
        &self,
        routing_domain: RoutingDomain,
    ) -> Option<PlatformRestoreState> {
        if routing_domain != RoutingDomain::PublicInternet {
            return None;
        }
        Some(PlatformRestoreState {
            validity: serialize_table_value(&self.public_internet_validity()).0,
            state: Bytes::new(),
        })
    }

    /// At startup, put back the PublicInternet's last published state if config still allows
    /// it, and probe its address families again
    pub(super) fn restore_public_internet(&self, controller: &dyn RoutingDomainController) {
        let Some(restore_point) = controller.read_dyn().get_restore_point() else {
            veilid_log!(self debug "[PublicInternet] no published state to restore");
            return;
        };
        let saved: Option<PublicInternetValidity> = self
            .table_store()
            .try_deserialize(&TableDBSerializedData(
                restore_point.platform.validity.clone(),
            ))
            .ok();
        if saved.as_ref() != Some(&self.public_internet_validity()) {
            veilid_log!(self info "[PublicInternet] not restoring the last published state, changed: config");
            // Transport failures blamed on peers may have been the old config's
            self.routing_table().clear_all_transport_evidence();
            return;
        }

        let same_peer_info = controller.restore(&restore_point);
        self.inner
            .lock()
            .verifying_address_families
            .insert(RoutingDomain::PublicInternet);
        veilid_log!(self info "[PublicInternet] restored the last published state, {}",
            if same_peer_info { "with its signed peer info" } else { "signing a new peer info" });
    }

    fn public_internet_validity(&self) -> PublicInternetValidity {
        PublicInternetValidity {
            protocol_config: self.inner.lock().protocol_config.clone(),
        }
    }
}
