use super::*;

impl_veilid_log_facility!("veilid_api");

/// What [VeilidAPI::wait_for_attachment_condition] waits for without a condition
const DEFAULT_ATTACHMENT_CONDITION: VeilidStateAttachmentCondition =
    VeilidStateAttachmentCondition {
        opt_min_state: Some(AttachmentState::AttachedWeak),
        opt_max_state: Some(AttachmentState::AttachedFull),
        opt_public_internet_ready: Some(true),
        opt_local_network_ready: None,
    };

/////////////////////////////////////////////////////////////////////////////////////////////////////

pub(super) struct VeilidAPIInner {
    context: Option<VeilidCoreContext>,
    #[cfg(all(feature = "debug-api", not(feature = "mock-api")))]
    pub(super) debug_cache: debug::DebugCache,
}

impl fmt::Debug for VeilidAPIInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VeilidAPIInner")
    }
}

impl Drop for VeilidAPIInner {
    fn drop(&mut self) {
        if let Some(context) = self.context.take() {
            #[cfg(feature = "mock-api")]
            MockNetwork::unregister(&context.registry());
            spawn_detached("api shutdown", api_shutdown(context));
        }
    }
}

/// The primary developer entrypoint into `veilid-core` functionality.
///
/// From [VeilidAPI] one can access various components:
///
/// * [VeilidConfig] - The Veilid configuration specified at startup time.
/// * [Crypto] - The available set of cryptosystems provided by Veilid.
/// * TableStore - The Veilid table-based encrypted persistent key-value store, via [VeilidAPI::open_table_db], [VeilidAPI::delete_table_db], etc.
/// * ProtectedStore - The Veilid abstract of the device's low-level 'protected secret storage', via [VeilidAPI::save_user_secret], [VeilidAPI::load_user_secret], etc.
/// * [VeilidState] - The current state of the Veilid node this API accesses.
/// * [RoutingContext] - Communication methods between Veilid nodes and private routes.
/// * Attach and detach from the network.
/// * Create and import private routes.
/// * Reply to `AppCall` RPCs.
#[derive(Clone, Debug)]
#[must_use]
pub struct VeilidAPI {
    inner: Arc<Mutex<VeilidAPIInner>>,
}

