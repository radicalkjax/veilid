use crate::network_manager::*;
use crate::routing_table::*;
use crate::*;

pub fn fix_typed_node_id(kind: CryptoKind, idx: u8) -> NodeId {
    NodeId::new(
        kind,
        BareNodeId::new(&[
            idx, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0,
        ]),
    )
}

#[expect(dead_code)]
pub fn fix_typed_node_id_group(valid_kinds: bool, unknown: bool) -> NodeIdGroup {
    let mut tks = NodeIdGroup::new();
    if valid_kinds {
        VALID_CRYPTO_KINDS.iter().for_each(|k| {
            tks.add(fix_typed_node_id(*k, 0));
        });
    }
    if unknown {
        tks.add(fix_typed_node_id(CryptoKind::new([1, 2, 3, 4]), 0));
    }
    tks
}

pub fn fix_crypto_info_list(valid: bool) -> Vec<CryptoInfo> {
    let mut cil: Vec<CryptoInfo> = vec![];
    VALID_CRYPTO_KINDS.into_iter().for_each(|ck| {
        let ci = match ck {
            #[cfg(feature = "enable-crypto-none")]
            CRYPTO_KIND_NONE => CryptoInfo::NONE {
                public_key: BarePublicKey::from_str(if !valid {
                    "xuYisL8R7-qoUQiJtHVpvemzd1x3mH246cMJSkMp6BQ"
                } else {
                    "xMzvYmY1C0B-pUrB9V1pnUf6A1hSqNTOju39UaFxQoU"
                })
                .expect("should parse bare public key"),
            },
            #[cfg(feature = "enable-crypto-vld0")]
            CRYPTO_KIND_VLD0 => CryptoInfo::VLD0 {
                public_key: BarePublicKey::from_str(if !valid {
                    "v6XPfyOoCP_ZP-CWFNrf_pF_dpxsq74p2LW_Q5Q4yPQ"
                } else {
                    "W7ENB-SUWpPA7usY8ORVQf_si5QmFbD1Uqa89Jg2Uc0"
                })
                .expect("should parse bare public key"),
            },
            // #[cfg(feature = "enable-crypto-vld1")]
            // CRYPTO_KIND_VLD1 => CryptoInfo::VLD1 {
            //     encapsulation_key: todo!(),
            //     signing_key: todo!(),
            // },
            _ => {
                unreachable!("missing match arm");
            }
        };
        cil.push(ci);
    });
    cil
}

pub fn fix_crypto_info_list_secrets() -> SecretKeyGroup {
    let mut skg = SecretKeyGroup::new();
    VALID_CRYPTO_KINDS.into_iter().for_each(|ck| {
        let sk = match ck {
            #[cfg(feature = "enable-crypto-none")]
            CRYPTO_KIND_NONE => {
                SecretKey::from_str("NONE:OzMQnZnK9L-BWrU-CqKWYrgF_KetVysxcRICrl6OvXo")
                    .expect("should parse secret key")
            }
            #[cfg(feature = "enable-crypto-vld0")]
            CRYPTO_KIND_VLD0 => {
                SecretKey::from_str("VLD0:hbdjau5sr3rBNwN68XeWLg3rfXnXLaLqfbbqhELqV1E")
                    .expect("should parse secret key")
            }
            // #[cfg(feature = "enable-crypto-vld1")]
            // CRYPTO_KIND_VLD1 => CryptoInfo::VLD1 {
            //     encapsulation_key: todo!(),
            //     signing_key: todo!(),
            // },
            _ => {
                unreachable!("missing match arm");
            }
        };
        skg.add(sk);
    });
    skg
}

pub fn fix_socket_address() -> SocketAddress {
    SocketAddress::new(Address::IPV4(Ipv4Addr::new(18, 218, 0, 160)), 53)
}

pub fn fix_dial_info_detail() -> DialInfoDetail {
    DialInfoDetail {
        class: DialInfoClass::Direct,
        dial_info: DialInfo::TCP(DialInfoTCP {
            socket_address: fix_socket_address(),
        }),
    }
}

pub fn fix_peer_info(
    routing_table: &RoutingTable,
    crypto_info_list: Vec<CryptoInfo>,
    secret_keys: SecretKeyGroup,
) -> VeilidAPIResult<PeerInfo> {
    fix_peer_info_at(
        routing_table,
        crypto_info_list,
        secret_keys,
        Timestamp::new(0),
    )
}

