//! Tracing layer that reports spans and events to the browser console and
//! `window.performance` marks/measures.

mod config;
mod layer;
pub(crate) mod recorder;

pub use config::*;
pub use layer::*;
pub use recorder::WasmFieldFilter;

use tracing::dispatcher::SetGlobalDefaultError;
use tracing_subscriber::layer::*;
use tracing_subscriber::registry::*;

use wasm_bindgen::prelude::*;

/// Re-exports of common types
pub mod prelude {
    #[doc(no_inline)]
    pub use super::{config::WasmLayerConfig, layer::WasmLayer, recorder::WasmFieldFilter};
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now() -> f64;
    #[wasm_bindgen(js_namespace = performance)]
    fn mark(name: &str);
    #[wasm_bindgen(catch, js_namespace = performance)]
    fn measure(name: String, startMark: String) -> Result<(), JsValue>;
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log1(message: String);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log4(message1: String, message2: &str, message3: &str, message4: &str);
    #[wasm_bindgen(js_namespace = console, js_name = debug)]
    fn debug1(message: String);
    #[wasm_bindgen(js_namespace = console, js_name = debug)]
    fn debug4(message1: String, message2: &str, message3: &str, message4: &str);
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn warn1(message: String);
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn warn4(message1: String, message2: &str, message3: &str, message4: &str);
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn error1(message: String);
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn error4(message1: String, message2: &str, message3: &str, message4: &str);
}

#[inline]
fn thread_display_suffix() -> &'static str {
    ""
}

fn mark_name(id: &tracing::Id) -> String {
    format!("t{:x}", id.into_u64())
}

/// Set a [`WasmLayer`] with default config as the global tracing subscriber.
///
/// Panics if a global default is already set.
pub fn set_as_global_default() {
    tracing::subscriber::set_global_default(
        Registry::default().with(WasmLayer::new(WasmLayerConfig::default())),
    )
    .expect("default global");
}

/// Set a [`WasmLayer`] with default config as the global tracing subscriber, without panicking.
pub fn try_set_as_global_default() -> Result<(), SetGlobalDefaultError> {
    tracing::subscriber::set_global_default(
        Registry::default().with(WasmLayer::new(WasmLayerConfig::default())),
    )
}

/// Set a [`WasmLayer`] with the given config as the global tracing subscriber.
pub fn set_as_global_default_with_config(
    config: WasmLayerConfig,
) -> Result<(), SetGlobalDefaultError> {
    tracing::subscriber::set_global_default(Registry::default().with(WasmLayer::new(config)))
}
