use super::*;
use crate::tests::*;

fn udp(socket_addr: &str) -> DialInfo {
    DialInfo::udp_from_socketaddr(socket_addr.parse().expect("socket addr"))
}

/// A dial info is ours by socket address, whatever its protocol, or by port when it points at loopback
pub async fn test_matches_own_dial_info() {
    info!("--- test_matches_own_dial_info ---");

    let registry = mock_registry::init("own_dial_info").await;
    let routing_table = registry.routing_table();

    assert!(!routing_table.matches_own_dial_info(&udp("8.8.8.8:5150")));

    {
        let rdc = routing_table.get_routing_domain_controller(RoutingDomain::PublicInternet);
        let mut editor = rdc.edit_dyn();
        editor.add_dial_info_detail(DialInfoDetail {
            class: DialInfoClass::Direct,
            dial_info: udp("8.8.8.8:5150"),
        });
        editor.commit();
    }

    assert!(routing_table.matches_own_dial_info(&udp("8.8.8.8:5150")));
    assert!(
        routing_table.matches_own_dial_info(&DialInfo::tcp_from_socketaddr(
            "8.8.8.8:5150".parse().expect("socket addr")
        ))
    );
    assert!(routing_table.matches_own_dial_info(&udp("127.0.0.1:5150")));
    assert!(!routing_table.matches_own_dial_info(&udp("127.0.0.1:5151")));
    assert!(!routing_table.matches_own_dial_info(&udp("8.8.4.4:5150")));
    assert!(!routing_table.matches_own_dial_info(&udp("8.8.8.8:5151")));

    mock_registry::terminate(registry).await;
}
