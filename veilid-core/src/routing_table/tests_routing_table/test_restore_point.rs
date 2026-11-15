use super::*;
use crate::tests::*;

fn udp(socket_addr: &str) -> DialInfoDetail {
    DialInfoDetail {
        class: DialInfoClass::Direct,
        dial_info: DialInfo::udp_from_socketaddr(socket_addr.parse().expect("socket addr")),
    }
}

fn mock_config(config: &mut VeilidConfig, identity: &(PublicKeyGroup, SecretKeyGroup)) {
    config.network.routing_table.bootstrap = vec![];
    set_identity(&mut config.network.routing_table, identity);
}

/// PublicInternet as a detection would leave it
fn confirm_public_internet(routing_table: &RoutingTable, dial_info_detail: DialInfoDetail) {
    let rdc = routing_table.get_routing_domain_controller(RoutingDomain::PublicInternet);
    let mut editor = rdc.edit_dyn();
    editor.set_network_config(RoutingDomainNetworkConfig::new(
        ProtocolTypeSet::all(),
        ProtocolTypeSet::all(),
        AddressTypeSet::all(),
        PUBLIC_INTERNET_CAPABILITIES.iter().copied().collect(),
    ));
    editor.clear_dial_info_details(None, None);
    editor.add_dial_info_detail(dial_info_detail);
    editor.set_confirmed(true);
    editor.commit();
}

fn mock_platform() -> PlatformRestoreState {
    PlatformRestoreState {
        validity: Bytes::from_static(b"validity"),
        state: Bytes::from_static(b"state"),
    }
}

/// The restore point for what the routing domain holds now
fn restore_point_now(
    routing_table: &RoutingTable,
    routing_domain: RoutingDomain,
) -> RoutingDomainRestorePoint {
    let rdc = routing_table.get_routing_domain_controller(routing_domain);
    let rdd = rdc.read_dyn();
    RoutingDomainRestorePoint::new(mock_platform(), &*rdd, rdd.get_peer_info())
}

fn restore_point_bytes(restore_point: &RoutingDomainRestorePoint) -> Bytes {
    serialize_table_value(restore_point).0
}

/// Each routing domain's restore point survives a restart as it was saved
pub async fn test_restore_point_round_trip() {
    let identity = fix_identity();
    let (registry, storage) = mock_registry::init_with_storage("rt_restore_point", |config| {
        mock_config(config, &identity)
    })
    .await;
    let saved = {
        let routing_table = registry.routing_table();
        confirm_public_internet(&routing_table, udp("8.8.8.8:5150"));
        let mut saved = Vec::new();
        for routing_domain in RoutingDomain::all() {
            let restore_point = restore_point_now(&routing_table, routing_domain);
            saved.push(restore_point_bytes(&restore_point));
            routing_table
                .get_routing_domain_controller(routing_domain)
                .read_dyn()
                .replace_restore_point(Some(Arc::new(restore_point)));
        }
        let counts = routing_table.save().await.expect("should save");
        assert_eq!(
            counts.routing_domain_states, 3,
            "two restore points and the PublicInternet state"
        );
        let counts = routing_table.save().await.expect("should save");
        assert_eq!(
            counts.routing_domain_states, 0,
            "nothing changed since the last save"
        );
        saved
    };
    mock_registry::terminate(registry).await;

    let registry = mock_registry::reopen("rt_restore_point", storage, |config| {
        mock_config(config, &identity)
    })
    .await;
    let loaded: Vec<Bytes> = RoutingDomain::all()
        .iter()
        .map(|routing_domain| {
            let restore_point = registry
                .routing_table()
                .get_routing_domain_controller(*routing_domain)
                .read_dyn()
                .get_restore_point()
                .expect("restore point should load");
            restore_point_bytes(&restore_point)
        })
        .collect();
    let counts = registry.routing_table().save().await.expect("should save");
    assert_eq!(
        counts.routing_domain_states, 0,
        "a restart should not rewrite routing domain states it loaded unchanged"
    );
    mock_registry::terminate(registry).await;

    assert_eq!(
        loaded, saved,
        "a restart should load each restore point as saved"
    );
}

/// A restore puts back the dial info and confirmation, and the signed peer info peers hold
pub async fn test_restore_keeps_the_signed_peer_info() {
    let registry = mock_registry::init("rt_restore_keeps_peer_info").await;
    let routing_table = registry.routing_table();
    confirm_public_internet(&routing_table, udp("8.8.8.8:5150"));
    let restore_point = restore_point_now(&routing_table, RoutingDomain::PublicInternet);

    // An attach starts from nothing
    let rdc = routing_table.get_routing_domain_controller(RoutingDomain::PublicInternet);
    rdc.edit_dyn().reset();
    assert!(!rdc.read_dyn().confirmed());

    assert!(
        rdc.restore(&restore_point),
        "the saved peer info should be kept"
    );
    {
        let rdd = rdc.read_dyn();
        assert!(rdd.confirmed());
        assert_eq!(rdd.dial_info_details(), &vec![udp("8.8.8.8:5150")]);
        assert!(
            Arc::ptr_eq(&rdd.get_peer_info(), &restore_point.peer_info),
            "the current peer info should be the saved one, not signed again"
        );
    }

    mock_registry::terminate(registry).await;
}

