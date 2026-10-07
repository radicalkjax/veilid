#![allow(non_snake_case)]
use super::*;
use crate::ts::try_from_js_option;
use veilid_crypto_js::VeilidCrypto;

#[wasm_bindgen(typescript_custom_section)]
const IUPDATE_VEILID_FUNCTION: &'static str = r#"
export type UpdateVeilidFunction = (event: VeilidUpdate) => void;
"#;

#[wasm_bindgen]
extern "C" {
    /// JS callback invoked for each `VeilidUpdate` produced by the node.
    #[wasm_bindgen(extends = Function, typescript_type = "UpdateVeilidFunction")]
    pub type UpdateVeilidFunction;

    /// Log a message to the JS `console.log`.
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn console_log(s: &str);
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Namespace of node-level Veilid operations: startup, shutdown, state, crypto, routes, and config.
#[wasm_bindgen(js_name = veilidClient)]
pub struct VeilidClient {}

// Since this implementation doesn't contain a `new` fn that's marked as a constructor,
// and none of the member fns take a &self arg,
// this is just a namespace/class of static functions.
#[wasm_bindgen(js_class = veilidClient)]
impl VeilidClient {
    // --------------------------------
    // Constants
    // (written as getters since wasm_bindgen doesn't support export of const)
    // --------------------------------

    /// The ROUTE capability
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability")]
    #[must_use]
    pub fn VEILID_CAPABILITY_ROUTE() -> JsValue {
        crate::VEILID_CAPABILITY_ROUTE.into()
    }

    /// The TUNL capability
    #[cfg(feature = "unstable-tunnels")]
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability")]
    #[must_use]
    pub fn VEILID_CAPABILITY_TUNNEL() -> JsValue {
        crate::VEILID_CAPABILITY_TUNNEL.into()
    }

    /// The SGNL capability
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability")]
    #[must_use]
    pub fn VEILID_CAPABILITY_SIGNAL() -> JsValue {
        crate::VEILID_CAPABILITY_SIGNAL.into()
    }

    /// The RLAY capability
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability")]
    #[must_use]
    pub fn VEILID_CAPABILITY_RELAY() -> JsValue {
        crate::VEILID_CAPABILITY_RELAY.into()
    }

    /// The DIAL capability
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability")]
    #[must_use]
    pub fn VEILID_CAPABILITY_VALIDATE_DIAL_INFO() -> JsValue {
        crate::VEILID_CAPABILITY_VALIDATE_DIAL_INFO.into()
    }

    /// The DHTV capability
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability")]
    #[must_use]
    pub fn VEILID_CAPABILITY_DHT() -> JsValue {
        crate::VEILID_CAPABILITY_DHT.into()
    }

    /// The APPM capability
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability")]
    #[must_use]
    pub fn VEILID_CAPABILITY_APPMESSAGE() -> JsValue {
        crate::VEILID_CAPABILITY_APPMESSAGE.into()
    }

    /// The BLOC capability
    #[cfg(feature = "unstable-blockstore")]
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability")]
    #[must_use]
    pub fn VEILID_CAPABILITY_BLOCKSTORE() -> JsValue {
        crate::VEILID_CAPABILITY_BLOCKSTORE.into()
    }

    /// All distance metric capabilites of this version of Veilid
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability[]")]
    #[must_use]
    pub fn DISTANCE_METRIC_CAPABILITIES() -> JsValue {
        js_sys::Array::from_iter(
            crate::DISTANCE_METRIC_CAPABILITIES
                .iter()
                .map(|x| JsValue::from(x.to_string())),
        )
        .into()
    }

    /// All connectivity capabilites of this version of Veilid
    #[wasm_bindgen(getter, unchecked_return_type = "VeilidCapability[]")]
    #[must_use]
    pub fn CONNECTIVITY_CAPABILITIES() -> JsValue {
        js_sys::Array::from_iter(
            crate::CONNECTIVITY_CAPABILITIES
                .iter()
                .map(|x| JsValue::from(x.to_string())),
        )
        .into()
    }

    ///////////////////////////////////////////////////////////////////////////////////////////

    /// Initialize the WASM platform: panic hook and tracing layers from the platform config.
    ///
    /// Idempotent; only the first call has any effect. Must run before `startupCore`.
    #[allow(clippy::unused_async)]
    pub async fn initializeCore(platformConfig: VeilidWASMConfig) {
        if INITIALIZED.swap(true, Ordering::AcqRel) {
            return;
        }
        console_error_panic_hook::set_once();

        // Set up subscriber and layers
        let subscriber = Registry::default();
        let mut layers = Vec::new();
        let mut filters = (*FILTERS).borrow_mut();

        // Performance logger
        if platformConfig.logging.performance.enabled {
            let filter = VeilidLayerFilter::new_with_config(
                VeilidLayerFilterConfig::new()
                    .with_common_log_level(platformConfig.logging.performance.level),
            );
            #[allow(deprecated)]
            filter.apply_ignore_change_list(&platformConfig.logging.performance.ignore_log_targets);
            filter
                .try_apply_directives(platformConfig.logging.performance.directives)
                .expect_or_log("failed to apply performance logging directives");

            let mut config = WasmLayerConfig::new();
            if !platformConfig.logging.performance.timings {
                config = config.remove_timings();
            }
            if !platformConfig.logging.performance.console.enabled {
                config = config.remove_console();
            }
            if !platformConfig.logging.performance.console.color {
                config = config.remove_color();
            }
            if !platformConfig.logging.performance.console.timestamp {
                config = config.remove_timestamp();
            }

            let layer = WasmLayer::new(config)
                .with_field_filter(Some(Arc::new(|k| k != VEILID_LOG_KEY_FIELD)))
                .with_filter(filter.clone());
            filters.insert("performance", filter);
            layers.push(layer.boxed());
        };

        // API logger
        if platformConfig.logging.api.enabled {
            let filter = VeilidLayerFilter::new_with_config(
                VeilidLayerFilterConfig::new()
                    .with_common_log_level(platformConfig.logging.api.level),
            );
            #[allow(deprecated)]
            filter.apply_ignore_change_list(&platformConfig.logging.api.ignore_log_targets);
            filter
                .try_apply_directives(platformConfig.logging.api.directives)
                .expect_or_log("failed to apply performance logging directives");

            let layer = ApiTracingLayer::init().with_filter(filter.clone());
            filters.insert("api", filter);
            layers.push(layer.boxed());
        }

        let subscriber = subscriber.with(layers);
        subscriber
            .try_init()
            .map_err(|e| format!("failed to initialize logging: {}", e))
            .expect_or_log("failed to initalize WASM platform");
    }

    /// Initialize a Veilid node, with the configuration in JSON format
    ///
    /// Must be called only once at the start of an application
    ///
    /// @param {UpdateVeilidFunction} updateCallbackJS - called when internal state of the Veilid node changes, for example, when app-level messages are received, when private routes die and need to be reallocated, or when routing table states change
    /// @param {VeilidConfig} config - the configuration object to use for the instance
    ///
    /// The open half of the startup/shutdown pair; release with [VeilidClient::shutdownCore]. Errors with `AlreadyInitialized` if a node is already running. Blocks bringing the core up.
    ///
    /// Throws `AlreadyInitialized` if a node is already running, otherwise the error from core startup (`Generic` on a bad config or a failure bringing the core up).
    pub async fn startupCore(
        updateCallbackJS: UpdateVeilidFunction,
        config: VeilidConfig,
    ) -> VeilidAPIResult<()> {
        let update_callback_js = SendWrapper::new(updateCallbackJS);
        let update_callback = Arc::new(move |update: VeilidUpdate| {
            let _ret = match Function::call1(
                &update_callback_js,
                &JsValue::UNDEFINED,
                &JsValue::from(update),
            ) {
                Ok(v) => v,
                Err(e) => {
                    console_log(&format!("calling update callback failed: {:?}", e));
                    return;
                }
            };
        });

        if VEILID_API.borrow().is_some() {
            return VeilidAPIResult::Err(VeilidAPIError::AlreadyInitialized);
        }

        let veilid_api = api_startup(update_callback, config).await?;
        VEILID_API.replace(Some(veilid_api));
        Ok(())
    }

    /// Apply tracing directives to a logging layer by name, or to all layers when `layer` is "all".
    ///
    /// Throws a parse error if `directive` is not a valid log directive string. An unknown `layer` name is a no-op, not an error.
    // TODO: can we refine the TS type of `layer`?
    pub fn changeLogLevel(layer: String, directive: String) -> VeilidAPIResult<()> {
        let layer = if layer == "all" { "".to_owned() } else { layer };
        let filters = (*FILTERS).borrow();
        if layer.is_empty() {
            // Change all layers
            for f in filters.values() {
                f.try_apply_directives_string(&directive)?;
            }
        } else {
            // Change a specific layer
            if let Some(f) = filters.get(layer.as_str()) {
                f.try_apply_directives_string(&directive)?;
            }
        }
        Ok(())
    }

    /// Apply ignore-list changes to a logging layer by name, or to all layers when `layer` is "all".
    // TODO: can we refine the TS type of `layer`?
    pub fn changeLogIgnore(layer: String, changes: Vec<String>) {
        let layer = if layer == "all" { "".to_owned() } else { layer };
        let filters = (*FILTERS).borrow();
        if layer.is_empty() {
            // Change all layers
            for f in filters.values() {
                #[allow(deprecated)]
                f.apply_ignore_change_list(&changes);
            }
        } else {
            // Change a specific layer
            if let Some(f) = filters.get(layer.as_str()) {
                #[allow(deprecated)]
                f.apply_ignore_change_list(&changes);
            }
        }
    }
    /// Shut down Veilid and terminate the API.
    ///
    /// The release for [VeilidClient::startupCore]. Blocks until the core finishes shutting down. A second call after shutdown errors with `NotInitialized`.
    ///
    /// Throws `NotInitialized` if the node is not running.
    pub async fn shutdownCore() -> VeilidAPIResult<()> {
        let veilid_api = take_veilid_api()?;
        veilid_api.shutdown().await;
        Ok(())
    }

    /// Check if Veilid is shutdown.
    pub fn isShutdown() -> VeilidAPIResult<bool> {
        let veilid_api = get_veilid_api();
        if let Err(VeilidAPIError::NotInitialized) = veilid_api {
            return Ok(true);
        }
        let veilid_api = veilid_api.unwrap_or_log();
        let is_shutdown = veilid_api.is_shutdown();
        Ok(is_shutdown)
    }

    /// Get a full copy of the current state of Veilid.
    ///
    /// Throws `NotInitialized` if the node is not started.
    pub async fn getState() -> VeilidAPIResult<VeilidState> {
        let veilid_api = get_veilid_api()?;
        veilid_api.get_state().await
    }

    /// Connect to the network.
    ///
    /// Sets the maintain-peers flag and returns; the network connect proceeds in the background tick loop. Errors if already attached.
    ///
    /// Throws the same error its core twin `VeilidAPI::attach` returns: `Generic` if already attached, and `NotInitialized` if the node is not started.
    pub async fn attach() -> VeilidAPIResult<()> {
        let veilid_api = get_veilid_api()?;
        veilid_api.attach().await
    }

    /// Disconnect from the network.
    ///
    /// Clears the maintain-peers flag and returns; the network teardown proceeds in the background tick loop. Errors if already detached.
    ///
    /// Throws the same error its core twin `VeilidAPI::detach` returns: `Generic` if already detached, and `NotInitialized` if the node is not started.
    pub async fn detach() -> VeilidAPIResult<()> {
        let veilid_api = get_veilid_api()?;
        veilid_api.detach().await
    }

    /// Get a cryptosystem by its kind
    ///
    /// Throws `InvalidArgument` if `kind` is not a cryptosystem this build supports, `NotInitialized` if the node is not started, and a parse error if `kind` fails to deserialize.
    pub fn getCrypto(
        #[wasm_bindgen(unchecked_param_type = "CryptoKind")] kind: JsValue,
    ) -> VeilidAPIResult<VeilidCrypto> {
        let kind = CryptoKind::from_js(kind)
            .map_err(|e| VeilidAPIError::parse_error("failed to parse kind", e.to_string()))?;
        let veilid_api = get_veilid_api()?;
        if veilid_api.crypto()?.get(kind).is_none() {
            apibail_invalid_argument!("get_crypto", "kind", kind);
        }
        Ok(VeilidCrypto { kind })
    }

    /// Verify multiple signatures with multiple cryptosystems
    ///
    /// Returns `undefined` if any supported signature fails to validate; otherwise the set of public keys that did validate. Throws `Generic` if a public key or signature has the wrong kind or length or if `publicKeys` or `signatures` fail to deserialize, `ParseError` if a matching public key is not a valid ed25519 point, and `NotInitialized` if the node is not started.
    pub fn verifySignatures(
        #[wasm_bindgen(unchecked_param_type = "PublicKey[]")] publicKeys: JsValue,
        data: Box<[u8]>,
        #[wasm_bindgen(unchecked_param_type = "Signature[]")] signatures: JsValue,
    ) -> VeilidAPIResult<Option<Vec<PublicKey>>> {
        let public_keys =
            try_from_js_array::<PublicKey>(publicKeys).map_err(VeilidAPIError::generic)?;
        let signatures =
            try_from_js_array::<Signature>(signatures).map_err(VeilidAPIError::generic)?;

        let veilid_api = get_veilid_api()?;
        let crypto = veilid_api.crypto()?;
        let out = crypto.verify_signatures(&public_keys, &data, &signatures)?;
        Ok(out.map(|v| v.iter().cloned().collect()))
    }

    /// Generate multiple signatures with multiple cryptosystems
    ///
    /// Unsupported crypto kinds in `keyPairs` are silently dropped. Throws `Generic` if a keypair has the wrong kind or length or if `keyPairs` fails to deserialize, `ParseError` or `Internal` if a keypair does not form a valid signing pair, and `NotInitialized` if the node is not started.
    pub fn generateSignatures(
        data: Box<[u8]>,
        #[wasm_bindgen(unchecked_param_type = "KeyPair[]")] keyPairs: JsValue,
    ) -> VeilidAPIResult<Vec<Signature>> {
        let key_pairs = try_from_js_array::<KeyPair>(keyPairs).map_err(VeilidAPIError::generic)?;
        let veilid_api = get_veilid_api()?;
        let crypto = veilid_api.crypto()?;
        crypto.generate_signatures(&data, &key_pairs, |_k, s| s)
    }

    /// Create a new MemberId for use with in creating `DHTSchema`s.
    ///
    /// Throws the same error its core twin `VeilidAPI::generate_member_id` returns: `Generic` if `writer_key` is an unsupported crypto kind, and `NotInitialized` if the node is not started.
    pub fn generateMemberId(writer_key: &PublicKey) -> VeilidAPIResult<MemberId> {
        let veilid_api = get_veilid_api()?;
        veilid_api.generate_member_id(writer_key)
    }

    /// Start a transaction on a set of DHT records
    /// Record keys must have been opened via a routing context already when passed to this function
    /// Options can be specified that supply a default signing keypair for records that are not opened for writing
    ///
    /// Blocks on a network Begin fanout across the record nodes (online-only). The returned [VeilidDHTTransaction] holds a network-side resource the caller must release via [VeilidDHTTransaction::commit] or [VeilidDHTTransaction::rollback]; dropping it without either tears the transaction down in the background.
    ///
    /// Throws the same error its core twin `VeilidAPI::transact_dht_records` returns: `InvalidArgument` if a record is not open or more than 32 records are passed, `MissingArgument` if `recordKeys` is empty or has duplicates, `Generic` if a record key is malformed or its encryption key does not match the opened record, `TryAgain` (retryable) if the DHT is offline, the records are contended, or Begin consensus was not reached, and `NotInitialized` if the node is not started. Network failures surface as `Timeout` or `NoConnection`. Also throws `Generic` if `recordKeys` or `options` fail to deserialize.
    pub async fn transactDHTRecords(
        #[wasm_bindgen(unchecked_param_type = "RecordKey[]")] recordKeys: JsValue,
        options: Option<ts::TransactDHTRecordsOptions>,
    ) -> VeilidAPIResult<VeilidDHTTransaction> {
        let record_keys =
            try_from_js_array::<RecordKey>(recordKeys).map_err(VeilidAPIError::generic)?;

        let veilid_api = get_veilid_api()?;

        let dht_transaction = veilid_api
            .transact_dht_records(
                record_keys,
                match options {
                    Some(o) => Some(o.try_into()?),
                    None => None,
                },
            )
            .await?;

        Ok(VeilidDHTTransaction {
            inner_transaction: Some(dht_transaction),
        })
    }

    /// Deterministicly builds the record key for a given schema and owner public key.
    /// The crypto kind of the record key will be that of the `owner` public key
    ///
    /// Local crypto computation only; despite being `async` it makes no network round-trip. The same inputs always yield the same key.
    ///
    /// Throws the same error its core twin `VeilidAPI::get_dht_record_key` returns: `InvalidArgument` or `Generic` if `schema` is malformed, `Generic` if `owner` or `encryptionKey` is an unsupported crypto kind, and `NotInitialized` if the node is not started. Also throws `Generic` (or `InvalidArgument` for `schema`) if `schema` or `encryptionKey` fail to deserialize.
    #[allow(clippy::unused_async)]
    pub async fn getDHTRecordKey(
        schema: ts::DHTSchema,
        owner: &PublicKey,
        encryptionKey: Option<TypeStubSharedSecret>,
    ) -> VeilidAPIResult<RecordKey> {
        let schema = schema.try_into()?;

        let encryption_key = match encryptionKey {
            Some(encryption_key) => try_from_js_option::<SharedSecret>(encryption_key)
                .map_err(VeilidAPIError::generic)?,
            None => None,
        };

        let veilid_api = get_veilid_api()?;

        veilid_api
            .get_dht_record_key(schema, owner.clone(), encryption_key)
            .await
    }

    /// Allocate a new private route set with default cryptography and network options.
    /// Returns a route id and a publishable 'blob' with the route encrypted with each crypto kind.
    /// Those nodes importing the blob will have their choice of which crypto kind to use.
    ///
    /// Returns a route id and 'blob' that can be published over some means (DHT or otherwise) to be imported by another Veilid node.
    ///
    /// Blocks on the network to allocate and test the route. The returned route id holds an allocated route the caller must free with [VeilidClient::releasePrivateRoute] or it leaks.
    ///
    /// Throws the same error its core twin `VeilidAPI::new_private_route` returns: `TryAgain` (retryable) if there is no valid PublicInternet network class yet, not enough nodes are known to build the route, or the route failed its reachability test, and `NotInitialized` if the node is not started.
    pub async fn newPrivateRoute() -> VeilidAPIResult<ts::RouteBlob> {
        let veilid_api = get_veilid_api()?;

        let route_blob = veilid_api.new_private_route().await?;
        Ok(route_blob.into())
    }

    /// Import a private route blob as a remote private route.
    ///
    /// Returns a route id that can be used to send private messages to the node creating this route.
    ///
    /// Local import, no network round-trip. The returned route id holds an imported route the caller must free with [VeilidClient::releasePrivateRoute] or it leaks.
    ///
    /// Throws the same error its core twin `VeilidAPI::import_remote_private_route` returns: `InvalidArgument` if `blob` is empty or names too many crypto kinds, `ParseError` if it is malformed, `Generic` if the decoded route has no first hop, and `NotInitialized` if the node is not started.
    #[allow(clippy::boxed_local)]
    pub fn importRemotePrivateRoute(blob: Box<[u8]>) -> VeilidAPIResult<RouteId> {
        let veilid_api = get_veilid_api()?;
        veilid_api.import_remote_private_route(blob.to_vec())
    }

    /// Allocate a new private route and specify a specific cryptosystem, stability and sequencing preference.
    /// Returns a route id and a publishable 'blob' with the route encrypted with each crypto kind.
    /// Those nodes importing the blob will have their choice of which crypto kind to use.
    ///
    /// Returns a route id and 'blob' that can be published over some means (DHT or otherwise) to be imported by another Veilid node.
    ///
    /// Blocks on the network to allocate and test the route. The returned route id holds an allocated route the caller must free with [VeilidClient::releasePrivateRoute] or it leaks.
    ///
    /// Throws the same error its core twin `VeilidAPI::new_custom_private_route` returns: `Generic` if `private_spec` names an invalid crypto kind, `InvalidArgument` if its hop count exceeds the configured maximum, `TryAgain` (retryable) if there is no valid PublicInternet network class yet, not enough nodes are known, or the route failed its reachability test, and `NotInitialized` if the node is not started.
    pub async fn newCustomPrivateRoute(
        private_spec: ts::PrivateSpec,
    ) -> VeilidAPIResult<ts::RouteBlob> {
        let veilid_api = get_veilid_api()?;
        let private_spec = private_spec.try_into()?;

        let route_blob = veilid_api.new_custom_private_route(private_spec).await?;
        Ok(route_blob.into())
    }

    /// Release either a locally allocated or remotely imported private route.
    ///
    /// This will deactivate the route and free its resources and it can no longer be sent to or received from.
    ///
    /// The release for [VeilidClient::newPrivateRoute], [VeilidClient::newCustomPrivateRoute], and [VeilidClient::importRemotePrivateRoute]. Local, no network round-trip. Releasing a route id that is unknown, already released, or malformed errors with `InvalidArgument`; throws `NotInitialized` if the node is not started.
    pub fn releasePrivateRoute(route_id: &RouteId) -> VeilidAPIResult<()> {
        let veilid_api = get_veilid_api()?;
        veilid_api.release_private_route(route_id.clone())
    }

    /// Respond to an AppCall received over a VeilidUpdate::AppCall.
    ///
    /// * `call_id` - specifies which call to reply to, and it comes from a VeilidUpdate::AppCall, specifically the VeilidAppCall::id() value.
    /// * `message` - is an answer blob to be returned by the remote node's RoutingContext::app_call() function, and may be up to 32768 bytes
    ///
    /// Awaits a network send of the answer.
    ///
    /// Throws a parse error if `callId` is not a valid operation id. Otherwise throws the same error its core twin `VeilidAPI::app_call_reply` returns: `Generic` if the `callId` is unknown or already answered, `TryAgain` if the node is mid-shutdown, and `NotInitialized` if the node is not started.
    pub async fn appCallReply(callId: String, message: Box<[u8]>) -> VeilidAPIResult<()> {
        let message = message.to_vec();
        let call_id = match callId.parse() {
            Ok(v) => v,
            Err(e) => {
                return VeilidAPIResult::Err(VeilidAPIError::parse_error(
                    format!("failed to parse call_id: {}", e),
                    callId,
                ));
            }
        };
        let veilid_api = get_veilid_api()?;
        veilid_api.app_call_reply(call_id, message).await
    }

    /// Get the current timestamp, in string format
    #[must_use]
    pub fn now() -> String {
        Timestamp::now().as_u64().to_string()
    }

    /// Get a monotonically non-decreasing timestamp, in string format.
    /// Successive calls return values that are never less than a previous call.
    #[must_use]
    pub fn nowNonDecreasing() -> String {
        Timestamp::now_non_decreasing().as_u64().to_string()
    }

    /// Get a strictly increasing timestamp, in string format. Successive
    /// calls return values that are strictly greater than any previous call,
    /// bumping by at least 1µs if the underlying clock has not advanced
    /// (e.g. on the web where Date.now() has ms resolution).
    #[must_use]
    pub fn nowIncreasing() -> String {
        Timestamp::now_increasing().as_u64().to_string()
    }

    /// Execute an 'internal debug command'.
    ///
    /// Throws a parse error if `command` cannot be tokenized, `Generic` if it is an unknown command, `InvalidArgument` if a known command's arguments are invalid, and `NotInitialized` if the node is not started. Individual commands may throw their own errors.
    pub async fn debug(command: String) -> VeilidAPIResult<String> {
        let veilid_api = get_veilid_api()?;
        veilid_api.debug(command).await
    }

    /// Return the cargo package version of veilid-core, in object format.
    #[must_use]
    pub fn version() -> VeilidVersion {
        let (major, minor, patch) = veilid_version();
        super::VeilidVersion {
            major,
            minor,
            patch,
        }
    }

    /// Return the features that were enabled when veilid-core was built.
    #[must_use]
    pub fn features() -> Vec<String> {
        veilid_features()
    }

    /// Return the cargo package version of veilid-core, in string format.
    #[must_use]
    pub fn versionString() -> String {
        veilid_version_string()
    }

    /// Return the default veilid configuration, in string format
    pub fn defaultConfig() -> VeilidConfig {
        VeilidConfig::default()
    }
}

/////////////////////////////////////////////////////////////////////////////////

/// A transaction over a set of DHT records, allowing atomic get/set across them.
#[wasm_bindgen]
pub struct VeilidDHTTransaction {
    inner_transaction: Option<DHTTransaction>,
}

#[wasm_bindgen]
impl VeilidDHTTransaction {
    fn getTransaction(&self) -> VeilidAPIResult<DHTTransaction> {
        let Some(transaction) = &self.inner_transaction else {
            return VeilidAPIResult::Err(veilid_core::VeilidAPIError::generic(
                "Unable to getTransaction instance. inner_transaction is None.",
            ));
        };
        VeilidAPIResult::Ok(transaction.clone())
    }

    /// Commit the transaction. Performs all actions atomically.
    ///
    /// The release for [VeilidClient::transactDHTRecords] (the other half is [VeilidDHTTransaction::rollback]). Blocks on the end and commit consensus barriers and requires the node to be online. Completes exactly once: a second commit or a rollback errors with `transaction_not_found`.
    ///
    /// Throws the same error its core twin `DHTTransaction::commit` returns: `TransactionNotFound` if the transaction was already committed, rolled back, or is unknown, and `TryAgain` (retryable) if the node is offline or the end/commit barriers could not reach consensus. Throws `NotInitialized` if the node is not started.
    pub async fn commit(&self) -> VeilidAPIResult<()> {
        let transaction = self.getTransaction()?;
        transaction.commit().await
    }

    /// Rollback the transaction. Does nothing to the DHT.
    ///
    /// The release for [VeilidClient::transactDHTRecords] (the other half is [VeilidDHTTransaction::commit]). Blocks sending rollbacks to the network and requires the node to be online. Completes exactly once: a second rollback or a commit errors with `transaction_not_found`.
    ///
    /// Throws the same error its core twin `DHTTransaction::rollback` returns: `TransactionNotFound` if the transaction was already committed, rolled back, or is unknown, and `TryAgain` (retryable) if the node is offline. Throws `NotInitialized` if the node is not started.
    pub async fn rollback(&self) -> VeilidAPIResult<()> {
        let transaction = self.getTransaction()?;
        transaction.rollback().await
    }

    /// Extend the transaction with additional record keys.
    ///
    /// Records already in the transaction are skipped. Idempotent if no
    /// new records are supplied. May return `TryAgain` on Begin contention.
    ///
    /// Blocks on a network Begin fanout for the new records (online-only).
    ///
    /// Throws the same error its core twin `DHTTransaction::extend` returns: `TransactionNotFound` if the transaction handle is already completed or unknown, `MissingArgument` if `recordKeys` contains duplicates, `InvalidArgument` if the merged record set would exceed the per-transaction record limit, and `TryAgain` (retryable) if the node is offline or the Begin fanout for the added records could not reach consensus. Throws `NotInitialized` if the node is not started, and `Generic` if `recordKeys` or `options` fail to deserialize.
    pub async fn extend(
        &self,
        #[wasm_bindgen(unchecked_param_type = "RecordKey[]")] recordKeys: JsValue,
        options: Option<ts::TransactDHTRecordsOptions>,
    ) -> VeilidAPIResult<()> {
        let record_keys =
            try_from_js_array::<RecordKey>(recordKeys).map_err(VeilidAPIError::generic)?;
        let transaction = self.getTransaction()?;
        transaction
            .extend(
                record_keys,
                match options {
                    Some(o) => Some(o.try_into()?),
                    None => None,
                },
            )
            .await
    }

    /// Add a set_dht_value operation to the transaction
    ///
    /// * Will fail if performed offline
    /// * Will fail if existing offline writes exist for this record key
    ///
    /// The writer, if specified, will override the 'default_writer' specified when the record is opened.
    ///
    /// Returns `None` if the value was successfully set.
    /// Returns `Some(data)` if the value set was older than the one available on the network.
    ///
    /// Blocks on the per-subkey lock (unbounded) and the set RPC to the transaction's node set. Each per-node RPC is bounded by `network.rpc.timeout_ms`, but the lock wait and retry rounds are not, so the whole call has no single-timeout bound.
    ///
    /// Throws the same error its core twin `DHTTransaction::set` returns: `TransactionNotFound` if the transaction handle is already completed or no longer in the Begin stage, `InvalidArgument` if `recordKey` is not open in the transaction or `subkey` is outside the schema range, `Generic` if `recordKey` is malformed (unsupported kind or bad length) or the subkey has no writer, and `TryAgain` (retryable) if the node is offline or write consensus was not reached this round. A non-responding node is retried rather than surfaced as a timeout. Throws `NotInitialized` if the node is not started, and `Generic` if `options` fails to deserialize.
    pub async fn set(
        &self,
        recordKey: &RecordKey,
        subkey: ValueSubkey,
        data: Vec<u8>,
        options: Option<ts::DHTTransactionSetValueOptions>,
    ) -> VeilidAPIResult<Option<ts::ValueData>> {
        let transaction = self.getTransaction()?;

        transaction
            .set(
                recordKey.clone(),
                subkey,
                data,
                match options {
                    Some(o) => Some(o.try_into()?),
                    None => None,
                },
            )
            .await
            .map(|x| x.map(|y| y.into()))
    }

    /// Perform a get_dht_value operation inside the transaction
    ///
    /// * Will fail if performed offline
    /// * Will pull the latest value from the network, will fail if the local value is newer
    /// * Will fail if existing offline writes exist for this record key
    ///
    /// Returns `None` if the value subkey has not yet been set.
    /// Returns `Some(data)` if the value subkey has valid data.
    ///
    /// Blocks on the per-subkey lock (unbounded) and the get RPC to the transaction's node set. Each per-node RPC is bounded by `network.rpc.timeout_ms`, but the lock wait and retry rounds are not, so the whole call has no single-timeout bound.
    ///
    /// Throws the same error its core twin `DHTTransaction::get` returns: `TransactionNotFound` if the transaction handle is already completed or no longer in the Begin stage, `InvalidArgument` if `recordKey` is not in the transaction or `subkey` is outside the schema range, `Generic` if `recordKey` is malformed (unsupported kind or bad length), and `TryAgain` (retryable) if the node is offline or the network did not return the value that existed at begin time. A non-responding node is retried rather than surfaced as a timeout. Throws `NotInitialized` if the node is not started.
    pub async fn get(
        &self,
        recordKey: &RecordKey,
        subkey: ValueSubkey,
    ) -> VeilidAPIResult<Option<ts::ValueData>> {
        let transaction = self.getTransaction()?;

        transaction
            .get(recordKey.clone(), subkey)
            .await
            .map(|x| x.map(|y| y.into()))
    }

    /// Perform a inspect_dht_record operation inside the transaction
    ///
    /// * Does not perform any network activity, as the transaction state keeps all of the required information after the begin
    ///
    /// For information on arguments, see [RoutingContext::inspect_dht_record]
    ///
    /// Returns a DHTRecordReport with the subkey ranges that were returned that overlapped the schema, and sequence numbers for each of the subkeys in the range.
    ///
    /// Throws the same error its core twin `DHTTransaction::inspect` returns: `TransactionNotFound` if the transaction handle is already completed, unknown, or no longer in the Begin stage, `InvalidArgument` if `recordKey` is not in the transaction (a malformed or wrong-kind key is not found), and `Generic` if the transaction has not started. Performs no network activity and cannot time out. Throws `NotInitialized` if the node is not started.
    pub async fn inspect(
        &self,
        recordKey: &RecordKey,
        subkeys: Option<ValueSubkeyRangeSet>,
        scope: Option<DHTReportScope>,
    ) -> VeilidAPIResult<DHTRecordReport> {
        let scope = scope.unwrap_or_default();

        let transaction = self.getTransaction()?;

        transaction.inspect(recordKey.clone(), subkeys, scope).await
    }
}
