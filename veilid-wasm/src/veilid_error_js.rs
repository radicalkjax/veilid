use super::*;

#[wasm_bindgen(module = "/js/veilid_errors_bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name = veilidErrorFactory)]
    fn veilid_error_factory() -> Function;
}

/// Makes every error thrown out of wasm an instance of the `VeilidAPIError` class
/// hierarchy in `js/veilid_errors.js`.
///
/// Registering with veilid-core rather than converting here covers the fallible items
/// veilid-core exports itself, such as `RecordKey.parse`. Runs during `init()`, before
/// any export is reachable.
#[wasm_bindgen(start)]
fn register_error_factory() {
    veilid_core::set_js_error_factory(veilid_error_factory());
}