/// A signed peer info whose node info carries `timestamp`
pub fn fix_peer_info_at(
    routing_table: &RoutingTable,
    crypto_info_list: Vec<CryptoInfo>,
    secret_keys: SecretKeyGroup,
    timestamp: Timestamp,
) -> VeilidAPIResult<PeerInfo> {
    let node_info = NodeInfo::new(
        timestamp,
        vec![ENVELOPE_VERSION_ENV0],
        crypto_info_list,
        PUBLIC_INTERNET_CAPABILITIES.to_vec(),
        ProtocolTypeSet::new(),
        AddressTypeSet::new(),
        vec![fix_dial_info_detail()],
        vec![],
        TransportMtus::default(),
    );

    PeerInfo::try_new_from_node_info(
        routing_table,
        RoutingDomain::PublicInternet,
        &secret_keys,
        node_info,
    )
}

pub fn fix_unsigned_peer_info(
    routing_table: &RoutingTable,
    crypto_info_list: Vec<CryptoInfo>,
) -> VeilidAPIResult<PeerInfo> {
    let node_info = NodeInfo::new(
        Timestamp::new(0),
        vec![ENVELOPE_VERSION_ENV0],
        crypto_info_list,
        PUBLIC_INTERNET_CAPABILITIES.to_vec(),
        ProtocolTypeSet::new(),
        AddressTypeSet::new(),
        vec![fix_dial_info_detail()],
        vec![],
        TransportMtus::default(),
    );

    PeerInfo::try_new_from_unsigned(routing_table, RoutingDomain::PublicInternet, node_info)
}

/// Crypto info and secret keys of a node no other call returns
pub fn fix_node_keys() -> (Vec<CryptoInfo>, SecretKeyGroup) {
    let mut crypto_info_list: Vec<CryptoInfo> = vec![];
    let mut secret_keys = SecretKeyGroup::new();
    for ck in VALID_CRYPTO_KINDS {
        let (public_key, secret) = Crypto::generate_keypair(ck)
            .expect("should generate keypair")
            .value()
            .into_split();
        let ci = match ck {
            #[cfg(feature = "enable-crypto-none")]
            CRYPTO_KIND_NONE => CryptoInfo::NONE { public_key },
            #[cfg(feature = "enable-crypto-vld0")]
            CRYPTO_KIND_VLD0 => CryptoInfo::VLD0 { public_key },
            _ => {
                unreachable!("missing match arm");
            }
        };
        crypto_info_list.push(ci);
        secret_keys.add(SecretKey::new(ck, secret));
    }
    (crypto_info_list, secret_keys)
}

/// An identity for a mock core to configure, so a reopened core keeps it
pub fn fix_identity() -> (PublicKeyGroup, SecretKeyGroup) {
    let mut public_keys = PublicKeyGroup::new();
    let mut secret_keys = SecretKeyGroup::new();
    for ck in VALID_CRYPTO_KINDS {
        let (public_key, secret_key) = Crypto::generate_keypair(ck)
            .expect("should generate keypair")
            .value()
            .into_split();
        public_keys.add(PublicKey::new(ck, public_key));
        secret_keys.add(SecretKey::new(ck, secret_key));
    }
    (public_keys, secret_keys)
}

/// Configure `identity` as the core's own
pub fn set_identity(
    rt: &mut VeilidConfigRoutingTable,
    identity: &(PublicKeyGroup, SecretKeyGroup),
) {
    rt.public_keys = identity.0.clone();
    rt.secret_keys = identity.1.clone();
}

fn make_distinct_peer_info(routing_table: &RoutingTable) -> PeerInfo {
    let (crypto_info_list, secret_keys) = fix_node_keys();
    fix_peer_info(routing_table, crypto_info_list, secret_keys).expect("should be valid")
}

pub fn seed_nodes(routing_table: &RoutingTable, count: usize) -> Vec<NodeRef> {
    (0..count)
        .map(|_| {
            let pi = make_distinct_peer_info(routing_table);
            routing_table
                .register_node_with_peer_info(
                    Arc::new(UnverifiedPeerInfo::from_verified(&pi)),
                    false,
                )
                .expect("should register")
                .unfiltered()
        })
        .collect()
}

pub fn rank_map(node_refs: &[NodeRef], target: &HashCoordinate) -> HashMap<NodeId, usize> {
    let mut ids: Vec<NodeId> = node_refs
        .iter()
        .map(|nr| nr.node_ids().get(CRYPTO_KIND_VLD0).unwrap_or_log())
        .collect();
    ids.sort_unstable_by(RoutingTable::make_closest_node_id_sort(target.clone()));
    ids.into_iter().enumerate().map(|(n, id)| (id, n)).collect()
}