/// A saved peer info that no longer says what we would publish is signed again, newer than it
pub async fn test_restore_signs_after_the_saved_peer_info() {
    let registry = mock_registry::init("rt_restore_signs_after").await;
    let routing_table = registry.routing_table();
    confirm_public_internet(&routing_table, udp("8.8.8.8:5150"));
    let mut restore_point = restore_point_now(&routing_table, RoutingDomain::PublicInternet);

    // Peers hold a peer info stamped later than our clock reads, and it is not one we would make
    let saved_ts = Timestamp::now().later(TimestampDuration::new_secs(3600));
    restore_point.peer_info = Arc::new(
        fix_peer_info_at(
            &routing_table,
            fix_crypto_info_list(true),
            fix_crypto_info_list_secrets(),
            saved_ts,
        )
        .expect("should be valid"),
    );

    let rdc = routing_table.get_routing_domain_controller(RoutingDomain::PublicInternet);
    rdc.edit_dyn().reset();
    assert!(
        !rdc.restore(&restore_point),
        "the saved peer info should be replaced"
    );
    let current = rdc.read_dyn().get_peer_info();
    assert!(!Arc::ptr_eq(&current, &restore_point.peer_info));
    assert!(
        current.node_info().timestamp() > saved_ts,
        "a peer info signed after a restore must be newer than the one peers hold"
    );

    mock_registry::terminate(registry).await;
}

/// Without a platform network to say what a publication rests on, no restore point is kept,
/// and a discard drops one
pub async fn test_restore_point_needs_the_network() {
    let registry = mock_registry::init("rt_restore_point_needs_network").await;
    let routing_table = registry.routing_table();
    confirm_public_internet(&routing_table, udp("8.8.8.8:5150"));
    let restore_point = restore_point_now(&routing_table, RoutingDomain::PublicInternet);
    let rdc = routing_table.get_routing_domain_controller(RoutingDomain::PublicInternet);
    rdc.read_dyn()
        .replace_restore_point(Some(Arc::new(restore_point.clone())));

    // An older restore point would hold a peer info older than the one just published
    rdc.capture_restore_point(rdc.read_dyn().get_peer_info());
    assert!(rdc.read_dyn().get_restore_point().is_none());

    rdc.read_dyn()
        .replace_restore_point(Some(Arc::new(restore_point)));
    rdc.discard_restore_point();
    assert!(rdc.read_dyn().get_restore_point().is_none());

    mock_registry::terminate(registry).await;
}

/// A peer's last report of our address survives a restart, so its next report can say ours moved
pub async fn test_last_sender_info_survives_restart() {
    let identity = fix_identity();
    let key = LastSenderInfoKey {
        routing_domain: RoutingDomain::PublicInternet,
        transport: TransportType::new(ProtocolType::UDP, AddressType::IPV4),
    };
    let before = SenderInfo {
        socket_address: SocketAddress::new(Address::IPV4(Ipv4Addr::new(203, 0, 113, 7)), 5150),
    };
    let after = SenderInfo {
        socket_address: SocketAddress::new(Address::IPV4(Ipv4Addr::new(198, 51, 100, 9)), 5150),
    };
    let (registry, storage) =
        mock_registry::init_with_storage("rt_sender_info_restart", |config| {
            mock_config(config, &identity)
        })
        .await;
    let node_id = {
        let routing_table = registry.routing_table();
        let node = seed_nodes(&routing_table, 1).remove(0);
        assert_eq!(
            node.operate_mut(|e| e.report_sender_info(key.clone(), before)),
            None
        );
        routing_table.save().await.expect("should save");
        node.best_node_id()
    };
    mock_registry::terminate(registry).await;

    let registry = mock_registry::reopen("rt_sender_info_restart", storage, |config| {
        mock_config(config, &identity)
    })
    .await;
    {
        let node = registry
            .routing_table()
            .lookup_node_id(node_id)
            .expect("should look up")
            .expect("entry should load");
        assert_eq!(
            node.operate_mut(|e| e.report_sender_info(key.clone(), after)),
            Some(before),
            "the report before the restart is what this one moved away from"
        );
    }
    mock_registry::terminate(registry).await;
}
