use super::*;
use crate::network_manager::*;
use crate::routing_table::*;

const BOOTSTRAP_RECORDS_WAIT_MS: u32 = 90_000;
const DIRECT_BOOTSTRAP_ATTEMPTS: u32 = 30;
const POLL_MS: u32 = 1000;

fn registry(api: &VeilidAPI) -> VeilidComponentRegistry {
    api.core_context().expect("api not started").registry()
}

fn as_set(strings: &[String]) -> BTreeSet<String> {
    strings.iter().cloned().collect()
}

async fn wait_for_bootstrap_records(api: &VeilidAPI) -> BootstrapCache {
    let routing_table = registry(api).routing_table();
    let mut waited_ms = 0;
    loop {
        let cache = routing_table.get_served_bootstrap_cache(RoutingDomain::PublicInternet);
        if !cache.is_empty() || waited_ms >= BOOTSTRAP_RECORDS_WAIT_MS {
            return cache;
        }
        sleep(POLL_MS).await;
        waited_ms += POLL_MS;
    }
}

async fn wait_for_network_started(api: &VeilidAPI) {
    while !api.get_state().await.unwrap().network.started {
        sleep(POLL_MS).await;
    }
}

fn local_udp_bootstrap_url(api: &VeilidAPI) -> String {
    let did = registry(api)
        .routing_table()
        .dial_info_details(RoutingDomain::LocalNetwork)
        .into_iter()
        .find(|d| {
            d.dial_info.protocol_type() == ProtocolType::UDP
                && d.dial_info.address_type() == AddressType::IPV4
        })
        .expect("no local ipv4 udp dial info");
    format!("udp://{}", did.dial_info.to_socket_addr())
}

/// Node A bootstraps from DNS and keeps its verified v1 records.
/// Node B lists only A as a bootstrap, so everything it learns arrives over B01T.
async fn test_txt_then_direct() -> BTreeSet<String> {
    info!("test_txt_then_direct: starting node A");
    let (update_callback, mut config) = fixture_veilid_core();
    // Serving direct bootstrap is opt-in, and A is not among its own bootstraps
    config.network.routing_table.serve_direct_bootstrap = true;
    let api_a = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    api_a.attach().await.unwrap();
    let _ = api_a
        .wait_for_attachment_condition(None, None)
        .await
        .unwrap();

    let cache_a = wait_for_bootstrap_records(&api_a).await;
    assert!(!cache_a.is_empty(), "TXT bootstrap stored no records");
    let strings_a = as_set(cache_a.record_strings());
    let response = registry(&api_a)
        .network_manager()
        .boot_v1_response()
        .expect("node A should serve B01T");
    assert_eq!(
        response.records.iter().cloned().collect::<BTreeSet<_>>(),
        strings_a
    );

    // A never bootstraps from itself, so a request to its own dial info is not sent
    let own_dial_info = BootstrapDialInfoConverter::default()
        .try_vec_from_url(&local_udp_bootstrap_url(&api_a))
        .await
        .expect("bad bootstrap url")
        .remove(0);
    let own_peers = registry(&api_a)
        .network_manager()
        .direct_bootstrap(own_dial_info)
        .await
        .expect("direct bootstrap failed");
    assert!(own_peers.peers.is_empty() && own_peers.record_strings.is_empty());

    info!("test_txt_then_direct: starting node B");
    let url = local_udp_bootstrap_url(&api_a);
    let (update_callback, mut config) = fixture_veilid_core();
    config.network.routing_table.bootstrap = vec![url.clone()];
    let api_b = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    api_b.attach().await.unwrap();
    wait_for_network_started(&api_b).await;

    // One B01T round trip returns exactly what A stored
    let dial_info = BootstrapDialInfoConverter::default()
        .try_vec_from_url(&url)
        .await
        .expect("bad bootstrap url")
        .remove(0);
    let network_manager_b = registry(&api_b).network_manager();
    let mut strings_b = BTreeSet::new();
    for _ in 0..DIRECT_BOOTSTRAP_ATTEMPTS {
        let peers = network_manager_b
            .direct_bootstrap(dial_info.clone())
            .await
            .expect("direct bootstrap failed");
        strings_b = as_set(&peers.record_strings);
        if !strings_b.is_empty() {
            break;
        }
        sleep(POLL_MS).await;
    }
    assert_eq!(strings_b, strings_a);

    // B's own bootstrap task records what A handed it
    let cache_b = wait_for_bootstrap_records(&api_b).await;
    assert!(!cache_b.is_empty(), "direct bootstrap stored no records");
    assert!(as_set(cache_b.record_strings()).is_subset(&strings_a));

    // Having recorded a bootstrap, neither node's saved table is condemned on the next start
    registry(&api_a)
        .routing_table()
        .flush(SaveReason::Bootstrap)
        .await;
    assert!(!persisted_state_requires_reset(&api_a).await);

    info!("test_txt_then_direct: shutting down");
    api_b.shutdown().await;
    api_a.shutdown().await;

    strings_a
}

/// Node C is told the records outright, so it bootstraps with no lookup of any kind.
/// It is configured with both the v0 and v1 strings a TXT lookup yields, serves both, and
/// bootstraps from the v1 ones because it has keys to check them with.
async fn test_config_records_bootstrap(strings_a: &BTreeSet<String>) {
    assert!(
        strings_a.iter().any(|s| s.starts_with("0|")),
        "expected v0 strings to configure"
    );
    assert!(
        strings_a.iter().any(|s| s.starts_with("1|")),
        "expected v1 strings to configure"
    );

    info!("test_config_records_bootstrap: starting node C");
    let (update_callback, mut config) = fixture_veilid_core();
    config.network.routing_table.bootstrap = vec![];
    config.network.routing_table.bootstrap_records = strings_a.iter().cloned().collect();
    let api_c = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    api_c.attach().await.unwrap();

    // Served before anything is bootstrapped, straight from the config, both versions
    wait_for_network_started(&api_c).await;
    let served = registry(&api_c)
        .routing_table()
        .get_served_bootstrap_cache(RoutingDomain::PublicInternet);
    assert_eq!(as_set(served.record_strings()), *strings_a);

    // And good enough to join with
    let _ = api_c
        .wait_for_attachment_condition(None, None)
        .await
        .unwrap();

    info!("test_config_records_bootstrap: shutting down");
    api_c.shutdown().await;
}

async fn persisted_state_requires_reset(api: &VeilidAPI) -> bool {
    let registry = registry(api);
    let table_store = registry.table_store();
    let db = table_store
        .open(
            "routing_table",
            crate::routing_table::ROUTING_TABLE_COLUMN_COUNT,
        )
        .await
        .expect("should open routing table db");
    let routing_table = registry.routing_table();
    let rdc = routing_table.get_routing_domain_controller(RoutingDomain::PublicInternet);
    rdc.persisted_state_requires_reset(&db).await
}

pub async fn test_all() {
    let strings_a = test_txt_then_direct().await;
    test_config_records_bootstrap(&strings_a).await;
}
