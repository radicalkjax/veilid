//! Posting to Dart from Rust, wasm flavor.
//!
//! [`DartUpdateSinkSet`] fans the veilid-core update callback out to every Dart update stream
//! registered against the running node, the sibling of the FFI shim's sink set. Rules it rests on:
//!
//! - Never call into Dart while holding the sink borrow. A Dart handler reenters wasm (a cancelled
//!   subscription deregisters itself), and a live `RefCell` borrow turns that into a panic. Copy
//!   out, call, then mutate.
//! - A Dart consumer parked in `await for` wakes only on a close, so every path that drops a sink
//!   calls its callback with null.
//! - There is no web analogue of the FFI port liveness probe. A Dart hot restart leaves the
//!   previous program's interop closures callable, so a leftover sink keeps accepting updates and
//!   cannot be detected by calling it. Staleness is decided by the generation id of the Dart
//!   program that registered the sink, resting on one Dart program per page. A throwing callback
//!   is only the backstop for a JS context that is genuinely gone.

use super::*;

/// One registered Dart update stream, keyed in the set by caller id and tagged with the
/// generation of the Dart program that registered it.
#[derive(Clone)]
pub struct DartUpdateSink {
    generation: String,
    callback: Function,
}

impl DartUpdateSink {
    pub fn new(generation: String, callback: Function) -> Self {
        DartUpdateSink {
            generation,
            callback,
        }
    }

    /// Deliver one serialized update, returning whether the Dart handler accepted it.
    pub fn item_json(&self, json: &str) -> bool {
        self.callback
            .call1(&JsValue::UNDEFINED, &JsValue::from_str(json))
            .is_ok()
    }

    /// Close the Dart stream. A null update is the close message.
    pub fn close(&self) -> bool {
        self.callback
            .call1(&JsValue::UNDEFINED, &JsValue::NULL)
            .is_ok()
    }
}

/// The set of Dart update streams a running node fans its updates out to.
///
/// Shared between the wasm shim and the `UpdateCallback` handed to veilid-core. The `SendWrapper`
/// is what lets a single-threaded `RefCell` satisfy the callback's `Send + Sync` bound.
pub struct DartUpdateSinkSet {
    sinks: SendWrapper<RefCell<BTreeMap<String, DartUpdateSink>>>,
}

impl Default for DartUpdateSinkSet {
    fn default() -> Self {
        DartUpdateSinkSet {
            sinks: SendWrapper::new(RefCell::new(BTreeMap::new())),
        }
    }
}

impl Drop for DartUpdateSinkSet {
    fn drop(&mut self) {
        for sink in self.sinks.get_mut().values() {
            sink.close();
        }
    }
}

impl DartUpdateSinkSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a sink for `caller_id`, returning whether it was not already registered.
    pub fn register(&self, caller_id: String, generation: String, callback: Function) -> bool {
        self.sinks
            .borrow_mut()
            .insert(caller_id, DartUpdateSink::new(generation, callback))
            .is_none()
    }

    /// Retire every sink a Dart program other than `generation` registered.
    ///
    /// A hot restart replaces the Dart program while this module and its node carry on, so those
    /// sinks belong to a context that no longer exists. Closing them ends the streams the old
    /// context is still consuming.
    pub fn retire_other_generations(&self, generation: &str) {
        let stale: Vec<DartUpdateSink> = {
            let mut sinks = self.sinks.borrow_mut();
            let stale_ids: Vec<String> = sinks
                .iter()
                .filter(|(_, sink)| sink.generation != generation)
                .map(|(caller_id, _)| caller_id.clone())
                .collect();
            stale_ids
                .iter()
                .filter_map(|caller_id| sinks.remove(caller_id))
                .collect()
        };
        for sink in stale {
            sink.close();
        }
    }

    /// Deregister `caller_id` and close its stream, returning whether it was registered.
    pub fn retire(&self, caller_id: &str) -> bool {
        let sink = self.sinks.borrow_mut().remove(caller_id);
        let Some(sink) = sink else {
            return false;
        };
        sink.close();
        true
    }

    /// Deregister `caller_id` without closing its stream, for a consumer that already ended it.
    pub fn deregister(&self, caller_id: &str) -> bool {
        self.sinks.borrow_mut().remove(caller_id).is_some()
    }

    pub fn contains(&self, caller_id: &str) -> bool {
        self.sinks.borrow().contains_key(caller_id)
    }

    pub fn len(&self) -> usize {
        self.sinks.borrow().len()
    }

    /// Post `value` to every registered sink, retiring the ones whose callback throws.
    pub fn broadcast_json<T: Serialize + fmt::Debug>(&self, value: T) {
        let sinks: Vec<(String, DartUpdateSink)> = self
            .sinks
            .borrow()
            .iter()
            .map(|(caller_id, sink)| (caller_id.clone(), sink.clone()))
            .collect();
        if sinks.is_empty() {
            return;
        }
        let json = veilid_core::serialize_json(value);
        let mut dead = Vec::<String>::new();
        for (caller_id, sink) in sinks {
            // the dart handler swallows its own decode failures, so a throw is a broken js context
            if !sink.item_json(&json) {
                dead.push(caller_id);
            }
        }
        if dead.is_empty() {
            return;
        }
        let dead: Vec<DartUpdateSink> = {
            let mut sinks = self.sinks.borrow_mut();
            dead.iter()
                .filter_map(|caller_id| sinks.remove(caller_id))
                .collect()
        };
        for sink in dead {
            sink.close();
        }
    }

    /// Deregister every sink and close its stream.
    pub fn close_all(&self) {
        let sinks = core::mem::take(&mut *self.sinks.borrow_mut());
        for sink in sinks.values() {
            sink.close();
        }
    }
}
