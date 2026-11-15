use crate::routing_table::*;

fn config(bootstrap: &[&str], bootstrap_records: &[&str]) -> VeilidConfigRoutingTable {
    VeilidConfigRoutingTable {
        bootstrap: bootstrap.iter().map(|x| x.to_string()).collect(),
        bootstrap_records: bootstrap_records.iter().map(|x| x.to_string()).collect(),
        serve_direct_bootstrap: false,
        ..Default::default()
    }
}

fn persisted(bootstrap: Option<BootstrapCache>) -> PublicInternetPersistedState {
    PublicInternetPersistedState {
        low_water_mark: LowWaterMark::default(),
        bootstrap,
    }
}

/// State written before a bootstrap ever recorded its result, which is every older build
pub fn test_persisted_reset_no_recorded_bootstrap() {
    info!("--- test_persisted_reset_no_recorded_bootstrap ---");

    let cfg = config(&["bootstrap-v1.veilid.net"], &[]);
    assert!(persisted_requires_reset(Some(&persisted(None)), &cfg));
    assert!(persisted_requires_reset(None, &cfg));
}

/// A bootstrap that ran and found nothing to cache still counts as having run.
/// A keyless node bootstrapping from a direct url is handed no records at all.
pub fn test_persisted_reset_recorded_empty() {
    info!("--- test_persisted_reset_recorded_empty ---");

    let cfg = config(&["udp://127.0.0.1:5150"], &[]);
    let state = persisted(Some(BootstrapCache::default()));
    assert!(!persisted_requires_reset(Some(&state), &cfg));
}

pub fn test_persisted_reset_recorded_records() {
    info!("--- test_persisted_reset_recorded_records ---");

    let cfg = config(&["bootstrap-v1.veilid.net"], &[]);
    let state = persisted(Some(BootstrapCache::new(
        vec!["1|0|VLD0:k|example.com|T5150|1|sig".to_owned()],
        vec![],
    )));
    assert!(!persisted_requires_reset(Some(&state), &cfg));
}

/// Configured records are served no matter what the table holds, so there is nothing to repair
pub fn test_persisted_reset_config_records() {
    info!("--- test_persisted_reset_config_records ---");

    let cfg = config(
        &["bootstrap-v1.veilid.net"],
        &["1|0|VLD0:k|example.com|T5150|1|sig"],
    );
    assert!(!persisted_requires_reset(Some(&persisted(None)), &cfg));
}

/// A node with no bootstraps configured has nothing to bootstrap from, so wiping its table
/// would only make it worse
pub fn test_persisted_reset_no_bootstraps() {
    info!("--- test_persisted_reset_no_bootstraps ---");

    let cfg = config(&[], &[]);
    assert!(!persisted_requires_reset(Some(&persisted(None)), &cfg));
}
