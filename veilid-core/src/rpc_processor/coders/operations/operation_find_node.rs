use super::*;

const MAX_FIND_NODE_A_PEERS_LEN: usize = veilid_capnp::MAX_FIND_NODE_A_PEERS_LEN as usize;

#[derive(Debug, Clone)]
pub(in crate::rpc_processor) struct RPCOperationFindNodeQ {
    hash_coordinate: HashCoordinate,
    capabilities: Vec<VeilidCapability>,
    fanout_config: FanoutConfig,
}

impl RPCOperationFindNodeQ {
    pub fn try_new(
        hash_coordinate: HashCoordinate,
        capabilities: Vec<VeilidCapability>,
        fanout_config: FanoutConfig,
    ) -> Result<Self, RPCError> {
        if capabilities.len() > MAX_CAPABILITIES {
            return Err(RPCError::internal("capabilities length too long"));
        }

        Ok(Self {
            hash_coordinate,
            capabilities,
            fanout_config,
        })
    }

    // pub async fn validate(
    //     &self,
    //     _validate_context: &RPCValidateContext<'_>,
    // ) -> Result<(), RPCError> {
    //     Ok(())
    // }

    pub fn destructure(self) -> (HashCoordinate, Vec<VeilidCapability>, FanoutConfig) {
        (self.hash_coordinate, self.capabilities, self.fanout_config)
    }

    pub fn decode(
        _decode_context: &RPCDecodeContext,
        reader: &veilid_capnp::operation_find_node_q::Reader,
    ) -> Result<Self, RPCError> {
        rpc_ignore_missing_property!(reader, node_id);
        let ni_reader = reader.get_node_id()?;
        let node_id = decode_node_id(&ni_reader)?;
        if node_id.ref_value().len() != HASH_COORDINATE_LENGTH {
            return Err(RPCError::protocol("invalid node id length"));
        }

        rpc_ignore_missing_property!(reader, capabilities);
        let cap_reader = reader.get_capabilities()?;
        rpc_ignore_max_len!(cap_reader, MAX_CAPABILITIES);
        let capabilities = cap_reader
            .as_slice()
            .map(|s| {
                s.iter()
                    .map(|x| VeilidCapability::from(x.to_be_bytes()))
                    .collect()
            })
            .unwrap_or_default();

        let fanout_config = FanoutConfig::decode(&reader.get_fanout_config()?);

        Self::try_new(node_id.to_hash_coordinate(), capabilities, fanout_config)
    }
    pub fn encode(
        &self,
        builder: &mut veilid_capnp::operation_find_node_q::Builder,
    ) -> Result<(), RPCError> {
        let mut ni_builder = builder.reborrow().init_node_id();
        encode_node_id(
            &NodeId::new(
                self.hash_coordinate.kind(),
                BareNodeId::new_from_bytes(self.hash_coordinate.ref_value().bytes()),
            ),
            &mut ni_builder,
        );

        let mut cap_builder = builder
            .reborrow()
            .init_capabilities(self.capabilities.len() as u32);
        if let Some(s) = cap_builder.as_slice() {
            let capvec: Vec<u32> = self.capabilities.iter().copied().map(u32::from).collect();

            s.clone_from_slice(&capvec);
        }
        self.fanout_config
            .encode(&mut builder.reborrow().init_fanout_config());
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone)]
pub(in crate::rpc_processor) struct RPCOperationFindNodeA {
    peers: Vec<Arc<UnverifiedPeerInfo>>,
}

impl RPCOperationFindNodeA {
    pub fn try_new(peers: Vec<Arc<UnverifiedPeerInfo>>) -> Result<Self, RPCError> {
        if peers.len() > MAX_FIND_NODE_A_PEERS_LEN {
            return Err(RPCError::internal(
                "encoded find node peers length too long",
            ));
        }

        Ok(Self { peers })
    }

    // pub async fn validate(
    //     &self,
    //     _validate_context: &RPCValidateContext<'_>,
    // ) -> Result<(), RPCError> {
    //     Ok(())
    // }

    pub fn destructure(self) -> Vec<Arc<UnverifiedPeerInfo>> {
        self.peers
    }

    pub fn decode(
        decode_context: &RPCDecodeContext,
        reader: &veilid_capnp::operation_find_node_a::Reader,
    ) -> Result<RPCOperationFindNodeA, RPCError> {
        rpc_ignore_missing_property!(reader, peers);
        let peers_reader = reader.get_peers()?;
        let peers_len = rpc_ignore_max_len!(peers_reader, MAX_FIND_NODE_A_PEERS_LEN);
        let mut peers = Vec::<Arc<UnverifiedPeerInfo>>::with_capacity(peers_len);
        for p in peers_reader.iter() {
            let Some(peer_info) = decode_peer_info(decode_context, &p).ignore_ok()? else {
                continue;
            };
            peers.push(Arc::new(peer_info));
        }

        Self::try_new(peers)
    }
    pub fn encode(
        &self,
        builder: &mut veilid_capnp::operation_find_node_a::Builder,
    ) -> Result<(), RPCError> {
        let mut peers_builder = builder.reborrow().init_peers(
            self.peers
                .len()
                .try_into()
                .map_err(RPCError::map_internal("invalid closest nodes list length"))?,
        );
        for (i, peer) in self.peers.iter().enumerate() {
            let mut pi_builder = peers_builder.reborrow().get(i as u32);
            encode_peer_info(peer, &mut pi_builder)?;
        }
        Ok(())
    }
}
