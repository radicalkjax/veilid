use super::*;
use js_sys::{global, Function, Object, Reflect};
use keyvaluedb::{DBTransaction, KeyValueDB as _};
use keyvaluedb_web::Database;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Storage, Window};

/// The active protected store backend.
pub(super) enum ProtectedStoreBackend {
    LocalStorage(LocalStorageBackend),
    IndexedDb(IndexedDbBackend),
    Extension(ExtensionBackend),
}

impl ProtectedStoreBackend {
    pub(super) async fn get(&self, key: &str) -> VeilidAPIResult<Option<Zeroizing<String>>> {
        match self {
            ProtectedStoreBackend::LocalStorage(b) => b.get(key),
            ProtectedStoreBackend::IndexedDb(b) => b.get(key).await,
            ProtectedStoreBackend::Extension(b) => b.get(key).await,
        }
    }
    pub(super) async fn set(&self, key: &str, value: &str) -> VeilidAPIResult<bool> {
        match self {
            ProtectedStoreBackend::LocalStorage(b) => b.set(key, value),
            ProtectedStoreBackend::IndexedDb(b) => b.set(key, value).await,
            ProtectedStoreBackend::Extension(b) => b.set(key, value).await,
        }
    }
    pub(super) async fn remove(&self, key: &str) -> VeilidAPIResult<bool> {
        match self {
            ProtectedStoreBackend::LocalStorage(b) => b.remove(key),
            ProtectedStoreBackend::IndexedDb(b) => b.remove(key).await,
            ProtectedStoreBackend::Extension(b) => b.remove(key).await,
        }
    }
    pub(super) async fn list(&self) -> VeilidAPIResult<Vec<String>> {
        match self {
            ProtectedStoreBackend::LocalStorage(b) => b.list(),
            ProtectedStoreBackend::IndexedDb(b) => b.list().await,
            ProtectedStoreBackend::Extension(b) => b.list().await,
        }
    }
}

fn map_js_error<M: ToString>(message: M) -> impl FnOnce(JsValue) -> VeilidAPIError {
    move |x| {
        VeilidAPIError::generic(format!(
            "{}: {}",
            message.to_string(),
            x.as_string().unwrap_or_default()
        ))
    }
}

/// Prefix that namespaces this store's keys within a shared browser key-value store.
fn key_prefix(config: &VeilidConfig) -> String {
    if config.namespace.is_empty() {
        "__veilid_protected_store_".to_owned()
    } else {
        format!("__veilid_protected_store_{}_", config.namespace)
    }
}

/// Protected store backed by browser `localStorage` (Window contexts only).
pub(super) struct LocalStorageBackend {
    prefix: String,
}

impl LocalStorageBackend {
    fn storage() -> VeilidAPIResult<Storage> {
        let win = window().ok_or_else(|| VeilidAPIError::generic("no window"))?;
        win.local_storage()
            .map_err(map_js_error("failed to get local storage"))?
            .ok_or_else(|| VeilidAPIError::generic("no local storage"))
    }

    fn get(&self, key: &str) -> VeilidAPIResult<Option<Zeroizing<String>>> {
        Ok(Self::storage()?
            .get_item(&format!("{}{}", self.prefix, key))
            .map_err(map_js_error("failed to read local storage"))?
            .map(Zeroizing::new))
    }

    fn set(&self, key: &str, value: &str) -> VeilidAPIResult<bool> {
        let ls = Self::storage()?;
        let vkey = format!("{}{}", self.prefix, key);
        let existed = ls
            .get_item(&vkey)
            .map_err(map_js_error("failed to read local storage"))?
            .is_some();
        ls.set_item(&vkey, value)
            .map_err(map_js_error("failed to write local storage"))?;
        Ok(existed)
    }

    fn remove(&self, key: &str) -> VeilidAPIResult<bool> {
        let ls = Self::storage()?;
        let vkey = format!("{}{}", self.prefix, key);
        let existed = ls
            .get_item(&vkey)
            .map_err(map_js_error("failed to read local storage"))?
            .is_some();
        if existed {
            ls.delete(&vkey)
                .map_err(map_js_error("failed to remove from local storage"))?;
        }
        Ok(existed)
    }

