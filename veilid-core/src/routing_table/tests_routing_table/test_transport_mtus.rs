use super::*;
use crate::tests::*;

/// PublicInternet as a detection would leave a public server, short of our mtus: direct dial
/// info on every transport, so it needs no relay
fn confirm_public_internet(routing_table: &RoutingTable) {
    let v4 = Ipv4Addr::new(8, 8, 8, 8);
    let v6 = Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888);
    let controller = routing_table
        .get_specific_routing_domain_controller::<PublicInternetRoutingDomainController>();
    let mut editor = controller.edit();
    editor.set_network_config(RoutingDomainNetworkConfig::new(
        ProtocolTypeSet::all(),
        ProtocolTypeSet::all(),
        AddressTypeSet::all(),
        PUBLIC_INTERNET_CAPABILITIES.iter().copied().collect(),
    ));
    editor.set_interface_addresses(vec![
        IfAddr::V4(Ifv4Addr {
            ip: v4,
            netmask: Ipv4Addr::new(255, 255, 255, 0),
            broadcast: None,
        }),
        IfAddr::V6(Ifv6Addr {
            ip: v6,
            netmask: Ipv6Addr::new(0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0, 0),
            broadcast: None,
        }),
    ]);
    editor.clear_dial_info_details(None, None);
    for ip in [IpAddr::V4(v4), IpAddr::V6(v6)] {
        let socket_addr = SocketAddr::new(ip, 5150);
        let socket_address = SocketAddress::from_socket_addr(socket_addr);
        let mut dial_infos = vec![
            DialInfo::udp_from_socketaddr(socket_addr),
            DialInfo::tcp_from_socketaddr(socket_addr),
            DialInfo::try_ws(socket_address, format!("ws://{socket_addr}/ws")).expect("ws"),
        ];
        #[cfg(feature = "enable-protocol-wss")]
        dial_infos.push(
            DialInfo::try_wss(socket_address, "wss://example.com:5150/ws".to_string())
                .expect("wss"),
        );
        for dial_info in dial_infos.drain(..) {
            editor.add_dial_info_detail(DialInfoDetail {
                class: DialInfoClass::Direct,
                dial_info,
            });
        }
    }
    editor.set_confirmed(true);
    editor.commit();
}

fn udp_v4_mtus(mtu: u16) -> TransportMtus {
    let mut mtus = TransportMtus::default();
    mtus.set(
        LowLevelTransportType {
            low_level_protocol_type: LowLevelProtocolType::UDP,
            address_type: AddressType::IPV4,
        },
        mtu,
    );
    mtus
}

/// PublicInternet holds its peer info back until our mtus are known, then publishes them, and a
/// change publishes again
pub async fn test_publish_waits_for_transport_mtus() {
    let registry = mock_registry::init("rt_publish_waits_for_mtus").await;
    let routing_table = registry.routing_table();
    confirm_public_internet(&routing_table);

    let controller = routing_table
        .get_specific_routing_domain_controller::<PublicInternetRoutingDomainController>();
    assert_eq!(
        controller.state().inbound_stage,
        RoutingDomainInboundStage::NeedsTransportMtus
    );
    assert!(
        !controller.publish_peer_info(),
        "nothing should publish before our mtus are known"
    );

    let mut editor = controller.edit();
    editor.set_receive_mtus(Some(udp_v4_mtus(1472)));
    editor.commit();
    assert_eq!(
        controller.state().inbound_stage,
        RoutingDomainInboundStage::ReadyToPublish
    );
    assert!(controller.publish_peer_info());
    assert_eq!(
        controller
            .get_published_peer_info()
            .expect("published")
            .node_info()
            .receive_mtus(),
        &udp_v4_mtus(1472)
    );

    let mut editor = controller.edit();
    editor.set_receive_mtus(Some(udp_v4_mtus(SAFE_TRANSPORT_MTU)));
    editor.commit();
    assert!(
        controller.publish_peer_info(),
        "a size change should publish again"
    );
    assert_eq!(
        controller
            .get_published_peer_info()
            .expect("published")
            .node_info()
            .receive_mtus(),
        &udp_v4_mtus(SAFE_TRANSPORT_MTU)
    );

    mock_registry::terminate(registry).await;
}
