use super::*;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerInfo {
    /// The routing domain this peer info was translatd to from its origin routing domain.
    /// This is the 'best' routing domain for this peer info at the time it was received.
    /// This may differ from the -set- of routing domains for which its node info is -valid- at the given time
    /// Local networks, for example, can change, making this routing domain no longer one of the ones for which the node info is valid
    routing_domain: RoutingDomain,
    /// The calculated node ids for each crypto type in the node info
    node_ids: NodeIdGroup,
    /// The node info as parsed; may omit parts of the message this node does not understand
    node_info: NodeInfo,
    /// The node info message bytes, as serialized from a capnp message
    node_info_message: Bytes,
    /// The signatures for each public key in the node info, over the node info message.
    /// Only the signatures that are for the supported cryptosystems of this node are included, and those are validated.
    signatures: SignatureGroup,
}

impl fmt::Display for PeerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "routing_domain: {:?}", self.routing_domain)?;
        writeln!(f, "node_ids: {}", self.node_ids)?;
        writeln!(f, "node_info:")?;
        write!(f, "{}", indent_all_string(f.to_string(&self.node_info)))?;
        Ok(())
    }
}

impl fmt::Debug for PeerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeerInfo")
            .field("routing_domain", &self.routing_domain)
            .field("node_ids", &self.node_ids)
            .field("node_info", &self.node_info)
            .field("len(node_info_message)", &self.node_info_message.len())
            .field("signatures", &self.signatures)
            .finish()
    }
}

impl PeerInfo {
    // Create a peer info from node info and secret
    pub fn try_new_from_node_info(
        routing_table: &RoutingTable,
        routing_domain: RoutingDomain,
        secret_keys: &SecretKeyGroup,
        node_info: NodeInfo,
    ) -> VeilidAPIResult<Self> {
        // Get signing public keys from node info
        let public_keys = node_info.public_keys();

        // Ensure node ids are within limits
        if public_keys.is_empty() {
            apibail_internal!(
                "no public keys for peer info ({:?})\n{:#?}",
                routing_domain,
                node_info
            );
        } else if public_keys.len() > MAX_CRYPTO_KINDS {
            apibail_internal!(
                "too many public keys for peer info ({:?}): {:?}\n{:#?}",
                routing_domain,
                public_keys,
                node_info
            );
        }

        // Make sure secret keys and public keys match and make keypairs
        let mut keypairs = KeyPairGroup::new();
        for pk in public_keys.iter() {
            let Some(sk) = secret_keys.get(pk.kind()) else {
                apibail_internal!("secret key not found for public key: {}", pk);
            };
            keypairs.add(KeyPair::new_from_parts(pk.clone(), sk.value()));
        }

        // Generate on-the-wire node info message
        let mut node_info_message_builder = ::capnp::message::Builder::new_default();
        let mut node_info_builder =
            node_info_message_builder.init_root::<veilid_capnp::node_info::Builder>();
        encode_node_info(&node_info, &mut node_info_builder)?;
        let node_info_message =
            canonical_message_builder_to_bytes_writer_packed(node_info_message_builder, |size| {
                BytesWriter::with_capacity(size)
            })?
            .into_inner()
            .freeze();

        // Sign the message
        let crypto = routing_table.crypto();
        let signatures = SignatureGroup::from(crypto.generate_signatures(
            &node_info_message,
            &keypairs,
            |_kp, sig| sig,
        )?);

        // Extract node ids for convenience
        let mut node_ids = NodeIdGroup::new();
        for pk in public_keys.iter() {
            node_ids.add(routing_table.generate_node_id(pk)?);
        }

        Ok(Self {
            routing_domain,
            node_ids,
            node_info,
            node_info_message,
            signatures,
        })
    }

