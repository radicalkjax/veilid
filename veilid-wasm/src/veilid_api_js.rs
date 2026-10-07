use super::*;

// API Singleton
lazy_static! {
    /// The single `VeilidAPI` instance for this WASM node, populated by startup and cleared by shutdown.
    pub static ref VEILID_API: SendWrapper<RefCell<Option<VeilidAPI>>> =
        SendWrapper::new(RefCell::new(None));
    /// Tracing layer filters by name, used to change log levels and ignore lists at runtime.
    pub static ref FILTERS: SendWrapper<RefCell<BTreeMap<&'static str, VeilidLayerFilter>>> =
        SendWrapper::new(RefCell::new(BTreeMap::new()));
}

/// Clone the active `VeilidAPI`, or return `NotInitialized` if the node has not started.
pub fn get_veilid_api() -> Result<VeilidAPI, veilid_core::VeilidAPIError> {
    (*VEILID_API)
        .borrow()
        .clone()
        .ok_or(VeilidAPIError::NotInitialized)
}

/// Take the active `VeilidAPI`, leaving the singleton empty, or return `NotInitialized`.
pub fn take_veilid_api() -> Result<VeilidAPI, VeilidAPIError> {
    (**VEILID_API).take().ok_or(VeilidAPIError::NotInitialized)
}
