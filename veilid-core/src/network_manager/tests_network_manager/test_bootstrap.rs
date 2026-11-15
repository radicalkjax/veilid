use super::*;
use crate::crypto::tests_crypto::*;
use crate::tests::*;

fn make_mock_bootstrap_record(include_timestamp: bool) -> BootstrapRecord {
    let public_keys = PublicKeyGroup::from(mock_public_keys());
    let envelope_support = VALID_ENVELOPE_VERSIONS.to_vec();
    let dial_info_details = vec![
        DialInfoDetail {
            class: DialInfoClass::Direct,
            dial_info: DialInfo::try_ws(
                SocketAddress::new(Address::IPV4(Ipv4Addr::UNSPECIFIED), 5150),
                "ws://example.com:5150/ws".to_owned(),
            )
            .expect("should make ws dialinfo"),
        },
        #[cfg(feature = "enable-protocol-wss")]
        DialInfoDetail {
            class: DialInfoClass::Direct,
            dial_info: DialInfo::try_wss(
                SocketAddress::new(Address::IPV4(Ipv4Addr::UNSPECIFIED), 5150),
                "wss://example.com:5150/wss".to_owned(),
            )
            .expect("should make wss dialinfo"),
        },
    ];
    let opt_timestamp = if include_timestamp {
        Some(Timestamp::now().as_u64() / 1_000_000u64)
    } else {
        None
    };
    BootstrapRecord::new(
        public_keys,
        envelope_support,
        dial_info_details,
        opt_timestamp,
        vec![],
    )
}

fn mock_signing_keys() -> Vec<PublicKey> {
    mock_keypairs()
        .iter()
        .map(|skp| PublicKey::new(skp.kind(), skp.ref_value().key()))
        .collect()
}

async fn mock_v1_string(
    network_manager: &NetworkManager,
    dial_info_converter: &dyn DialInfoConverter,
) -> String {
    make_mock_bootstrap_record(true)
        .to_v1_string(
            network_manager,
            dial_info_converter,
            mock_keypairs()[0].clone(),
        )
        .await
        .expect("should make string")
}

pub async fn test_bootstrap_v0() {
    let registry = mock_registry::init("").await;
    let network_manager = registry.network_manager();
    let dial_info_converter = MockDialInfoConverter::default();

    let bsrec = make_mock_bootstrap_record(false);
    let v0str = bsrec
        .to_v0_string(&dial_info_converter)
        .await
        .expect("should make string");
    let bsrec2 =
        BootstrapRecord::try_new_from_v0_str(&network_manager, Some(&dial_info_converter), &v0str)
            .await
            .expect("should parse string")
            .expect("should be valid record");
    assert_eq!(bsrec, bsrec2);

    mock_registry::terminate(registry).await;
}

pub async fn test_bootstrap_v1() {
    let registry = mock_registry::init("").await;
    let network_manager = registry.network_manager();
    let dial_info_converter = MockDialInfoConverter::default();

    let bsrec = make_mock_bootstrap_record(true);
    let signing_key_pairs = mock_keypairs();
    println!("signing_key_pairs: {:?}", signing_key_pairs);
    let signing_keys = signing_key_pairs
        .iter()
        .map(|skp| PublicKey::new(skp.kind(), skp.ref_value().key()))
        .collect::<Vec<_>>();
    let v1str = bsrec
        .to_v1_string(
            &network_manager,
            &dial_info_converter,
            signing_key_pairs[0].clone(),
        )
        .await
        .expect("should make string");
    let bsrec2 = BootstrapRecord::try_new_from_v1_str(
        &network_manager,
        Some(&dial_info_converter),
        &v1str,
        &signing_keys,
    )
    .await
    .expect("should parse string")
    .expect("should be valid record");
    assert_eq!(bsrec, bsrec2);

    // Persisted with the routing domain state
    let bsrec3: BootstrapRecord =
        deserialize_json(&serialize_json(&bsrec2)).expect("should round trip");
    assert_eq!(bsrec2, bsrec3);

    mock_registry::terminate(registry).await;
}

