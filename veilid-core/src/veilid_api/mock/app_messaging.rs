use super::*;

/// Private routes allocated in this process, mapped to the node that answers them.
///
/// The blob is the route id itself: both ends live in this process, so there is nothing to encode
/// for a hop that does not exist.
static MOCK_ROUTES: LazyLock<Mutex<HashMap<RouteId, MockNodeKey>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// App calls waiting for a reply, each holding the channel its caller is parked on.
static MOCK_APP_CALLS: LazyLock<Mutex<HashMap<OperationId, flume::Sender<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl VeilidAPI {
    /// Allocate a private route answered by this node.
    pub(crate) async fn mock_new_private_route(&self) -> VeilidAPIResult<RouteBlob> {
        let key = self.mock_key()?;
        let crypto = self.crypto()?;
        let kind = VALID_CRYPTO_KINDS
            .first()
            .copied()
            .ok_or_else(|| VeilidAPIError::generic("no cryptosystem"))?;
        let Some(vcrypto) = crypto.get_async(kind) else {
            apibail_generic!("unsupported cryptosystem");
        };

        let secret = vcrypto.random_shared_secret().await;
        let hash = vcrypto
            .generate_hash(Bytes::copy_from_slice(secret.ref_value()))
            .await;
        let route_id = RouteId::new(kind, BareRouteId::new(hash.ref_value()));

        MOCK_ROUTES.lock().insert(route_id.clone(), key);
        Ok(RouteBlob {
            route_id: route_id.clone(),
            blob: route_id.to_string().into_bytes(),
        })
    }

    /// Adopt a route blob allocated elsewhere in this process.
    pub(crate) fn mock_import_remote_private_route(
        &self,
        blob: Vec<u8>,
    ) -> VeilidAPIResult<RouteId> {
        let s = String::from_utf8(blob)
            .map_err(|_| VeilidAPIError::parse_error("invalid route blob", "blob"))?;
        let route_id = RouteId::from_str(&s)
            .map_err(|_| VeilidAPIError::parse_error("invalid route blob", s.clone()))?;

        let Some(owner) = MOCK_ROUTES.lock().get(&route_id).copied() else {
            apibail_invalid_argument!("unknown route", "blob", s);
        };

        // An imported route gets its own id, so a self-import leaves two ids to release
        let crypto = self.crypto()?;
        let Some(vcrypto) = crypto.get(route_id.kind()) else {
            apibail_generic!("unsupported cryptosystem");
        };
        let mut seed = route_id.value().to_string().into_bytes();
        seed.extend_from_slice(b"imported");
        let hash = vcrypto.generate_hash(&seed);
        let imported = RouteId::new(route_id.kind(), BareRouteId::new(hash.ref_value()));

        MOCK_ROUTES.lock().insert(imported.clone(), owner);
        Ok(imported)
    }

    /// Drop a private route.
    pub(crate) fn mock_release_private_route(&self, route_id: RouteId) -> VeilidAPIResult<()> {
        let crypto = self.crypto()?;
        check_route_id(&crypto, &route_id)?;
        if MOCK_ROUTES.lock().remove(&route_id).is_none() {
            apibail_invalid_argument!("unknown route", "route_id", route_id);
        }
        Ok(())
    }

    /// Resolve a target to the node that should receive the message.
    fn mock_resolve_target(
        &self,
        target: &Target,
    ) -> VeilidAPIResult<(MockNodeKey, Option<RouteId>)> {
        match target {
            Target::RouteId(route_id) => {
                let owner =
                    MOCK_ROUTES.lock().get(route_id).copied().ok_or_else(|| {
                        VeilidAPIError::invalid_target("unknown route".to_owned())
                    })?;
                Ok((owner, Some(route_id.clone())))
            }
            Target::NodeId(node_id) => {
                let owner = MockNetwork::find_node_by_id(node_id)
                    .ok_or_else(|| VeilidAPIError::invalid_target("unknown node".to_owned()))?;
                Ok((owner, None))
            }
        }
    }

    /// Deliver a one-way message to the target.
    pub(crate) fn mock_app_message(&self, target: Target, message: Vec<u8>) -> VeilidAPIResult<()> {
        let (owner, route_id) = self.mock_resolve_target(&target)?;
        MockNetwork::send_update(
            &owner,
            VeilidUpdate::AppMessage(Box::new(VeilidAppMessage::new(None, route_id, message))),
        )
    }

    /// Deliver a message and wait for the target's reply.
    pub(crate) async fn mock_app_call(
        &self,
        target: Target,
        message: Vec<u8>,
    ) -> VeilidAPIResult<Vec<u8>> {
        let (owner, route_id) = self.mock_resolve_target(&target)?;
        let call_id = OperationId::new(get_random_u64());

        let (tx, rx) = flume::bounded(1);
        MOCK_APP_CALLS.lock().insert(call_id, tx);

        // Delivered with no lock held: the receiver answers by re-entering the API
        let sent = MockNetwork::send_update(
            &owner,
            VeilidUpdate::AppCall(Box::new(VeilidAppCall::new(
                None, route_id, message, call_id,
            ))),
        );
        if let Err(e) = sent {
            MOCK_APP_CALLS.lock().remove(&call_id);
            return Err(e);
        }

        rx.recv_async()
            .await
            .map_err(|_| VeilidAPIError::generic("app call was never answered"))
    }

    /// Answer a call this node received. Each call may be answered once.
    pub(crate) fn mock_app_call_reply(
        &self,
        call_id: OperationId,
        message: Vec<u8>,
    ) -> VeilidAPIResult<()> {
        let tx = MOCK_APP_CALLS
            .lock()
            .remove(&call_id)
            .ok_or_else(|| VeilidAPIError::generic("unknown or already answered app call"))?;
        tx.send(message)
            .map_err(|_| VeilidAPIError::generic("app call caller is gone"))
    }
}
