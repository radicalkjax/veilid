use super::*;
use data_encoding::BASE64URL_NOPAD;
use zeroize::Zeroizing;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod wasm;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm::*;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod native;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use native::*;

/// Protected store keys that Veilid creates and manages, removed together by `delete_all`.
/// Hidden from and rejected for [ProtectedStoreContext::Api] callers.
pub static KNOWN_PROTECTED_STORE_KEYS: [&str; 2] = ["device_encryption_key", "_test_key"];

/// Whether a protected store operation comes from veilid-core itself or from an application.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProtectedStoreContext {
    /// An application, via the public API
    Api,
    /// veilid-core itself, with access to every key
    Internal,
}

fn is_reserved_key(key: &str) -> bool {
    KNOWN_PROTECTED_STORE_KEYS.contains(&key)
}

#[cfg(any(test, feature = "test-util"))]
#[doc(hidden)]
#[allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "test support: printing and raw clocks are the thing under test"
)]
pub mod tests_protected_store;

impl_veilid_log_facility!("pstore");

/// A request to the backend actor, carrying a reply channel for the result.
enum ProtectedStoreCommand {
    Get(
        String,
        flume::Sender<VeilidAPIResult<Option<Zeroizing<String>>>>,
    ),
    Set(
        String,
        Zeroizing<String>,
        flume::Sender<VeilidAPIResult<bool>>,
    ),
    Remove(String, flume::Sender<VeilidAPIResult<bool>>),
    List(flume::Sender<VeilidAPIResult<Vec<String>>>),
}

/// Mutable interior of the `ProtectedStore`, holding the channel to the backend actor.
struct ProtectedStoreInner {
    cmd_tx: Option<flume::Sender<ProtectedStoreCommand>>,
}
impl fmt::Debug for ProtectedStoreInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProtectedStoreInner").finish()
    }
}

/// Secure key-value storage for user secrets.
///
/// Every operation is async and runs on a single backend actor task, so a backend call that
/// blocks on user input (e.g. an OS keychain permission dialog) never stalls the async executor.
/// Secrets are held only for the duration of a call and zeroized when dropped.
#[derive(Debug)]
#[must_use]
pub struct ProtectedStore {
    registry: VeilidComponentRegistry,
    inner: Mutex<ProtectedStoreInner>,
}

impl_veilid_component!(ProtectedStore);

impl ProtectedStore {
    fn new_inner() -> ProtectedStoreInner {
        ProtectedStoreInner { cmd_tx: None }
    }

    pub(crate) fn new(registry: VeilidComponentRegistry) -> Self {
        Self {
            registry,
            inner: Mutex::new(Self::new_inner()),
        }
    }