/// B01T answers only from the routing domain's stored records and never resolves anything
pub async fn test_boot_v1_response() {
    let registry = mock_registry::init("").await;
    let network_manager = registry.network_manager();
    let routing_table = registry.routing_table();
    let dial_info_converter = MockDialInfoConverter::default();

    // Nothing stored, nothing served
    assert!(network_manager.boot_v1_response().is_none());

    let signing_key_pairs = mock_keypairs();
    let signing_keys = signing_key_pairs
        .iter()
        .map(|skp| PublicKey::new(skp.kind(), skp.ref_value().key()))
        .collect::<Vec<_>>();
    let v1str = make_mock_bootstrap_record(true)
        .to_v1_string(
            &network_manager,
            &dial_info_converter,
            signing_key_pairs[0].clone(),
        )
        .await
        .expect("should make string");
    let bsrec = BootstrapRecord::try_new_from_v1_str(
        &network_manager,
        Some(&dial_info_converter),
        &v1str,
        &signing_keys,
    )
    .await
    .expect("should parse string")
    .expect("should be valid record");

    // A v0 string and a version this build cannot parse are cached and served just the same,
    // so a node passes on what it was told rather than only what it understood
    let v0str = make_mock_bootstrap_record(false)
        .to_v0_string(&dial_info_converter)
        .await
        .expect("should make string");
    let future_str = "2|0|VLD0:something|example.com|T5150|1746492253|sig".to_owned();

    routing_table
        .get_routing_domain_controller(RoutingDomain::PublicInternet)
        .read_dyn()
        .record_bootstrap_cache(BootstrapCache::new(
            vec![
                v1str.clone(),
                v1str.clone(),
                v0str.clone(),
                future_str.clone(),
            ],
            vec![bsrec.clone(), bsrec],
        ));

    let response = network_manager
        .boot_v1_response()
        .expect("should serve stored records");
    let mut expected = vec![v1str, v0str, future_str];
    expected.sort_unstable();
    assert_eq!(response.records, expected);
    // The record's node is not in the routing table and is not us
    assert!(response.peers.is_empty());

    mock_registry::terminate(registry).await;
}

/// A JSON bootstrap peer's node info and ids come from its signed bytes, not from the JSON
pub async fn test_bootstrap_peer_json_binding() {
    let registry = mock_registry::init("jsonbind").await;
    let network_manager = registry.network_manager();
    let routing_table = registry.routing_table();

    let keypair = mock_keypairs()[0].clone();
    let node_info = NodeInfo::new(
        Timestamp::now(),
        VALID_ENVELOPE_VERSIONS.to_vec(),
        vec![CryptoInfo::VLD0 {
            public_key: keypair.ref_value().key(),
        }],
        PUBLIC_INTERNET_CAPABILITIES.to_vec(),
        ProtocolTypeSet::all(),
        AddressTypeSet::all(),
        vec![DialInfoDetail {
            class: DialInfoClass::Direct,
            dial_info: DialInfo::udp(SocketAddress::new(
                Address::IPV4(Ipv4Addr::new(1, 2, 3, 4)),
                5150,
            )),
        }],
        vec![],
        TransportMtus::default(),
    );
    let secret_keys =
        SecretKeyGroup::from(SecretKey::new(keypair.kind(), keypair.ref_value().secret()));
    let peer_info = PeerInfo::try_new_from_node_info(
        &routing_table,
        RoutingDomain::PublicInternet,
        &secret_keys,
        node_info,
    )
    .expect("should sign peer info");
    let json = serialize_json(&peer_info);

    // Fields beside the signed bytes are ignored
    let mut tampered: serde_json::Value = serde_json::from_str(&json).expect("should parse");
    tampered["node_info"]["timestamp"] =
        serde_json::to_value(Timestamp::new(u64::MAX / 2)).expect("should serialize");
    tampered["node_info"]["dial_info_detail_list"] = serde_json::json!([]);
    let tampered: PeerInfo =
        deserialize_json(&tampered.to_string()).expect("should parse tampered peer info");
    assert_ne!(tampered.node_info(), peer_info.node_info());
    let bound = network_manager
        .bind_bootstrap_peer(&tampered)
        .expect("signed bytes are intact");
    assert_eq!(bound.node_info(), peer_info.node_info());
    assert_eq!(bound.node_ids(), peer_info.node_ids());

    // A changed signed byte is refused
    let mut tampered: serde_json::Value = serde_json::from_str(&json).expect("should parse");
    let byte = tampered["node_info_message"][0]
        .as_u64()
        .expect("bytes serialize as numbers");
    tampered["node_info_message"][0] = serde_json::Value::from(byte ^ 1);
    let tampered: PeerInfo =
        deserialize_json(&tampered.to_string()).expect("should parse tampered peer info");
    assert!(network_manager.bind_bootstrap_peer(&tampered).is_none());

    mock_registry::terminate(registry).await;
}