    // Create an unsigned peer info
    pub fn try_new_from_unsigned(
        routing_table: &RoutingTable,
        routing_domain: RoutingDomain,
        node_info: NodeInfo,
    ) -> VeilidAPIResult<Self> {
        // Get signing public keys from node info
        let public_keys = node_info.public_keys();

        // Ensure node ids are within limits
        if public_keys.is_empty() {
            apibail_internal!(
                "no public keys for peer info ({:?})\n{:#?}",
                routing_domain,
                node_info
            );
        } else if public_keys.len() > MAX_CRYPTO_KINDS {
            apibail_internal!(
                "too many public keys for peer info ({:?}): {:?}\n{:#?}",
                routing_domain,
                public_keys,
                node_info
            );
        }

        // Generate on-the-wire node info message
        let mut node_info_message_builder = ::capnp::message::Builder::new_default();
        let mut node_info_builder =
            node_info_message_builder.init_root::<veilid_capnp::node_info::Builder>();
        encode_node_info(&node_info, &mut node_info_builder)?;
        let node_info_message =
            canonical_message_builder_to_bytes_writer_packed(node_info_message_builder, |size| {
                BytesWriter::with_capacity(size)
            })?
            .into_inner()
            .freeze();

        // Extract node ids for convenience
        let mut node_ids = NodeIdGroup::new();
        for pk in public_keys.iter() {
            node_ids.add(routing_table.generate_node_id(pk)?);
        }

        Ok(Self {
            routing_domain,
            node_ids,
            node_info,
            node_info_message,
            signatures: SignatureGroup::new(),
        })
    }

    /// Rebuild a peer info from its stored signed message, without checking its signatures
    pub(crate) fn try_new_from_parts(
        routing_domain: RoutingDomain,
        node_ids: NodeIdGroup,
        node_info_message: Bytes,
        signatures: SignatureGroup,
    ) -> VeilidAPIResult<Self> {
        let node_info = NodeInfo::try_new_from_wire(&node_info_message)?;
        Ok(Self {
            routing_domain,
            node_ids,
            node_info,
            node_info_message,
            signatures,
        })
    }

    pub fn routing_domain(&self) -> RoutingDomain {
        self.routing_domain
    }
    pub fn node_ids(&self) -> &NodeIdGroup {
        &self.node_ids
    }
    pub fn node_info(&self) -> &NodeInfo {
        &self.node_info
    }
    pub fn signatures(&self) -> &SignatureGroup {
        &self.signatures
    }
    pub fn node_info_message(&self) -> &Bytes {
        &self.node_info_message
    }

    #[expect(dead_code)]
    pub fn destructure(self) -> (RoutingDomain, NodeIdGroup, NodeInfo, Bytes, SignatureGroup) {
        (
            self.routing_domain,
            self.node_ids,
            self.node_info,
            self.node_info_message,
            self.signatures,
        )
    }

    /// Compare this PeerInfo to another one
    /// Exclude the signature and timestamp and any other fields that are not
    /// semantically valuable
    /// If the two are not equivalent they should be considered different
    /// enough for republication, but this is not the only criteria required
    /// for publication.
    pub fn equivalent(&self, other: &PeerInfo) -> bool {
        self.routing_domain == other.routing_domain
            && self.node_ids == other.node_ids
            && self.node_info.equivalent(&other.node_info)
    }
}

/// A peer info off the wire whose signatures have not been checked.
///
/// Only node ids, timestamp and node info are readable, enough to decide whether the
/// routing table wants it; the verified `PeerInfo` comes out of
/// [`Self::verify_and_extract`], which the routing table calls once per registration
/// and skips when it already holds the same node info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnverifiedPeerInfo(PeerInfo);

impl fmt::Display for UnverifiedPeerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl UnverifiedPeerInfo {
    /// Parse a peer info from the wire without verifying its signatures.
    ///
    /// Returns `Ok(None)` when no signature matches a supported crypto kind or the node
    /// info is not acceptable from `origin_routing_domain`.
    pub fn try_new_from_wire(
        routing_table: &RoutingTable,
        origin_routing_domain: RoutingDomain,
        node_info_message: &[u8],
        signatures: SignatureGroup,
    ) -> VeilidAPIResult<Option<Self>> {
        let node_info = NodeInfo::try_new_from_wire(node_info_message)?;

        // Get signing public keys from node info
        let public_keys = node_info.public_keys();

        // Ensure node ids are within limits
        if public_keys.is_empty() {
            apibail_internal!(
                "no public keys for peer info ({:?})\n{:#?}",
                origin_routing_domain,
                node_info
            );
        } else if public_keys.len() > MAX_CRYPTO_KINDS {
            apibail_internal!(
                "too many public keys for peer info ({:?}): {:?}\n{:#?}",
                origin_routing_domain,
                public_keys,
                node_info
            );
        }

        // No signature for any supported key: nothing here can ever verify
        let crypto = routing_table.crypto();
        if !signatures.iter().any(|sig| {
            crypto.get(sig.kind()).is_some() && public_keys.iter().any(|pk| pk.kind() == sig.kind())
        }) {
            return Ok(None);
        }

        // Translate the routing domain
        let Some(routing_domain) =
            routing_table.find_best_node_info_routing_domain(origin_routing_domain, &node_info)
        else {
            return Ok(None);
        };

        // Extract node ids for convenience
        let mut node_ids = NodeIdGroup::new();
        for pk in public_keys.iter() {
            node_ids.add(routing_table.generate_node_id(pk)?);
        }

        // Make owned node info message
        let node_info_message = Bytes::copy_from_slice(node_info_message);

        Ok(Some(Self(PeerInfo {
            routing_domain,
            node_ids,
            node_info,
            node_info_message,
            signatures,
        })))
    }