    fn log_facilities_impl(&self) -> VeilidComponentLogFacilities {
        VeilidComponentLogFacilities::new().with_facility(
            VeilidComponentLogFacility::try_new_with_tags("pstore", ["#common"]).unwrap(),
        )
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip(self), err, fields(__VEILID_LOG_KEY = self.log_key())))]
    async fn init_async(&self) -> EyreResult<()> {
        let backend = build_backend(self).await.map_err(|e| eyre!("{}", e))?;

        let (cmd_tx, cmd_rx) = flume::unbounded();
        self.inner.lock().cmd_tx = Some(cmd_tx);

        cfg_if! {
            if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                spawn_detached_local("protected_store", run_backend_actor(backend, cmd_rx));
            } else {
                spawn_detached("protected_store", run_backend_actor(backend, cmd_rx));
            }
        }

        if self.config().protected_store.delete {
            self.delete_all().await;
        }

        Ok(())
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip(self), err, fields(__VEILID_LOG_KEY = self.log_key())))]
    #[allow(clippy::unused_async)]
    async fn post_init_async(&self) -> EyreResult<()> {
        Ok(())
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip(self), fields(__VEILID_LOG_KEY = self.log_key())))]
    #[allow(clippy::unused_async)]
    async fn pre_terminate_async(&self) {}

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip(self), fields(__VEILID_LOG_KEY = self.log_key())))]
    #[allow(clippy::unused_async)]
    async fn terminate_async(&self) {
        // Dropping the sender ends the detached actor, which drops the backend
        *self.inner.lock() = Self::new_inner();
    }

    fn command_sender(&self) -> VeilidAPIResult<flume::Sender<ProtectedStoreCommand>> {
        self.inner
            .lock()
            .cmd_tx
            .clone()
            .ok_or_else(VeilidAPIError::not_initialized)
    }

    async fn backend_get(&self, key: &str) -> VeilidAPIResult<Option<Zeroizing<String>>> {
        let (tx, rx) = flume::bounded(1);
        self.command_sender()?
            .send_async(ProtectedStoreCommand::Get(key.to_owned(), tx))
            .await
            .map_err(|_| VeilidAPIError::not_initialized())?;
        rx.recv_async()
            .await
            .map_err(|_| VeilidAPIError::not_initialized())?
    }

    async fn backend_set(&self, key: &str, value: Zeroizing<String>) -> VeilidAPIResult<bool> {
        let (tx, rx) = flume::bounded(1);
        self.command_sender()?
            .send_async(ProtectedStoreCommand::Set(key.to_owned(), value, tx))
            .await
            .map_err(|_| VeilidAPIError::not_initialized())?;
        rx.recv_async()
            .await
            .map_err(|_| VeilidAPIError::not_initialized())?
    }

    async fn backend_remove(&self, key: &str) -> VeilidAPIResult<bool> {
        let (tx, rx) = flume::bounded(1);
        self.command_sender()?
            .send_async(ProtectedStoreCommand::Remove(key.to_owned(), tx))
            .await
            .map_err(|_| VeilidAPIError::not_initialized())?;
        rx.recv_async()
            .await
            .map_err(|_| VeilidAPIError::not_initialized())?
    }

    async fn backend_list(&self) -> VeilidAPIResult<Vec<String>> {
        let (tx, rx) = flume::bounded(1);
        self.command_sender()?
            .send_async(ProtectedStoreCommand::List(tx))
            .await
            .map_err(|_| VeilidAPIError::not_initialized())?;
        rx.recv_async()
            .await
            .map_err(|_| VeilidAPIError::not_initialized())?
    }

    /// Remove every known Veilid-managed protected store key, logging any failures. Idempotent.
    pub async fn delete_all(&self) {
        for kpsk in &KNOWN_PROTECTED_STORE_KEYS {
            match self
                .remove_user_secret(ProtectedStoreContext::Internal, kpsk)
                .await
            {
                Ok(_) => veilid_log!(self debug "deleted protected store key '{}'", kpsk),
                Err(e) => veilid_log!(self error "failed to delete '{}': {}", kpsk, e),
            }
        }
    }

    /// Reject application access to keys veilid-core manages for itself.
    fn check_key_access(context: ProtectedStoreContext, key: &str) -> VeilidAPIResult<()> {
        if context == ProtectedStoreContext::Api && is_reserved_key(key) {
            apibail_invalid_argument!("protected store key is reserved by veilid", "key", key);
        }
        Ok(())
    }

    /// Store a byte buffer as a secret. Returns true if a value already existed for that key.
    pub async fn save_user_secret<K: AsRef<str>>(
        &self,
        context: ProtectedStoreContext,
        key: K,
        value: &[u8],
    ) -> VeilidAPIResult<bool> {
        Self::check_key_access(context, key.as_ref())?;
        let mut s = BASE64URL_NOPAD.encode(value);
        s.push('!');
        self.backend_set(key.as_ref(), Zeroizing::new(s)).await
    }

    /// Load a byte buffer secret by key, or `None` if no value is stored for it.
    ///
    /// `None` means the backend established that nothing is stored. A backend that could not read
    /// the store returns [VeilidAPIError::TryAgain] instead (an iOS keychain before first unlock),
    /// and the stored value is intact. A caller that reprovisions on `None` must propagate that
    /// error rather than fold it into absence, or it overwrites a secret it merely could not read.
    pub async fn load_user_secret<K: AsRef<str>>(
        &self,
        context: ProtectedStoreContext,
        key: K,
    ) -> VeilidAPIResult<Option<Vec<u8>>> {
        Self::check_key_access(context, key.as_ref())?;
        let Some(mut s) = self.backend_get(key.as_ref()).await? else {
            return Ok(None);
        };

        if s.pop() != Some('!') {
            apibail_generic!("User secret is not a buffer");
        }

        let Ok(len) = BASE64URL_NOPAD.decode_len(s.len()) else {
            apibail_generic!("Failed to decode");
        };
        let mut bytes = Zeroizing::new(vec![0u8; len]);
        match BASE64URL_NOPAD.decode_mut(s.as_bytes(), &mut bytes) {
            Ok(_) => Ok(Some((*bytes).clone())),
            Err(_) => apibail_generic!("Failed to decode"),
        }
    }

    /// Remove a secret by key. Returns true if a value was present and deleted. Idempotent.
    pub async fn remove_user_secret<K: AsRef<str>>(
        &self,
        context: ProtectedStoreContext,
        key: K,
    ) -> VeilidAPIResult<bool> {
        Self::check_key_access(context, key.as_ref())?;
        self.backend_remove(key.as_ref()).await
    }

    /// List the key names of the secrets stored for this store's namespace.
    ///
    /// [ProtectedStoreContext::Api] callers never see [KNOWN_PROTECTED_STORE_KEYS].
    ///
    /// Returns [VeilidAPIError::Generic] if the active backend does not support enumeration
    /// (some OS keychains).
    pub async fn list_user_secrets(
        &self,
        context: ProtectedStoreContext,
    ) -> VeilidAPIResult<Vec<String>> {
        let mut keys = self.backend_list().await?;
        if context == ProtectedStoreContext::Api {
            keys.retain(|k| !is_reserved_key(k));
        }
        Ok(keys)
    }

    /// Remove every secret stored for this namespace.
    ///
    /// Enumerates and removes all keys. An [ProtectedStoreContext::Internal] caller falls back to
    /// the known Veilid-managed keys when the backend cannot enumerate; an application caller gets
    /// the enumeration error, since without a listing there is no way to tell its keys from ours.
    pub async fn delete_all_user_secrets(
        &self,
        context: ProtectedStoreContext,
    ) -> VeilidAPIResult<()> {
        match self.list_user_secrets(context).await {
            Ok(keys) => {
                for key in keys {
                    self.remove_user_secret(context, &key).await?;
                }
                Ok(())
            }
            Err(e) => {
                if context == ProtectedStoreContext::Api {
                    return Err(e);
                }
                self.delete_all().await;
                Ok(())
            }
        }
    }
}

/// Serve backend commands until the command channel is closed (on component terminate).
async fn run_backend_actor(
    backend: ProtectedStoreBackend,
    cmd_rx: flume::Receiver<ProtectedStoreCommand>,
) {
    while let Ok(cmd) = cmd_rx.recv_async().await {
        match cmd {
            ProtectedStoreCommand::Get(key, reply) => {
                let _ = reply.send(backend.get(&key).await);
            }
            ProtectedStoreCommand::Set(key, value, reply) => {
                let _ = reply.send(backend.set(&key, &value).await);
            }
            ProtectedStoreCommand::Remove(key, reply) => {
                let _ = reply.send(backend.remove(&key).await);
            }
            ProtectedStoreCommand::List(reply) => {
                let _ = reply.send(backend.list().await);
            }
        }
    }
}