/// A short dial info or hostname with a multibyte character fails to parse instead of panicking
pub async fn test_short_dial_info_multibyte() {
    let dial_info_converter = MockDialInfoConverter::default();
    for (short_url, hostname) in [
        ("é5150", "example.org"),
        ("Ué150", "example.org"),
        ("U5150", "é.example.org"),
        ("U5150", "example.orgé"),
        ("W5150", "[::1]é"),
    ] {
        let short_dial_info = ShortDialInfo {
            short_url: short_url.to_owned(),
            hostname: hostname.to_owned(),
        };
        assert!(dial_info_converter
            .try_vec_from_short(&short_dial_info)
            .await
            .is_err());
    }
}

/// A record parsed for serving is verified but never resolved
pub async fn test_bootstrap_record_unresolved() {
    let registry = mock_registry::init("unresolved").await;
    let network_manager = registry.network_manager();
    let dial_info_converter = MockDialInfoConverter::default();
    let signing_keys = mock_signing_keys();

    let v1str = mock_v1_string(&network_manager, &dial_info_converter).await;
    let baseline = dial_info_converter.resolve_count();

    // Parsing for serving consults no converter at all
    let unresolved =
        BootstrapRecord::try_new_from_v1_str(&network_manager, None, &v1str, &signing_keys)
            .await
            .expect("should parse string")
            .expect("should be valid record");
    assert!(unresolved.dial_info_details().is_empty());
    assert_eq!(dial_info_converter.resolve_count(), baseline);

    // Same record, resolved, differs only in its dial info
    let resolved = BootstrapRecord::try_new_from_v1_str(
        &network_manager,
        Some(&dial_info_converter),
        &v1str,
        &signing_keys,
    )
    .await
    .expect("should parse string")
    .expect("should be valid record");
    assert_eq!(resolved.public_keys(), unresolved.public_keys());
    assert!(!resolved.dial_info_details().is_empty());
    assert!(dial_info_converter.resolve_count() > baseline);

    // A config file is a likely source of trailing whitespace, and the signature covers
    // the record exactly as written
    let padded = format!("  {}\n", v1str);
    assert!(
        BootstrapRecord::try_new_from_v1_str(&network_manager, None, &padded, &signing_keys)
            .await
            .expect("should parse string")
            .is_some()
    );

    // A record no configured key signed is refused
    let mut tampered = v1str.clone();
    tampered.insert(tampered.rfind('|').expect("has a signature") + 1, 'A');
    assert!(
        BootstrapRecord::try_new_from_v1_str(&network_manager, None, &tampered, &signing_keys)
            .await
            .is_err()
    );

    mock_registry::terminate(registry).await;
}

/// Configured records are served without a bootstrap ever running, and without claiming one did.
/// Both record versions are accepted: what a node serves is not limited to what it can verify.
pub async fn test_config_bootstrap_records() {
    // Mint the records to configure the next node with
    let registry = mock_registry::init("configsrc").await;
    let dial_info_converter = MockDialInfoConverter::default();
    let v1str = mock_v1_string(&registry.network_manager(), &dial_info_converter).await;
    let v0str = make_mock_bootstrap_record(false)
        .to_v0_string(&dial_info_converter)
        .await
        .expect("should make string");
    mock_registry::terminate(registry).await;

    let configured = vec![v1str.clone(), v0str.clone()];
    let registry = mock_registry::init_with_config("configured", move |config| {
        config.network.routing_table.bootstrap_keys = mock_signing_keys();
        config.network.routing_table.bootstrap_records = configured;
    })
    .await;
    let network_manager = registry.network_manager();
    let routing_table = registry.routing_table();

    // What RoutingTable::startup does with them
    let cache = network_manager.config_bootstrap_cache().await;
    let rdc = routing_table.get_routing_domain_controller(RoutingDomain::PublicInternet);
    rdc.read_dyn().set_config_bootstrap_cache(cache);

    let response = network_manager
        .boot_v1_response()
        .expect("should serve configured records");
    let mut expected = vec![v1str, v0str];
    expected.sort_unstable();
    assert_eq!(response.records, expected);

    // Configuring records is not the same as having bootstrapped
    assert!(rdc.read_dyn().get_bootstrap_cache().is_none());

    mock_registry::terminate(registry).await;
}

/// A hostile bootstrap cannot grow every response of every node that contacted it
pub fn test_bootstrap_cache_cap() {
    let strings: Vec<String> = (0..MAX_BOOTSTRAP_RECORD_STRINGS * 2)
        .map(|i| format!("1|0|VLD0:k{}|example.com|T5150|1|sig", i))
        .collect();
    let cache = BootstrapCache::new(strings, vec![]);
    assert_eq!(cache.record_strings().len(), MAX_BOOTSTRAP_RECORD_STRINGS);
}
