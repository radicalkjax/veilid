use super::*;

use crate::network_manager::*;
use crate::routing_table::*;
use crate::storage_manager::*;

pub(crate) async fn init<S: AsRef<str>>(namespace: S) -> VeilidComponentRegistry {
    init_with_config(namespace, |_| {}).await
}

pub(crate) async fn init_with_config<S: AsRef<str>>(
    namespace: S,
    modify_config: impl FnOnce(&mut VeilidConfig),
) -> VeilidComponentRegistry {
    let (update_callback, mut config) = fixture_veilid_core_with_namespace(namespace);
    modify_config(&mut config);
    let startup_options = VeilidStartupOptions::try_new(config, update_callback).unwrap();
    let registry = VeilidComponentRegistry::new(startup_options);
    registry.enable_mock();
    registry.register(ProtectedStore::new);
    registry.register(TableStore::new);
    registry.register(Crypto::new);
    registry.register(StorageManager::new);
    registry.register_with_context(RoutingTable::new, RoutingTableStartupContext::default());
    registry.register_with_context(NetworkManager::new, NetworkManagerStartupContext::default());

    registry.init().await.expect("should init");
    registry.post_init().await.expect("should post init");

    registry
}

pub(crate) async fn terminate(registry: VeilidComponentRegistry) {
    registry.pre_terminate().await;
    registry.terminate().await;
}

/// Where a mock core keeps its stores, so a later mock core can reopen them
pub(crate) struct MockStorage {
    /// The protected store finds the table store's key by program name
    program_name: String,
    /// Table store directory
    table_store_directory: String,
    /// Protected store directory
    protected_store_directory: String,
}

/// Like `init_with_config`, also returning where the core keeps its stores
pub(crate) async fn init_with_storage<S: AsRef<str>>(
    namespace: S,
    modify_config: impl FnOnce(&mut VeilidConfig),
) -> (VeilidComponentRegistry, MockStorage) {
    let mut storage = None;
    let registry = init_with_config(namespace, |config| {
        modify_config(config);
        storage = Some(MockStorage {
            program_name: config.program_name.clone(),
            table_store_directory: config.table_store.directory.clone(),
            protected_store_directory: config.protected_store.directory.clone(),
        });
    })
    .await;
    (registry, storage.expect("config should have been built"))
}

/// Start a mock core on the stores of an earlier one, keeping what they hold
pub(crate) async fn reopen<S: AsRef<str>>(
    namespace: S,
    storage: MockStorage,
    modify_config: impl FnOnce(&mut VeilidConfig),
) -> VeilidComponentRegistry {
    init_with_config(namespace, |config| {
        config.program_name = storage.program_name;
        config.table_store.directory = storage.table_store_directory;
        config.table_store.delete = false;
        config.protected_store.directory = storage.protected_store_directory;
        config.protected_store.delete = false;
        modify_config(config);
    })
    .await
}