    fn list(&self) -> VeilidAPIResult<Vec<String>> {
        let ls = Self::storage()?;
        let len = ls
            .length()
            .map_err(map_js_error("failed to read local storage length"))?;
        let mut out = Vec::new();
        for i in 0..len {
            if let Some(k) = ls
                .key(i)
                .map_err(map_js_error("failed to read local storage key"))?
            {
                if let Some(logical) = k.strip_prefix(&self.prefix) {
                    out.push(logical.to_owned());
                }
            }
        }
        Ok(out)
    }
}

/// Protected store backed by IndexedDB, usable in any worker context.
pub(super) struct IndexedDbBackend {
    db: Database,
}

impl IndexedDbBackend {
    async fn open(table_name: &str) -> VeilidAPIResult<Self> {
        let db = Database::open(table_name, 1, false).await.map_err(|e| {
            VeilidAPIError::generic(format!("failed to open protected store: {}", e))
        })?;
        Ok(Self { db })
    }

    async fn get(&self, key: &str) -> VeilidAPIResult<Option<Zeroizing<String>>> {
        match self
            .db
            .get(0, key.as_bytes())
            .await
            .map_err(|e| VeilidAPIError::generic(format!("failed to load user secret: {}", e)))?
        {
            Some(v) => {
                let s = String::from_utf8(v.to_vec())
                    .map_err(|e| VeilidAPIError::generic(format!("invalid utf8: {}", e)))?;
                Ok(Some(Zeroizing::new(s)))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &str) -> VeilidAPIResult<bool> {
        let existed = self
            .db
            .get(0, key.as_bytes())
            .await
            .map_err(|e| VeilidAPIError::generic(format!("failed to read protected store: {}", e)))?
            .is_some();
        let mut tx = DBTransaction::new();
        tx.put(0, key.as_bytes(), value.as_bytes());
        self.db
            .write(tx)
            .await
            .map_err(|e| VeilidAPIError::generic(format!("failed to save user secret: {}", e)))?;
        Ok(existed)
    }

    async fn remove(&self, key: &str) -> VeilidAPIResult<bool> {
        let existed = self
            .db
            .delete(0, key.as_bytes())
            .await
            .map_err(|e| VeilidAPIError::generic(format!("failed to remove user secret: {}", e)))?
            .is_some();
        Ok(existed)
    }

    async fn list(&self) -> VeilidAPIResult<Vec<String>> {
        let (keys, _) = self
            .db
            .iter_keys(0, None, Vec::<String>::new(), |ctx, key| {
                ctx.push(String::from_utf8_lossy(key).into_owned());
                Ok::<Option<()>, std::io::Error>(None)
            })
            .await
            .map_err(|e| VeilidAPIError::generic(format!("failed to list user secrets: {}", e)))?;
        Ok(keys)
    }
}

/// Protected store backed by WebExtension storage (`chrome.storage.local`/`browser.storage.local`).
pub(super) struct ExtensionBackend {
    prefix: String,
}

impl ExtensionBackend {
    /// Resolve `chrome.storage.local` or `browser.storage.local`, whichever the runtime provides.
    fn storage_area() -> VeilidAPIResult<Object> {
        for ns in ["chrome", "browser"] {
            let Ok(root) = Reflect::get(global().as_ref(), &JsValue::from_str(ns)) else {
                continue;
            };
            let Ok(storage) = Reflect::get(&root, &JsValue::from_str("storage")) else {
                continue;
            };
            let Ok(local) = Reflect::get(&storage, &JsValue::from_str("local")) else {
                continue;
            };
            if local.is_object() {
                return Ok(local.unchecked_into());
            }
        }
        Err(VeilidAPIError::generic("extension storage not available"))
    }

    async fn call_promise(area: &Object, method: &str, arg: &JsValue) -> VeilidAPIResult<JsValue> {
        let func: Function = Reflect::get(area, &JsValue::from_str(method))
            .map_err(map_js_error("extension storage method missing"))?
            .unchecked_into();
        let promise = func
            .call1(area, arg)
            .map_err(map_js_error("extension storage call failed"))?;
        JsFuture::from(js_sys::Promise::from(promise))
            .await
            .map_err(map_js_error("extension storage rejected"))
    }

    async fn get(&self, key: &str) -> VeilidAPIResult<Option<Zeroizing<String>>> {
        let area = Self::storage_area()?;
        let vkey = format!("{}{}", self.prefix, key);
        let result = Self::call_promise(&area, "get", &JsValue::from_str(&vkey)).await?;
        match Reflect::get(&result, &JsValue::from_str(&vkey))
            .ok()
            .and_then(|v| v.as_string())
        {
            Some(s) => Ok(Some(Zeroizing::new(s))),
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &str) -> VeilidAPIResult<bool> {
        let area = Self::storage_area()?;
        let vkey = format!("{}{}", self.prefix, key);
        let existed = self.get(key).await?.is_some();
        let obj = Object::new();
        Reflect::set(&obj, &JsValue::from_str(&vkey), &JsValue::from_str(value))
            .map_err(map_js_error("failed to build extension storage record"))?;
        Self::call_promise(&area, "set", obj.as_ref()).await?;
        Ok(existed)
    }

    async fn remove(&self, key: &str) -> VeilidAPIResult<bool> {
        let area = Self::storage_area()?;
        let vkey = format!("{}{}", self.prefix, key);
        let existed = self.get(key).await?.is_some();
        Self::call_promise(&area, "remove", &JsValue::from_str(&vkey)).await?;
        Ok(existed)
    }

    async fn list(&self) -> VeilidAPIResult<Vec<String>> {
        let area = Self::storage_area()?;
        let all = Self::call_promise(&area, "get", &JsValue::NULL).await?;
        let obj: Object = all.unchecked_into();
        let mut out = Vec::new();
        for k in Object::keys(&obj).iter() {
            if let Some(k) = k.as_string() {
                if let Some(logical) = k.strip_prefix(&self.prefix) {
                    out.push(logical.to_owned());
                }
            }
        }
        Ok(out)
    }
}

/// The `Window` scope if present (localStorage requires it).
fn window() -> Option<Window> {
    let g: JsValue = global().into();
    g.dyn_into::<Window>().ok()
}

fn resolve_backend(
    configured: VeilidConfigProtectedStoreBackend,
) -> VeilidConfigProtectedStoreBackend {
    use VeilidConfigProtectedStoreBackend::*;
    if configured != PlatformDefault {
        return configured;
    }
    match wasm_runtime_context() {
        WasmRuntimeContext::Extension => ExtensionStorage,
        WasmRuntimeContext::Window => LocalStorage,
        WasmRuntimeContext::DedicatedWorker
        | WasmRuntimeContext::SharedWorker
        | WasmRuntimeContext::ServiceWorker
        | WasmRuntimeContext::Unknown => IndexedDb,
    }
}

pub(super) async fn build_backend(ps: &ProtectedStore) -> VeilidAPIResult<ProtectedStoreBackend> {
    let config = ps.config();
    let prefix = key_prefix(&config);

    match resolve_backend(config.protected_store.backend) {
        VeilidConfigProtectedStoreBackend::LocalStorage
        | VeilidConfigProtectedStoreBackend::PlatformDefault => {
            Ok(ProtectedStoreBackend::LocalStorage(LocalStorageBackend {
                prefix,
            }))
        }
        VeilidConfigProtectedStoreBackend::IndexedDb => {
            let table_name = prefix.trim_end_matches('_').to_owned();
            Ok(ProtectedStoreBackend::IndexedDb(
                IndexedDbBackend::open(&table_name).await?,
            ))
        }
        // VeilidConfigProtectedStoreBackend::Callback => {
        //     apibail_generic!("callback protected store backend is not yet implemented");
        // }
        VeilidConfigProtectedStoreBackend::ExtensionStorage => {
            Ok(ProtectedStoreBackend::Extension(ExtensionBackend {
                prefix,
            }))
        }
    }
}
