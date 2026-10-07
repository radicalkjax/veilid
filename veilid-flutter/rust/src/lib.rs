#![recursion_limit = "256"]

cfg_if::cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        // Dart-flavored wasm-bindgen bindings.
        use core::fmt;
        use futures_util::FutureExt;
        use gloo_utils::format::JsValueSerdeExt;
        use js_sys::*;
        use lazy_static::*;
        use send_wrapper::*;
        use serde::*;
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::*;
        use veilid_core::tools::*;
        use veilid_core::*;
        use veilid_core::tools::wasm_tracing::*;
        use wasm_bindgen_futures::*;

        pub mod dart_wasm;
        pub mod veilid_version;
        pub mod veilid_wasm_config;

        pub use veilid_version::*;
        pub use veilid_wasm_config::*;
    } else {
        // Native FFI bindings for Linux/Mac/Windows/iOS/Android.
        mod dart_ffi;
        mod dart_isolate_wrapper;
        mod tools;

        #[cfg(target_os = "android")]
        use jni::{objects::JClass, objects::JObject, EnvUnowned};

        #[cfg(target_os = "android")]
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "system" fn Java_com_veilid_veilid_VeilidPlugin_init_1android(
            env: EnvUnowned,
            _class: JClass,
            ctx: JObject,
        ) {
            veilid_core::veilid_core_setup_android(env, ctx);
        }
    }
}