    /// A verified peer info of our own, in the form the wire carries
    pub fn from_verified(peer_info: &PeerInfo) -> Self {
        Self(peer_info.clone())
    }

    /// Verified peer infos from our table, in the form an answer carries
    pub fn from_verified_list(list: Vec<Arc<PeerInfo>>) -> Vec<Arc<Self>> {
        list.iter()
            .map(|pi| Arc::new(Self::from_verified(pi)))
            .collect()
    }

    /// Check the signatures and hand back the verified peer info.
    ///
    /// Byte-identical peer infos verified before are served from the routing table's
    /// cache. An unsigned peer info passes through unchanged: whether that is acceptable
    /// is the caller's `allow_invalid` decision.
    pub fn verify_and_extract(&self, routing_table: &RoutingTable) -> VeilidAPIResult<PeerInfo> {
        let pi = &self.0;
        if pi.signatures.is_empty() {
            return Ok(pi.clone());
        }
        let cache_key = (pi.node_info_message.clone(), pi.signatures.clone());
        if routing_table.peer_info_verified(&cache_key) {
            return Ok(pi.clone());
        }
        let public_keys = pi.node_info.public_keys();
        let crypto = routing_table.crypto();
        let Some(valid_signatures) =
            crypto.verify_signatures(&public_keys, &pi.node_info_message, &pi.signatures)?
        else {
            apibail_generic!("invalid signature for node info");
        };
        if valid_signatures.is_empty() {
            apibail_generic!("no supported crypto kind for node info");
        }
        routing_table.note_peer_info_verified(cache_key);
        Ok(pi.clone())
    }

    pub fn routing_domain(&self) -> RoutingDomain {
        self.0.routing_domain
    }
    pub fn node_ids(&self) -> &NodeIdGroup {
        &self.0.node_ids
    }
    pub fn timestamp(&self) -> Timestamp {
        self.0.node_info.timestamp()
    }
    /// Unverified: for deciding whether to register, never for trusting
    pub(crate) fn node_info(&self) -> &NodeInfo {
        &self.0.node_info
    }
    pub fn node_info_message(&self) -> &[u8] {
        &self.0.node_info_message
    }
    pub fn signatures(&self) -> &SignatureGroup {
        &self.0.signatures
    }
}

/// A peer info stored as its signed message, decoded again when loaded
pub(crate) mod signed_peer_info {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    type SignedParts = (RoutingDomain, NodeIdGroup, Bytes, SignatureGroup);

    fn to_parts(peer_info: &PeerInfo) -> (RoutingDomain, &NodeIdGroup, &Bytes, &SignatureGroup) {
        (
            peer_info.routing_domain(),
            peer_info.node_ids(),
            peer_info.node_info_message(),
            peer_info.signatures(),
        )
    }

    fn from_parts<E: serde::de::Error>(
        (routing_domain, node_ids, node_info_message, signatures): SignedParts,
    ) -> Result<Arc<PeerInfo>, E> {
        PeerInfo::try_new_from_parts(routing_domain, node_ids, node_info_message, signatures)
            .map(Arc::new)
            .map_err(E::custom)
    }

    pub fn serialize<S: Serializer>(
        peer_info: &Arc<PeerInfo>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        to_parts(peer_info).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<PeerInfo>, D::Error> {
        from_parts(SignedParts::deserialize(deserializer)?)
    }

    /// The same for an optional peer info
    pub mod option {
        use super::*;

        pub fn serialize<S: Serializer>(
            peer_info: &Option<Arc<PeerInfo>>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            peer_info.as_deref().map(to_parts).serialize(serializer)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<Arc<PeerInfo>>, D::Error> {
            Option::<SignedParts>::deserialize(deserializer)?
                .map(from_parts)
                .transpose()
        }
    }
}
