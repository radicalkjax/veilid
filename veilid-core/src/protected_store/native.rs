use super::*;
use keyring_manager::*;
use std::path::Path;

/// The active protected store backend.
pub(super) enum ProtectedStoreBackend {
    Keyring(KeyringBackend),
}

impl ProtectedStoreBackend {
    pub(super) async fn get(&self, key: &str) -> VeilidAPIResult<Option<Zeroizing<String>>> {
        match self {
            ProtectedStoreBackend::Keyring(b) => b.get(key).await,
        }
    }
    pub(super) async fn set(&self, key: &str, value: &str) -> VeilidAPIResult<bool> {
        match self {
            ProtectedStoreBackend::Keyring(b) => b.set(key, value).await,
        }
    }
    pub(super) async fn remove(&self, key: &str) -> VeilidAPIResult<bool> {
        match self {
            ProtectedStoreBackend::Keyring(b) => b.remove(key).await,
        }
    }
    pub(super) async fn list(&self) -> VeilidAPIResult<Vec<String>> {
        match self {
            ProtectedStoreBackend::Keyring(b) => b.list().await,
        }
    }
}

/// A keyring reported that the store could not be read or written because the device is locked.
/// The stored value is intact, so this is transient and never absence.
fn locked_error() -> VeilidAPIError {
    VeilidAPIError::try_again("protected store is locked, retry once the device is unlocked")
}

/// Protected store backend backed by the OS keyring (or an on-disk insecure keyring).
///
/// Keyring calls can block on user input (e.g. a keychain permission dialog), so each runs on a
/// blocking task rather than the async executor.
pub(super) struct KeyringBackend {
    km: Arc<Mutex<KeyringManager>>,
    service: String,
}

impl KeyringBackend {
    async fn get(&self, key: &str) -> VeilidAPIResult<Option<Zeroizing<String>>> {
        let km = self.km.clone();
        let service = self.service.clone();
        let key = key.to_owned();
        blocking_yielding(move || {
            match km.lock().with_keyring(&service, &key, |kr| kr.get_value()) {
                Ok(v) => Ok(Some(Zeroizing::new(v))),
                Err(KeyringError::NoPasswordFound) => Ok(None),
                Err(KeyringError::Locked) => Err(locked_error()),
                Err(e) => Err(VeilidAPIError::generic(format!(
                    "failed to load user secret: {}",
                    e
                ))),
            }
        })
        .await
    }

    async fn set(&self, key: &str, value: &str) -> VeilidAPIResult<bool> {
        let km = self.km.clone();
        let service = self.service.clone();
        let key = key.to_owned();
        let value = Zeroizing::new(value.to_owned());
        blocking_yielding(move || {
            match km.lock().with_keyring(&service, &key, |kr| {
                let existed = kr.get_value().is_ok();
                kr.set_value(value.as_str())?;
                Ok(existed)
            }) {
                Ok(existed) => Ok(existed),
                Err(KeyringError::Locked) => Err(locked_error()),
                Err(e) => Err(VeilidAPIError::generic(format!(
                    "failed to save user secret: {}",
                    e
                ))),
            }
        })
        .await
    }

    async fn remove(&self, key: &str) -> VeilidAPIResult<bool> {
        let km = self.km.clone();
        let service = self.service.clone();
        let key = key.to_owned();
        blocking_yielding(move || {
            match km
                .lock()
                .with_keyring(&service, &key, |kr| kr.delete_value())
            {
                Ok(_) => Ok(true),
                Err(KeyringError::NoPasswordFound) => Ok(false),
                Err(KeyringError::Locked) => Err(locked_error()),
                Err(e) => Err(VeilidAPIError::generic(format!(
                    "failed to remove user secret: {}",
                    e
                ))),
            }
        })
        .await
    }

    async fn list(&self) -> VeilidAPIResult<Vec<String>> {
        let km = self.km.clone();
        let service = self.service.clone();
        blocking_yielding(move || match km.lock().list_keys(&service) {
            Ok(keys) => Ok(keys),
            Err(KeyringError::Unsupported) => Err(VeilidAPIError::generic(
                "this protected store backend does not support listing keys",
            )),
            Err(KeyringError::Locked) => Err(locked_error()),
            Err(e) => Err(VeilidAPIError::generic(format!(
                "failed to list user secrets: {}",
                e
            ))),
        })
        .await
    }
}

fn service_name(config: &VeilidConfig) -> String {
    if config.namespace.is_empty() {
        "veilid_protected_store".to_owned()
    } else {
        format!("veilid_protected_store_{}", config.namespace)
    }
}

pub(super) async fn build_backend(ps: &ProtectedStore) -> VeilidAPIResult<ProtectedStoreBackend> {
    let config = ps.config();

    match config.protected_store.backend {
        VeilidConfigProtectedStoreBackend::PlatformDefault => {}
        // VeilidConfigProtectedStoreBackend::Callback => {
        //     apibail_generic!("callback protected store backend is not yet implemented");
        // }
        VeilidConfigProtectedStoreBackend::LocalStorage
        | VeilidConfigProtectedStoreBackend::IndexedDb
        | VeilidConfigProtectedStoreBackend::ExtensionStorage => {
            apibail_generic!("browser protected store backends are not available on native");
        }
    }

    let mut keyring_manager: Option<KeyringManager> = None;

    if !config.protected_store.always_use_insecure_storage {
        // Construction probes the store and can block on an unlock dialog, so it runs off the async threads
        let program_name = config.program_name.clone();
        let maybe_km = blocking_yielding(move || {
            cfg_if! {
                if #[cfg(target_os = "android")] {
                    KeyringManager::new_secure(&program_name, crate::veilid_api::android::get_android_globals())
                } else {
                    KeyringManager::new_secure(&program_name)
                }
            }
        })
        .await;
        keyring_manager = match maybe_km {
            Ok(v) => Some(v),
            // A locked store is not a missing store; an insecure fallback would re-key over what it still holds
            Err(KeyringError::Locked) => return Err(locked_error()),
            Err(e) => {
                veilid_log!(ps info "Secure key storage service unavailable, falling back to direct disk-based storage: {}", e);
                None
            }
        };
    }

    if (config.protected_store.always_use_insecure_storage
        || config.protected_store.allow_insecure_fallback)
        && keyring_manager.is_none()
    {
        let directory = Path::new(&config.protected_store.directory);
        let insecure_keyring_file = directory.to_owned().join(format!(
            "insecure_keyring{}",
            if config.namespace.is_empty() {
                "".to_owned()
            } else {
                format!("_{}", config.namespace)
            }
        ));

        ensure_file_private_owner(&insecure_keyring_file).map_err(VeilidAPIError::generic)?;

        keyring_manager = Some(
            KeyringManager::new_insecure(&config.program_name, &insecure_keyring_file).map_err(
                |e| VeilidAPIError::generic(format!("failed to create insecure keyring: {}", e)),
            )?,
        );
    }

    let Some(keyring_manager) = keyring_manager else {
        apibail_generic!("Could not initialize the protected store.");
    };

    Ok(ProtectedStoreBackend::Keyring(KeyringBackend {
        km: Arc::new(Mutex::new(keyring_manager)),
        service: service_name(&config),
    }))
}