impl VeilidAPI {
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = context.log_key()), skip_all))]
    pub(crate) fn new(context: VeilidCoreContext) -> Self {
        veilid_log!(context debug "VeilidAPI::new()");

        // Join the process-global mock network, so other instances in this process can reach us
        #[cfg(feature = "mock-api")]
        {
            let registry = context.registry();
            let node_ids = generate_node_ids(&registry.crypto());
            MockNetwork::register(&registry, node_ids);
        }

        record_duration(|| Self {
            inner: Arc::new(Mutex::new(VeilidAPIInner {
                context: Some(context),
                #[cfg(all(feature = "debug-api", not(feature = "mock-api")))]
                debug_cache: Default::default(),
            })),
        })
    }

    /// Shut down Veilid and terminate the API.
    ///
    /// Blocks until the core context has finished shutting down. Idempotent: a second call after the
    /// context is already taken is a no-op.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip_all))]
    pub async fn shutdown(self) {
        let context = { self.inner.lock().context.take() };
        let recorder = DurationRecorder::new("VeilidAPI::shutdown", |name, start| {
            veilid_log!(self debug "{}[start={:#}]()", name, start);
        });
        recorder
            .record_fut(
                async {
                    if let Some(context) = context {
                        #[cfg(feature = "mock-api")]
                        MockNetwork::unregister(&context.registry());
                        api_shutdown(context).await;
                    }
                },
                |name, start, dur, ret| {
                    veilid_log!(self debug "{}[start={:#} dur={:#}](ret: ())", name, start, dur);
                    ret
                },
            )
            .await
    }

    /// Check to see if Veilid is already shut down.
    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.inner.lock().context.is_none()
    }

    ////////////////////////////////////////////////////////////////
    // Public Accessors

    /// Access the configuration that Veilid was initialized with.
    ///
    /// Errors with [VeilidAPIError::NotInitialized] if the API has been shut down.
    pub fn config(&self) -> VeilidAPIResult<Arc<VeilidConfig>> {
        let inner = self.inner.lock();
        let Some(context) = &inner.context else {
            return Err(VeilidAPIError::NotInitialized);
        };
        Ok(context.registry().config())
    }

    /// Get the cryptosystem component.
    ///
    /// Errors with [VeilidAPIError::NotInitialized] if the API has been shut down.
    pub fn crypto(&self) -> VeilidAPIResult<VeilidComponentGuard<'_, Crypto>> {
        let inner = self.inner.lock();
        let Some(context) = &inner.context else {
            return Err(VeilidAPIError::NotInitialized);
        };
        context
            .registry()
            .lookup::<Crypto>()
            .ok_or(VeilidAPIError::NotInitialized)
    }

    /// Get the TableStore component.
    ///
    /// Errors with [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg(not(feature = "mock-api"))]
    pub(crate) fn table_store(&self) -> VeilidAPIResult<VeilidComponentGuard<'_, TableStore>> {
        let inner = self.inner.lock();
        let Some(context) = &inner.context else {
            return Err(VeilidAPIError::NotInitialized);
        };
        context
            .registry()
            .lookup::<TableStore>()
            .ok_or(VeilidAPIError::NotInitialized)
    }

    /// Identify this instance's node in the process-global mock network.
    #[cfg(feature = "mock-api")]
    pub(crate) fn mock_key(&self) -> VeilidAPIResult<MockNodeKey> {
        let inner = self.inner.lock();
        let Some(context) = &inner.context else {
            return Err(VeilidAPIError::NotInitialized);
        };
        Ok(MockNodeKey::from_registry(&context.registry()))
    }

    /// Get or create a TableDB database table.
    ///
    /// Upgrades the table if `column_count` exceeds the existing column count. Returns a handle the
    /// caller must drop before the table can be deleted; while any clone is held the table counts
    /// as opened. Opening the same name again returns a handle to the same shared database.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] if `column_count` is zero or `name` contains
    /// characters other than alphanumeric, `_`, or `-`; [VeilidAPIError::Generic] if the table is
    /// already open with a smaller column count than requested or the backing-store open fails; and
    /// [VeilidAPIError::NotInitialized] if the API has been shut down or stored data fails to
    /// decrypt.
    pub async fn open_table_db(&self, name: &str, column_count: u32) -> VeilidAPIResult<TableDB> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "open_table_db({}, {})", name, column_count);
                MockNetwork::with_node(&self.mock_key()?, |node| node.tables.open(name, column_count))?
            } else {
                self.table_store()?.open(name, column_count).await
            }
        }
    }

    /// Delete a TableDB database table. Returns false if no table by that name exists.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] if `name` contains characters other than
    /// alphanumeric, `_`, or `-`; [VeilidAPIError::Generic] if the table is still open (drop all
    /// handles first) or the backing-store delete fails; and [VeilidAPIError::NotInitialized] if
    /// the API has been shut down.
    pub async fn delete_table_db(&self, name: &str) -> VeilidAPIResult<bool> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "delete_table_db({})", name);
                MockNetwork::with_node(&self.mock_key()?, |node| node.tables.delete(name))?
            } else {
                self.table_store()?.delete(name).await
            }
        }
    }

    /// Rename a TableDB database table.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] if `old_name` or `new_name` contains
    /// characters other than alphanumeric, `_`, or `-`; [VeilidAPIError::Generic] if `new_name`
    /// already exists or `old_name` does not exist; and [VeilidAPIError::NotInitialized] if the API
    /// has been shut down.
    pub async fn rename_table_db(&self, old_name: &str, new_name: &str) -> VeilidAPIResult<()> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "rename_table_db({}, {})", old_name, new_name);
                MockNetwork::with_node(&self.mock_key()?, |node| node.tables.rename(old_name, new_name))?
            } else {
                self.table_store()?.rename(old_name, new_name).await
            }
        }
    }

    /// List the TableDB tables stored for this node's namespace, as pairs of table name and
    /// backing-store name.
    ///
    /// Reads cached names without blocking on disk. Errors with [VeilidAPIError::NotInitialized]
    /// if the API has been shut down.
    pub fn list_table_dbs(&self) -> VeilidAPIResult<Vec<(String, String)>> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "list_table_dbs()");
                MockNetwork::with_node(&self.mock_key()?, |node| node.tables.list_all())
            } else {
                Ok(self.table_store()?.list_all())
            }
        }
    }

    /// Get the ProtectedStore component.
    ///
    /// Errors with [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg(not(feature = "mock-api"))]
    pub(crate) fn protected_store(
        &self,
    ) -> VeilidAPIResult<VeilidComponentGuard<'_, ProtectedStore>> {
        let inner = self.inner.lock();
        let Some(context) = &inner.context else {
            return Err(VeilidAPIError::NotInitialized);
        };
        context
            .registry()
            .lookup::<ProtectedStore>()
            .ok_or(VeilidAPIError::NotInitialized)
    }

    /// Save a user secret to the protected store. Returns true if a value already existed.
    ///
    /// Uses hardware-backed storage and system keychains (iOS Keychain, Android KeyStore, Windows
    /// Credential Manager) where the platform has them. On devices that have none,
    /// [VeilidConfigProtectedStore::allow_insecure_fallback] permits less secure storage such as
    /// owner-readable disk files.
    ///
    /// ProtectedStore secrets are size-limited per platform and slow to access. Unless the secret
    /// must survive uninstalling and reinstalling the app, prefer the `TableStore`, which is
    /// encrypted with the *device encryption key* that already lives in the `ProtectedStore`.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] for keys veilid reserves for itself.
    pub async fn save_user_secret(&self, key: String, value: Vec<u8>) -> VeilidAPIResult<bool> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "save_user_secret({})", key);
                MockNetwork::with_node(&self.mock_key()?, |node| node.secrets.save_user_secret(&key, &value))?
            } else {
                self.protected_store()?
                    .save_user_secret(ProtectedStoreContext::Api, key, &value)
                    .await
            }
        }
    }

    /// Load a user secret from the protected store, or `None` if not present.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] for keys veilid reserves for itself.
    pub async fn load_user_secret(&self, key: String) -> VeilidAPIResult<Option<Vec<u8>>> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "load_user_secret({})", key);
                MockNetwork::with_node(&self.mock_key()?, |node| node.secrets.load_user_secret(&key))?
            } else {
                self.protected_store()?
                    .load_user_secret(ProtectedStoreContext::Api, key)
                    .await
            }
        }
    }

    /// Remove a user secret from the protected store. Returns true if it existed.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] for keys veilid reserves for itself.
    pub async fn remove_user_secret(&self, key: String) -> VeilidAPIResult<bool> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "remove_user_secret({})", key);
                MockNetwork::with_node(&self.mock_key()?, |node| node.secrets.remove_user_secret(&key))?
            } else {
                self.protected_store()?
                    .remove_user_secret(ProtectedStoreContext::Api, key)
                    .await
            }
        }
    }

    /// List the key names of the user secrets stored for this node's namespace.
    ///
    /// Keys veilid reserves for itself are never listed.
    pub async fn list_user_secrets(&self) -> VeilidAPIResult<Vec<String>> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "list_user_secrets()");
                MockNetwork::with_node(&self.mock_key()?, |node| node.secrets.list_user_secrets())
            } else {
                self.protected_store()?
                    .list_user_secrets(ProtectedStoreContext::Api)
                    .await
            }
        }
    }

    /// Remove every user secret stored for this node's namespace, leaving veilid's own keys alone.
    pub async fn delete_all_user_secrets(&self) -> VeilidAPIResult<()> {
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                veilid_log!(self debug "delete_all_user_secrets()");
                MockNetwork::with_node(&self.mock_key()?, |node| node.secrets.delete_all_user_secrets())
            } else {
                self.protected_store()?
                    .delete_all_user_secrets(ProtectedStoreContext::Api)
                    .await
            }
        }
    }

    /// Get the BlockStore component.
    ///
    /// Errors with [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg(feature = "unstable-blockstore")]
    pub fn block_store(&self) -> VeilidAPIResult<VeilidComponentGuard<'_, BlockStore>> {
        let inner = self.inner.lock();
        let Some(context) = &inner.context else {
            return Err(VeilidAPIError::NotInitialized);
        };
        context
            .registry()
            .lookup::<BlockStore>()
            .ok_or(VeilidAPIError::NotInitialized)
    }

    ////////////////////////////////////////////////////////////////
    // Attach/Detach

    /// Get a full copy of the current state of Veilid.
    ///
    /// Errors with [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg_attr(not(feature = "mock-api"), expect(clippy::unused_async))]
    pub async fn get_state(&self) -> VeilidAPIResult<VeilidState> {
        let config = self.config()?;

        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                let (attachment, network) = MockNetwork::with_node(&self.mock_key()?, |node| {
                    let attachment = node.attachment.to_state();
                    let network = VeilidStateNetwork {
                        started: attachment.state.is_attached(),
                        bps_down: ByteCount::new(0),
                        bps_up: ByteCount::new(0),
                        peers: Vec::new(),
                        node_ids: node.node_ids.clone(),
                    };
                    (Box::new(attachment), Box::new(network))
                })?;
            } else {
                let attachment_manager = self.core_context()?.attachment_manager();
                let network_manager = attachment_manager.network_manager();

                let attachment = attachment_manager.get_veilid_state();
                let network = network_manager.get_veilid_state();
            }
        }

        Ok(VeilidState {
            attachment,
            network,
            config: Box::new(VeilidStateConfig {
                config: config.as_ref().clone(),
            }),
        })
    }

    /// Connect to the network.
    ///
    /// Moves the attachment to [AttachmentState::Attaching], where it stays, waiting to bind if it must,
    /// until the node attaches or [VeilidAPI::detach] is called; the network connect proceeds in the
    /// background tick loop. Succeeds while already attaching.
    ///
    /// Errors with [VeilidAPIError::Generic] if already attached, or [VeilidAPIError::NotInitialized]
    /// if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip_all, ret))]
    pub async fn attach(&self) -> VeilidAPIResult<()> {
        async {
            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    veilid_log!(self debug "attach()");
                    let key = self.mock_key()?;
                    // Reports fully ready at once: there is no network to wait for
                    let (changed, state) = MockNetwork::with_node(&key, |node| {
                        (node.attachment.attach(), node.attachment.to_state())
                    })?;
                    if !changed {
                        apibail_generic!("Already attached");
                    }
                    MockNetwork::send_update(&key, VeilidUpdate::Attachment(Box::new(state)))?;
                    // Coming back online is what drains the offline write queue on a real node too
                    self.mock_drain_all_offline_writes()?;
                    VeilidAPIResult::Ok(())
                } else {
                    let attachment_manager = self.core_context()?.attachment_manager();
                    let recorder = DurationRecorder::new("VeilidAPI::attach", |name, start| {
                        veilid_log!(self debug "{}[start={:#}]()", name, start);
                    });
                    recorder.record_fut(
                        Box::pin(attachment_manager.attach()),
                        |name, start, dur, ret| {
                            veilid_log!(self debug "{}[start={:#} dur={:#}](ret: {:?})", name, start, dur, ret);
                            ret
                        },
                    ).await
                }
            }
        }
        .await
        .inspect_err(log_veilid_api_error!(self))
    }

    /// Disconnect from the network.
    ///
    /// Moves the attachment to [AttachmentState::Detaching], cancelling an attach in progress; the
    /// network teardown proceeds in the background tick loop. Succeeds while already detaching.
    ///
    /// Errors with [VeilidAPIError::Generic] if already detached, or [VeilidAPIError::NotInitialized]
    /// if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip_all, ret))]
    pub async fn detach(&self) -> VeilidAPIResult<()> {
        async {
            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    veilid_log!(self debug "detach()");
                    let key = self.mock_key()?;
                    let (changed, state) = MockNetwork::with_node(&key, |node| {
                        (node.attachment.detach(), node.attachment.to_state())
                    })?;
                    if !changed {
                        apibail_generic!("Not attached");
                    }
                    MockNetwork::send_update(&key, VeilidUpdate::Attachment(Box::new(state)))?;
                    VeilidAPIResult::Ok(())
                } else {
                    let attachment_manager = self.core_context()?.attachment_manager();
                    let recorder = DurationRecorder::new("VeilidAPI::detach", |name, start| {
                        veilid_log!(self debug "{}[start={:#}]()", name, start);
                    });
                    recorder.record_fut(
                        Box::pin(attachment_manager.detach()),
                        |name, start, dur, ret| {
                            veilid_log!(self debug "{}[start={:#} dur={:#}](ret: {:?})", name, start, dur, ret);
                            ret
                        },
                    ).await
                }
            }
        }
        .await
        .inspect_err(log_veilid_api_error!(self))
    }

    /// Replace the configuration of a detached node; the new values apply at the next
    /// [VeilidAPI::attach]. The keys a running node cannot take a new value for must be
    /// unchanged, see [VeilidConfig::ensure_changeable_from]. A config as [VeilidAPI::get_state]
    /// reports it, with its secrets blanked, keeps the current secrets.
    ///
    /// Errors with [VeilidAPIError::Generic] unless the node is detached, with
    /// [VeilidAPIError::InvalidArgument] for a key that needs a restart or a value that fails
    /// validation, or [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(__VEILID_LOG_KEY = self.log_key()), skip_all, ret))]
    pub fn change_config(&self, config: VeilidConfig) -> VeilidAPIResult<()> {
        let res = (|| {
            veilid_log!(self debug "change_config()");
            let context = self.core_context()?;
            let detached = {
                cfg_if! {
                    if #[cfg(feature = "mock-api")] {
                        MockNetwork::with_node(&self.mock_key()?, |node| {
                            node.attachment.state.is_detached()
                        })?
                    } else {
                        context.attachment_manager().is_detached()
                    }
                }
            };
            if !detached {
                apibail_generic!("the node must be detached to change its config");
            }
            config.validate()?;
            let registry = context.registry();
            let current = registry.config();
            let mut config = config;
            config.fill_secrets_from(&current);
            config.ensure_changeable_from(&current)?;
            let safe = (*config.safe()).clone();
            registry.set_config(config);
            (registry.update_callback())(VeilidUpdate::Config(Box::new(VeilidStateConfig {
                config: safe,
            })));
            VeilidAPIResult::Ok(())
        })();
        res.inspect_err(log_veilid_api_error!(self))
    }

    /// Wait until the attachment state satisfies a condition.
    ///
    /// Returns the first [VeilidStateAttachment] that satisfies `condition`, at once if the current state does.
    /// A field of `condition` left `None` is not checked. Without a `condition`, waits until the node is
    /// attached, at any strength, with PublicInternet ready.
    /// When a `timeout` is specified, errors with [VeilidAPIError::Timeout] once it elapses; without one, waits indefinitely.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] if the condition's least state is above its most,
    /// or [VeilidAPIError::NotInitialized] if the API shuts down while waiting.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip_all, ret))]
    pub async fn wait_for_attachment_condition(
        &self,
        condition: Option<VeilidStateAttachmentCondition>,
        timeout: Option<Duration>,
    ) -> VeilidAPIResult<VeilidStateAttachment> {
        async {
            let condition = condition.unwrap_or(DEFAULT_ATTACHMENT_CONDITION);
            if let (Some(min), Some(max)) = (condition.opt_min_state, condition.opt_max_state) {
                if min > max {
                    apibail_invalid_argument!(
                        "wait_for_attachment_condition",
                        "condition",
                        format!("min {} above max {}", min, max)
                    );
                }
            }
            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    veilid_log!(self debug "wait_for_attachment_condition(condition: {:?}, timeout: {:?})", condition, timeout);
                    self.mock_wait_for_attachment_condition(condition, timeout).await
                } else {
                    let attachment_manager = self.core_context()?.attachment_manager();
                    let recorder = DurationRecorder::new("VeilidAPI::wait_for_attachment_condition", |name, start| {
                        veilid_log!(self debug "{}[start={:#}](condition: {:?}, timeout: {:?})", name, start, condition, timeout);
                    });
                    recorder.record_fut(
                        Box::pin(attachment_manager.wait_for_condition(condition, timeout)),
                        |name, start, dur, ret| {
                            veilid_log!(self debug "{}[start={:#} dur={:#}](ret: {:?})", name, start, dur, ret);
                            ret
                        },
                    ).await
                }
            }
        }
        .await
        .inspect_err(log_veilid_api_error!(self))
    }

    /// Report where the host application stands in its lifecycle.
    ///
    /// The node follows this itself in an iOS, Android or macOS app, a web page and a service
    /// worker, and a report counts there only when `attachment.app_lifecycle.auto_detect_state`
    /// is off. Elsewhere, as in a dedicated or shared worker or a Linux or Windows app, the host's
    /// reports are all it has.
    ///
    /// Errors with [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg(feature = "app-lifecycle")]
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(__VEILID_LOG_KEY = self.log_key()), skip_all, ret))]
    pub fn report_app_lifecycle_state(
        &self,
        state: VeilidAppLifecycleState,
    ) -> VeilidAPIResult<()> {
        veilid_log!(self debug "VeilidAPI::report_app_lifecycle_state(state: {:?})", state);
        cfg_if! {
            if #[cfg(feature = "mock-api")] {
                // The mock makes no saves for a lifecycle to drive
                self.mock_key()?;
            } else {
                self.core_context()?
                    .attachment_manager()
                    .report_app_lifecycle_state(state);
            }
        }
        Ok(())
    }

    ////////////////////////////////////////////////////////////////
    // Routing Context

    /// Get a new `RoutingContext` object to use to send messages over the Veilid network with default safety, sequencing, and stability parameters.
    ///
    /// Errors with [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip_all, ret))]
    pub fn routing_context(&self) -> VeilidAPIResult<RoutingContext> {
        record_duration(|| {
            veilid_log!(self debug "VeilidAPI::routing_context()");

            RoutingContext::try_new(self.clone())
        })
        .inspect_err(log_veilid_api_error!(self))
    }

    ////////////////////////////////////////////////////////////////
    // Non-RoutingContext DHT Operations

    /// Deterministicly builds the record key for a given schema and owner public key.
    /// The crypto kind of the record key will be that of the `owner` public key
    ///
    /// Local crypto computation only; despite being `async` it makes no network round-trip.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] if `schema` is malformed, [VeilidAPIError::Generic]
    /// if `owner_key` or `encryption_key` names an unsupported crypto kind, or
    /// [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), ret))]
    pub async fn get_dht_record_key(
        &self,
        schema: DHTSchema,
        owner_key: PublicKey,
        encryption_key: Option<SharedSecret>,
    ) -> VeilidAPIResult<RecordKey> {
        async {
            schema.validate()?;
            self.crypto()?.check_public_key(&owner_key)?;
            if let Some(encryption_key) = encryption_key.as_ref() {
                self.crypto()?.check_shared_secret(encryption_key)?;
            }
            let recorder = DurationRecorder::new("VeilidAPI::get_dht_record_key", |name, start| {
                veilid_log!(self debug
                    "{}[start={:#}](self: {:?}, schema: {:?}, owner_key: {:?}, encryption_key: {:?})", name, start, self, schema, owner_key, encryption_key);
            });
            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    // Shares the real derivation, so a key made here matches one an app derives
                    let crypto = self.crypto()?;
                    let fut = dht_record_key(&crypto, schema, &owner_key, encryption_key);
                } else {
                    let storage_manager = self.core_context()?.storage_manager();
                    let fut = storage_manager.get_record_key(schema, &owner_key, encryption_key);
                }
            }
            recorder.record_fut(
                fut,
                |name, start, dur, ret| {
                    veilid_log!(self debug
                        "{}[start={:#} dur={:#}](ret: {:?})", name, start, dur, ret);
                    ret
                },
            ).await
        }.await.inspect_err(log_veilid_api_error!(self))
    }

    /// Create a new MemberId for use with in creating `DHTSchema`s.
    ///
    /// Errors with [VeilidAPIError::Generic] if `writer_key` names an unsupported crypto kind, or
    /// [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", skip(self), fields(duration, __VEILID_LOG_KEY = self.log_key()), ret))]
    pub fn generate_member_id(&self, writer_key: &PublicKey) -> VeilidAPIResult<MemberId> {
        record_duration(move || {
            veilid_log!(self debug "VeilidAPI::generate_member_id(writer_key: {:?}", writer_key);

            self.crypto()?.check_public_key(writer_key)?;

            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    let crypto = self.crypto()?;
                    generate_member_id(&crypto, writer_key)
                } else {
                    let storage_manager = self.core_context()?.storage_manager();
                    storage_manager.generate_member_id(writer_key)
                }
            }
        })
        .inspect_err(log_veilid_api_error!(self))
    }

    /// Start a transaction on a set of DHT records
    /// Record keys must have been opened via a routing context already when passed to this function
    /// The maximum number of records per transaction is currently 32.
    /// Options can be specified that supply a default signing keypair for records that are not opened for writing
    ///
    /// Blocks on the network to begin the transaction across the record nodes (online-only).
    /// The returned [DHTTransaction] holds a network-side resource the caller must release by calling
    /// [DHTTransaction::commit] or [DHTTransaction::rollback]; dropping it without doing either logs a
    /// warning and tears the transaction down in the background.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] if a record is not open or more than 32 records
    /// are passed, [VeilidAPIError::MissingArgument] if `record_keys` is empty or has duplicates,
    /// [VeilidAPIError::Generic] if a record key is malformed or its encryption key does not match the
    /// opened record, [VeilidAPIError::TryAgain] if the DHT is offline, the records are contended, or
    /// begin consensus was not reached (retry), [VeilidAPIError::NotInitialized] if the API has been
    /// shut down. Network failures surface as [VeilidAPIError::Timeout] or
    /// [VeilidAPIError::NoConnection].
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), ret))]
    pub async fn transact_dht_records(
        &self,
        record_keys: Vec<RecordKey>,
        options: Option<TransactDHTRecordsOptions>,
    ) -> VeilidAPIResult<DHTTransaction> {
        async {
            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    let handle = self.mock_begin_transaction(record_keys, options)?;
                    DHTTransaction::try_new(self.clone(), handle)
                } else {
                    let storage_manager = self.core_context()?.storage_manager();
                    for record_key in &record_keys {
                        storage_manager.check_record_key(record_key)?;
                    }

                    let recorder = DurationRecorder::new("VeilidAPI::transact_dht_records", |name, start| {
                        veilid_log!(self debug
                            "{}[start={:#}](self: {:?}, record_keys: {:?}, options: {:?})", name, start, self, record_keys, options);
                    });
                    recorder.record_fut(
                        async {
                            let handle = Box::pin(storage_manager.begin_transaction(record_keys, options)).await?;
                            DHTTransaction::try_new(self.clone(), handle)
                        },
                        |name, start, dur, ret| {
                            veilid_log!(self debug
                                "{}[start={:#} dur={:#}](ret: {:?})", name, start, dur, ret);
                            ret
                        },
                    ).await
                }
            }
        }.await.inspect_err(log_veilid_api_error!(self))
    }

    ////////////////////////////////////////////////////////////////
    // Private route allocation

    /// Allocate a new private route set with default cryptography and network options.
    /// Default settings are for [Stability::Reliable] and [Sequencing::PreferOrdered].
    /// Returns a route id and a publishable 'blob' with the route encrypted with each crypto kind.
    /// Those nodes importing the blob will have their choice of which crypto kind to use.
    ///
    /// Returns a route id and 'blob' that can be published over some means (DHT or otherwise) to be
    /// imported by another Veilid node.
    ///
    /// Blocks on the network to allocate and test the route. The returned route id holds an allocated
    /// route the caller must free with [VeilidAPI::release_private_route].
    ///
    /// Errors with [VeilidAPIError::TryAgain] if there is no valid PublicInternet network class yet,
    /// not enough nodes are known to build the route, or the route failed its reachability test
    /// (retry), or [VeilidAPIError::NotInitialized] if the API has been shut down.
    pub async fn new_private_route(&self) -> VeilidAPIResult<RouteBlob> {
        Box::pin(self.new_custom_private_route(PrivateSpec::default())).await
    }

    /// Allocate a new private route and specify a specific cryptosystem, stability and sequencing preference.
    /// Faster connections may be possible with [Stability::LowLatency], and [Sequencing::PreferUnordered] at the
    /// expense of some loss of messages.
    /// Returns a route id and a publishable 'blob' with the route encrypted with each crypto kind.
    /// Those nodes importing the blob will have their choice of which crypto kind to use.
    ///
    /// Returns a route id and 'blob' that can be published over some means (DHT or otherwise) to be
    /// imported by another Veilid node.
    ///
    /// Blocks on the network to allocate and test the route. The returned route id holds an allocated
    /// route the caller must free with [VeilidAPI::release_private_route].
    ///
    /// Errors with [VeilidAPIError::Generic] if `private_spec` names an invalid crypto kind,
    /// [VeilidAPIError::InvalidArgument] if the hop count exceeds the configured maximum,
    /// [VeilidAPIError::TryAgain] if there is no valid PublicInternet network class yet, not enough
    /// nodes are known to build the route, or the route failed its reachability test (retry), or
    /// [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip(self), ret))]
    pub async fn new_custom_private_route(
        &self,
        mut private_spec: PrivateSpec,
    ) -> VeilidAPIResult<RouteBlob> {
        async {
            let default_route_hop_count: usize =
                self.config()?.network.rpc.default_route_hop_count.into();

            if private_spec.crypto_kinds.is_empty() {
                private_spec.crypto_kinds = VALID_CRYPTO_KINDS.to_vec();
            } else {
                for kind in &private_spec.crypto_kinds {
                    Crypto::validate_crypto_kind(*kind)?;
                }
            }
            if private_spec.hop_count == 0 {
                private_spec.hop_count = default_route_hop_count;
            }

            #[cfg(feature = "mock-api")]
            return self.mock_new_private_route().await;

            #[cfg(not(feature = "mock-api"))]
            {
            let routing_table = self.core_context()?.routing_table();
            let rss = routing_table.route_spec_store();

            let recorder = DurationRecorder::new("VeilidAPI::new_custom_private_route", |name, start| {
                veilid_log!(self debug "{}[start={:#}](private_spec: {:?})", name, start, private_spec);
            });
            recorder.record_fut(
                async {
                    let allocate_route_params = AllocateRouteParams {
                        crypto_kinds: private_spec.crypto_kinds,
                        hop_count: private_spec.hop_count,
                        stability: private_spec.stability,
                        sequencing: private_spec.sequencing,
                        directions: DirectionSet::all(),
                        avoid_nodes: Vec::new(),
                        automatic: false,
                    };
                    let RouteIdAndKeys {
                        route_id,
                        route_set_keys: _,
                    } = rss.allocate_route(allocate_route_params).await?;
                    let route_id_api: RouteId = route_id.clone().into();
                    match Box::pin(rss.test_route(route_id_api.clone())).await? {
                        Some(true) => {}
                        Some(false) => {
                            rss.release_route(route_id_api.clone());
                            apibail_try_again!("allocated route failed to test");
                        }
                        None => {
                            rss.release_route(route_id_api.clone());
                            apibail_try_again!("allocated route could not be tested");
                        }
                    }
                    let private_routes = rss.assemble_private_route_set(&route_id, Some(true)).await?;
                    let blob = match RouteSpecStore::private_routes_to_blob(&private_routes) {
                        Ok(v) => v,
                        Err(e) => {
                            rss.release_route(route_id_api);
                            return Err(e);
                        }
                    };
                    rss.mark_route_published(&route_id, true)?;
                    VeilidAPIResult::Ok(RouteBlob {
                        route_id: route_id_api,
                        blob: blob.into(),
                    })
                },
                |name, start, dur, ret| {
                    veilid_log!(self debug "{}[start={:#} dur={:#}](ret: {:?})", name, start, dur, ret);
                    ret
                },
            ).await
            }
        }
        .await
        .inspect_err(log_veilid_api_error!(self))
    }

    /// Import a private route blob as a remote private route.
    ///
    /// Returns a route id that can be used to send private messages to the node creating this route.
    ///
    /// Local import, no network round-trip. The returned route id holds an imported route the caller
    /// must free with [VeilidAPI::release_private_route].
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] if `blob` is empty or names too many crypto kinds,
    /// [VeilidAPIError::ParseError] if it is malformed, [VeilidAPIError::Generic] if the decoded route
    /// has no first hop, or [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip(self), ret))]
    pub fn import_remote_private_route(&self, blob: Vec<u8>) -> VeilidAPIResult<RouteId> {
        record_duration(|| {
            veilid_log!(self debug
                "VeilidAPI::import_remote_private_route(blob: {:?})", blob);
            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    self.mock_import_remote_private_route(blob)
                } else {
                    let routing_table = self.core_context()?.routing_table();
                    let rss = routing_table.route_spec_store();
                    rss.import_remote_route_blob(blob)
                }
            }
        })
        .inspect_err(log_veilid_api_error!(self))
    }

    /// Release either a locally allocated or remotely imported private route.
    ///
    /// This will deactivate the route and free its resources and it can no longer be sent to
    /// or received from.
    ///
    /// This is the release for [VeilidAPI::new_private_route], [VeilidAPI::new_custom_private_route], and
    /// [VeilidAPI::import_remote_private_route]. Local, no network round-trip. Releasing a route id that
    /// is unknown, already released, or malformed (unsupported crypto kind or bad length) returns
    /// [VeilidAPIError::InvalidArgument]; errors with [VeilidAPIError::NotInitialized] if the API has
    /// been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip(self), ret))]
    pub fn release_private_route(&self, route_id: RouteId) -> VeilidAPIResult<()> {
        record_duration(|| {
            veilid_log!(self debug
                "VeilidAPI::release_private_route(route_id: {:?})", route_id);

            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    self.mock_release_private_route(route_id)
                } else {
                    let routing_table = self.core_context()?.routing_table();
                    routing_table.check_route_id(&route_id)?;

                    let rss = routing_table.route_spec_store();
                    if !rss.release_route(route_id.clone()) {
                        apibail_invalid_argument!("release_private_route", "key", route_id);
                    }
                    Ok(())
                }
            }
        })
        .inspect_err(log_veilid_api_error!(self))
    }

    ////////////////////////////////////////////////////////////////
    // App Calls

    /// Respond to an AppCall received over a [VeilidUpdate::AppCall].
    ///
    /// * `call_id` - specifies which call to reply to, and it comes from a [VeilidUpdate::AppCall], specifically the [VeilidAppCall::id()] value.
    /// * `message` - is an answer blob to be returned by the remote node's [RoutingContext::app_call()] function, and may be up to 32768 bytes.
    ///
    /// Completes the pending call locally and does not block on the network. Each `call_id` may be
    /// answered only once; replying to an unknown or already-answered `call_id` errors with
    /// [VeilidAPIError::Generic]. Errors with [VeilidAPIError::TryAgain] if the node is mid-shutdown,
    /// or [VeilidAPIError::NotInitialized] if the API has been shut down.
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip(self), ret))]
    pub async fn app_call_reply(
        &self,
        call_id: OperationId,
        message: Vec<u8>,
    ) -> VeilidAPIResult<()> {
        async {
            cfg_if! {
                if #[cfg(feature = "mock-api")] {
                    self.mock_app_call_reply(call_id, message)
                } else {
                    let rpc_processor = self.core_context()?.rpc_processor();

                    let message_len = message.len();
                    let recorder = DurationRecorder::new("VeilidAPI::app_call_reply", |name, start| {
                        veilid_log!(self debug
                            "{}[start={:#}](call_id: {:?}, message_len: {})", name, start, call_id, message_len);
                        veilid_log!(self trace "message: {:?}", message);
                    });
                    recorder.record(
                        || rpc_processor
                            .app_call_reply(call_id, message.into())
                            .map_err(|e| e.into()),
                        |name, start, dur, ret| {
                            veilid_log!(self debug
                                "{}[start={:#} dur={:#}](ret: {:?})", name, start, dur, ret);
                            ret
                        },
                    )
                }
            }
        }.await.inspect_err(log_veilid_api_error!(self))
    }

    ////////////////////////////////////////////////////////////////
    // Tunnel Building

    #[cfg(feature = "unstable-tunnels")]
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip(self), ret))]
    pub async fn start_tunnel(
        &self,
        _endpoint_mode: TunnelMode,
        _depth: u8,
    ) -> VeilidAPIResult<PartialTunnel> {
        apibail_internal!("unimplemented");
    }

    #[cfg(feature = "unstable-tunnels")]
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip(self), ret))]
    pub async fn complete_tunnel(
        &self,
        _endpoint_mode: TunnelMode,
        _depth: u8,
        _partial_tunnel: PartialTunnel,
    ) -> VeilidAPIResult<FullTunnel> {
        apibail_internal!("unimplemented");
    }

    #[cfg(feature = "unstable-tunnels")]
    #[cfg_attr(feature = "instrument", instrument(target = "veilid_api", level = "debug", fields(duration, __VEILID_LOG_KEY = self.log_key()), skip(self), ret))]
    pub async fn cancel_tunnel(&self, _tunnel_id: TunnelId) -> VeilidAPIResult<bool> {
        apibail_internal!("unimplemented");
    }

    ////////////////////////////////////////////////////////////////
    // Internal Accessors

    pub(crate) fn core_context(&self) -> VeilidAPIResult<VeilidCoreContext> {
        let inner = self.inner.lock();
        let Some(context) = &inner.context else {
            return Err(VeilidAPIError::NotInitialized);
        };
        Ok(context.clone())
    }

    #[cfg(all(feature = "debug-api", not(feature = "mock-api")))]
    pub(crate) fn with_debug_cache<R, F: FnOnce(&mut debug::DebugCache) -> R>(
        &self,
        callback: F,
    ) -> R {
        let mut inner = self.inner.lock();
        callback(&mut inner.debug_cache)
    }

    #[must_use]
    pub(crate) fn log_key(&self) -> &str {
        let inner = self.inner.lock();
        let Some(context) = &inner.context else {
            return "";
        };
        context.log_key()
    }
}
