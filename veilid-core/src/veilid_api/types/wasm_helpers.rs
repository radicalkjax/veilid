use super::*;

// Recovers a wasm-bindgen-exported veilid-core type from a `&JsValue` by reading its
// `__wbg_ptr`, used by the crypto-type macros to accept JS instances as parameters.
//
// wasm-bindgen's own `TryFromJsValue` consumes the JS wrapper, so this reads through it
// instead. Before touching the pointer, the value must be an instance of exactly this
// type's JS class: its prototype is compared by identity against the prototype of a seed
// instance, which survives minification and rejects any other class carrying a `__wbg_ptr`.

/// Implement `TryFrom<&JsValue>` for a wasm-bindgen type by checking its class and
/// cloning the instance behind its `__wbg_ptr`. `$seed` builds a throwaway instance whose
/// JS class prototype identifies the type.
macro_rules! impl_try_from_js_value {
    ($name:ident, $seed:expr) => {
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        impl ::core::convert::TryFrom<&::wasm_bindgen::JsValue> for $name {
            type Error = String;

            fn try_from(js: &::wasm_bindgen::JsValue) -> Result<Self, Self::Error> {
                use ::wasm_bindgen::convert::RefFromWasmAbi;
                use ::wasm_bindgen::JsCast;

                thread_local! {
                    static PROTOTYPE: ::wasm_bindgen::JsValue = {
                        let seed: ::wasm_bindgen::JsValue = ::wasm_bindgen::JsValue::from($seed);
                        ::js_sys::Object::get_prototype_of(&seed).into()
                    };
                }

                let obj: &::js_sys::Object = js.dyn_ref().ok_or_else(|| {
                    ::alloc::format!(
                        "expected {}, value is not an object",
                        ::core::stringify!($name)
                    )
                })?;
                let proto: ::wasm_bindgen::JsValue = ::js_sys::Object::get_prototype_of(obj).into();
                if !PROTOTYPE.with(|expected| ::js_sys::Object::is(&proto, expected)) {
                    return Err(::alloc::format!(
                        "expected {}, value is an instance of another class",
                        ::core::stringify!($name)
                    ));
                }
                let ptr =
                    ::js_sys::Reflect::get(js, &::wasm_bindgen::JsValue::from_str("__wbg_ptr"))
                        .ok()
                        .and_then(|v| v.as_f64())
                        .ok_or_else(|| {
                            ::alloc::format!(
                                "expected {}, missing __wbg_ptr",
                                ::core::stringify!($name)
                            )
                        })?;
                // wasm-bindgen zeroes __wbg_ptr when the object is freed
                if ptr.fract() != 0.0 || ptr <= 0.0 || ptr > f64::from(u32::MAX) {
                    return Err(::alloc::format!(
                        "expected {}, object was freed or its __wbg_ptr is invalid",
                        ::core::stringify!($name)
                    ));
                }
                let wasm_ptr = ::wasm_bindgen::__rt::WasmPtr::<
                    ::wasm_bindgen::__rt::WasmRefCell<$name>,
                >::from_usize(ptr as u32 as usize);
                // SAFETY: the prototype check proves js is a wrapper of this class, so its
                // __wbg_ptr is one wasm-bindgen minted for a live WasmRefCell<$name>
                let instance_ref = unsafe { Self::ref_from_abi(wasm_ptr) };
                Ok(instance_ref.clone())
            }
        }
    };
}
pub(crate) use impl_try_from_js_value;

thread_local! {
    /// Builds the JS value thrown for an error: `(data, message) => any`.
    ///
    /// Unset by default, which serializes the error as a plain object. A binding crate
    /// installs one to throw its own `Error` subclasses instead; veilid-wasm does, and
    /// veilid-flutter deliberately does not, since its Dart side reads the plain form.
    static JS_ERROR_FACTORY: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}

/// Install the factory that builds thrown JS errors, replacing any previous one.
///
/// Call before the first fallible export runs, or errors raised until then fall back to
/// the plain serialized object.
pub fn set_js_error_factory(factory: js_sys::Function) {
    JS_ERROR_FACTORY.with_borrow_mut(|f| *f = Some(factory));
}

/// Lets `Result<_, VeilidAPIError>` cross `#[wasm_bindgen]` boundaries as a thrown
/// value, through the installed factory when there is one.
impl From<VeilidAPIError> for wasm_bindgen::JsValue {
    fn from(e: VeilidAPIError) -> Self {
        let message = wasm_bindgen::JsValue::from_str(&e.to_string());
        let data = serde_wasm_bindgen::to_value(&e).unwrap_or_else(|_| message.clone());
        JS_ERROR_FACTORY
            .with_borrow(|f| {
                f.as_ref()
                    .and_then(|factory| factory.call2(&JsValue::UNDEFINED, &data, &message).ok())
            })
            .unwrap_or(data)
    }
}
