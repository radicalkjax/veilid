use super::*;
use crate::tests::*;

/// A key no other call returns
fn fresh_bootstrap_key() -> PublicKey {
    let (public_keys, _) = fix_identity();
    public_keys[0].clone()
}

/// Saves a table holding one entry, then starts a second core on the same storage and
/// returns how many entries it loaded. Neither core configures a bootstrap unless told to,
/// so the no-recorded-bootstrap reset stays out of the way. `forget_identity` deletes the
/// generated identity a test-util build stores (a cfg(test) core never stores one), so the
/// second core generates another.
async fn reload_entry_count(
    first: impl FnOnce(&mut VeilidConfigRoutingTable),
    second: impl FnOnce(&mut VeilidConfigRoutingTable),
    forget_identity: bool,
) -> usize {
    let (registry, storage) = mock_registry::init_with_storage("rt_cache_validity", |config| {
        config.network.routing_table.bootstrap = vec![];
        first(&mut config.network.routing_table);
    })
    .await;
    {
        let routing_table = registry.routing_table();
        seed_nodes(&routing_table, 1);
        assert_eq!(
            routing_table.inner.read().bucket_entry_count(),
            1,
            "the first core should hold one entry to save"
        );
    }
    if forget_identity {
        registry
            .table_store()
            .delete("__veilid_config")
            .await
            .expect("should delete the stored identity");
    }
    mock_registry::terminate(registry).await;

    let registry = mock_registry::reopen("rt_cache_validity", storage, |config| {
        config.network.routing_table.bootstrap = vec![];
        second(&mut config.network.routing_table);
    })
    .await;
    let loaded = registry.routing_table().inner.read().bucket_entry_count();
    mock_registry::terminate(registry).await;
    loaded
}

/// The same identity and configuration load the saved table
pub async fn test_cache_validity_same_config_loads() {
    info!("--- test_cache_validity_same_config_loads ---");

    let identity = fix_identity();
    let loaded = reload_entry_count(
        |rt| set_identity(rt, &identity),
        |rt| set_identity(rt, &identity),
        false,
    )
    .await;
    assert_eq!(loaded, 1, "an unchanged core should load its saved table");
}

/// A generated identity is part of what the table was saved under
pub async fn test_cache_validity_generated_identity() {
    info!("--- test_cache_validity_generated_identity ---");

    let loaded = reload_entry_count(|_| {}, |_| {}, true).await;
    assert_eq!(
        loaded, 0,
        "a table saved under another generated identity must not load"
    );
}

/// Bootstrap lists that concatenate to the same bytes are still different lists
pub async fn test_cache_validity_bootstrap_list() {
    info!("--- test_cache_validity_bootstrap_list ---");

    let identity = fix_identity();
    // Configured records keep a bootstrap list from tripping the no-recorded-bootstrap reset
    let records = vec!["1|0|VLD0:k|example.com|T5150|1|sig".to_owned()];
    let loaded = reload_entry_count(
        |rt| {
            set_identity(rt, &identity);
            rt.bootstrap = vec!["ab".to_owned(), "c".to_owned()];
            rt.bootstrap_records = records.clone();
        },
        |rt| {
            set_identity(rt, &identity);
            rt.bootstrap = vec!["a".to_owned(), "bc".to_owned()];
            rt.bootstrap_records = records.clone();
        },
        false,
    )
    .await;
    assert_eq!(
        loaded, 0,
        "a table saved under another bootstrap list must not load"
    );
}

/// The keys trusted to sign bootstrap records are part of what the table was saved under
pub async fn test_cache_validity_bootstrap_keys() {
    info!("--- test_cache_validity_bootstrap_keys ---");

    let identity = fix_identity();
    let loaded = reload_entry_count(
        |rt| {
            set_identity(rt, &identity);
            rt.bootstrap_keys = vec![fresh_bootstrap_key()];
        },
        |rt| {
            set_identity(rt, &identity);
            rt.bootstrap_keys = vec![fresh_bootstrap_key()];
        },
        false,
    )
    .await;
    assert_eq!(
        loaded, 0,
        "a table saved under other bootstrap keys must not load"
    );
}

/// Configured bootstrap records are part of what the table was saved under
pub async fn test_cache_validity_bootstrap_records() {
    info!("--- test_cache_validity_bootstrap_records ---");

    let identity = fix_identity();
    let loaded = reload_entry_count(
        |rt| {
            set_identity(rt, &identity);
            rt.bootstrap_records = vec!["1|0|VLD0:k|example.com|T5150|1|sig".to_owned()];
        },
        |rt| {
            set_identity(rt, &identity);
            rt.bootstrap_records = vec!["1|0|VLD0:k|example.org|T5150|1|sig".to_owned()];
        },
        false,
    )
    .await;
    assert_eq!(
        loaded, 0,
        "a table saved under other bootstrap records must not load"
    );
}
