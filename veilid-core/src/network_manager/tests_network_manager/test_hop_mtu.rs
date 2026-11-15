use super::*;
use crate::routing_table::tests_routing_table::*;
use crate::tests::*;

fn tcp_v4() -> LowLevelTransportType {
    LowLevelTransportType {
        low_level_protocol_type: LowLevelProtocolType::TCP,
        address_type: AddressType::IPV4,
    }
}

fn tcp_v4_mtus(mtu: u16) -> TransportMtus {
    let mut mtus = TransportMtus::default();
    mtus.set(tcp_v4(), mtu);
    mtus
}

fn dial_info_detail(ip: Ipv4Addr) -> DialInfoDetail {
    DialInfoDetail {
        class: DialInfoClass::Direct,
        dial_info: DialInfo::tcp_from_socketaddr(SocketAddr::new(ip.into(), 5150)),
    }
}

/// A signed PublicInternet peer at `ip`, registered when `register` is set
fn make_peer(
    routing_table: &RoutingTable,
    ip: Ipv4Addr,
    receive_mtus: TransportMtus,
    relay_info_list: Vec<RelayInfo>,
    register: bool,
) -> (Option<NodeRef>, NodeIdGroup) {
    let (crypto_info_list, secret_keys) = fix_node_keys();
    let node_info = NodeInfo::new(
        Timestamp::now(),
        vec![ENVELOPE_VERSION_ENV0],
        crypto_info_list,
        PUBLIC_INTERNET_CAPABILITIES.to_vec(),
        ProtocolTypeSet::all(),
        AddressTypeSet::all(),
        vec![dial_info_detail(ip)],
        relay_info_list,
        receive_mtus,
    );
    let peer_info = PeerInfo::try_new_from_node_info(
        routing_table,
        RoutingDomain::PublicInternet,
        &secret_keys,
        node_info,
    )
    .expect("peer info");
    let node_ids = peer_info.node_ids().clone();
    let opt_node_ref = register.then(|| {
        routing_table
            .register_node_with_peer_info(
                Arc::new(UnverifiedPeerInfo::from_verified(&peer_info)),
                false,
            )
            .expect("register")
            .unfiltered()
    });
    (opt_node_ref, node_ids)
}

fn relay_info(node_ids: NodeIdGroup, ip: Ipv4Addr, receive_mtus: TransportMtus) -> RelayInfo {
    RelayInfo::new(
        Timestamp::now(),
        node_ids,
        ProtocolTypeSet::all(),
        AddressTypeSet::all(),
        vec![dial_info_detail(ip)],
        RelayKind::Inbound,
        receive_mtus,
    )
}

/// A send is sized by the node at the far end of its hop: the target at its own address, a relay
/// by its own signed sizes over the copy its client carries, an unknown relay by that copy, and
/// nobody we know at the safe size
pub async fn test_hop_mtu_sizes_by_the_next_hop() {
    let registry = mock_registry::init("nm_hop_mtu").await;
    let network_manager = registry.network_manager();
    let routing_table = registry.routing_table();
    // Node refs must go before the registry: a dropped ref queues a bucket kick
    {
        network_manager.own_mtus().set(tcp_v4(), 1460, 1460);
        let transport = TransportType::new(ProtocolType::TCP, AddressType::IPV4);

        // A relay we hold, advertising full size in its own node info
        let relay_ip = Ipv4Addr::new(8, 8, 4, 4);
        let (_, relay_node_ids) =
            make_peer(&routing_table, relay_ip, tcp_v4_mtus(1460), vec![], true);
        // Its client claims the relay takes the minimum, and itself 1200
        let target_ip = Ipv4Addr::new(1, 1, 1, 1);
        let forged_copy = relay_info(relay_node_ids, relay_ip, tcp_v4_mtus(MIN_TRANSPORT_MTU));
        let (target, _) = make_peer(
            &routing_table,
            target_ip,
            tcp_v4_mtus(1200),
            vec![forged_copy],
            true,
        );
        let target = target.expect("registered");
        assert_eq!(
            network_manager.hop_mtu(&target, transport, target_ip.into()),
            1200,
            "the target's own size at its own address"
        );
        assert_eq!(
            network_manager.hop_mtu(&target, transport, relay_ip.into()),
            1460,
            "the relay's own signed size, not its client's copy"
        );

        // A relay we do not hold is sized by the copy
        let unknown_ip = Ipv4Addr::new(9, 9, 9, 9);
        let (_, unknown_node_ids) =
            make_peer(&routing_table, unknown_ip, tcp_v4_mtus(1460), vec![], false);
        let copy = relay_info(unknown_node_ids, unknown_ip, tcp_v4_mtus(1300));
        let (relayed, _) = make_peer(
            &routing_table,
            Ipv4Addr::new(1, 0, 0, 1),
            tcp_v4_mtus(1460),
            vec![copy],
            true,
        );
        assert_eq!(
            network_manager.hop_mtu(&relayed.expect("registered"), transport, unknown_ip.into()),
            1300
        );

        // An old node advertises nothing, and an address nobody we know owns is sized the same
        let old_ip = Ipv4Addr::new(8, 8, 8, 8);
        let (old, _) = make_peer(
            &routing_table,
            old_ip,
            TransportMtus::default(),
            vec![],
            true,
        );
        assert_eq!(
            network_manager.hop_mtu(&old.expect("registered"), transport, old_ip.into()),
            SAFE_TRANSPORT_MTU
        );
        assert_eq!(
            network_manager.hop_mtu(&target, transport, Ipv4Addr::new(5, 5, 5, 5).into()),
            SAFE_TRANSPORT_MTU
        );
    }

    mock_registry::terminate(registry).await;
}
