//! Posting to Dart from Rust.
//!
//! [`DartIsolateWrapper`] answers one FFI call on a one-shot port. [`DartUpdateSinkSet`] fans the
//! veilid-core update callback out to every Dart update stream registered against the running
//! node. Rules the sink set rests on:
//!
//! - Never post while holding the sink lock. Disposing a Dart stream posts a close message, and a
//!   synchronous dispose racing an in-flight post deadlocks. Take what you need out under the
//!   lock, release, then post.
//! - Posting from a tokio thread after every Flutter engine has died is defined only because
//!   Flutter leaks a strong VM reference (`leak_vm = true`) so `Dart_Cleanup` never runs;
//!   `Dart_PostCObject` is contractually legal only before `Dart_Cleanup`.
//! - A Dart consumer parked in `await for` wakes only on a close message, so every path that
//!   retires a sink posts one.

use crate::tools::*;

pub use allo_isolate::IntoDart;
use allo_isolate::Isolate;
use core::fmt::Debug;
use core::future::Future;
use parking_lot::Mutex;
use serde::*;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct DartIsolateWrapper {
    isolate: Isolate,
}

const MESSAGE_OK: i32 = 0;
//const MESSAGE_ERR: i32 = 1;
const MESSAGE_OK_JSON: i32 = 2;
const MESSAGE_ERR_JSON: i32 = 3;
//const MESSAGE_STREAM_ITEM: i32 = 4;
const MESSAGE_STREAM_ITEM_JSON: i32 = 5;
//const MESSAGE_STREAM_ABORT: i32 = 6;
//const MESSAGE_STREAM_ABORT_JSON: i32 = 7;
const MESSAGE_STREAM_CLOSE: i32 = 8;

impl DartIsolateWrapper {
    pub fn new(port: i64) -> Self {
        DartIsolateWrapper {
            isolate: Isolate::new(port),
        }
    }

    pub fn spawn_result<F, T, E>(self, future: F)
    where
        F: Future<Output = Result<T, E>> + Send + 'static,
        T: IntoDart + Debug,
        E: Serialize + Debug,
    {
        spawn(async move {
            self.result(future.await);
        });
    }

    pub fn spawn_result_json<F, T, E>(self, future: F)
    where
        F: Future<Output = Result<T, E>> + Send + 'static,
        T: Serialize + Debug,
        E: Serialize + Debug,
    {
        spawn(async move {
            self.result_json(future.await);
        });
    }

    pub fn result<T: IntoDart + Debug, E: Serialize + Debug>(self, result: Result<T, E>) -> bool {
        match result {
            Ok(v) => self.ok(v),
            Err(e) => self.err_json(e),
        }
    }

    pub fn result_json<T: Serialize + Debug, E: Serialize + Debug>(
        self,
        result: Result<T, E>,
    ) -> bool {
        match result {
            Ok(v) => self.ok_json(v),
            Err(e) => self.err_json(e),
        }
    }

    pub fn ok<T: IntoDart>(self, value: T) -> bool {
        self.isolate
            .post(vec![MESSAGE_OK.into_dart(), value.into_dart()])
    }

    pub fn ok_json<T: Serialize + Debug>(self, value: T) -> bool {
        self.isolate.post(vec![
            MESSAGE_OK_JSON.into_dart(),
            veilid_core::serialize_json(value).into_dart(),
        ])
    }

    // pub fn err<E: IntoDart + Debug>(self, error: E) -> bool {
    //     self.isolate
    //         .post(vec![MESSAGE_ERR.into_dart(), error.into_dart()])
    // }

    pub fn err_json<E: Serialize + Debug>(self, error: E) -> bool {
        self.isolate.post(vec![
            MESSAGE_ERR_JSON.into_dart(),
            veilid_core::serialize_json(error).into_dart(),
        ])
    }
}

/// One registered Dart update stream, identified by the ReceivePort it posts to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DartUpdateSink {
    port: i64,
}

impl DartUpdateSink {
    pub fn new(port: i64) -> Self {
        DartUpdateSink { port }
    }

    pub fn port(&self) -> i64 {
        self.port
    }

    pub fn item_json(&self, json: String) -> bool {
        Isolate::new(self.port).post(vec![MESSAGE_STREAM_ITEM_JSON.into_dart(), json.into_dart()])
    }

    pub fn close(&self) -> bool {
        Isolate::new(self.port).post(vec![MESSAGE_STREAM_CLOSE.into_dart()])
    }
}

/// The set of Dart update streams a running node fans its updates out to.
///
/// Shared between the FFI shim and the `UpdateCallback` handed to veilid-core, which posts through
/// it from tokio threads for the life of the node.
#[derive(Default)]
pub struct DartUpdateSinkSet {
    sinks: Mutex<BTreeMap<i64, DartUpdateSink>>,
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

    /// Register a sink for `port`, returning whether it was not already registered.
    pub fn register(&self, port: i64) -> bool {
        self.sinks
            .lock()
            .insert(port, DartUpdateSink::new(port))
            .is_none()
    }

    /// Deregister `port` and close its stream, returning whether it was registered.
    pub fn retire(&self, port: i64) -> bool {
        let sink = self.sinks.lock().remove(&port);
        let Some(sink) = sink else {
            return false;
        };
        sink.close();
        true
    }

    pub fn contains(&self, port: i64) -> bool {
        self.sinks.lock().contains_key(&port)
    }

    pub fn len(&self) -> usize {
        self.sinks.lock().len()
    }

    /// Post `value` to every registered sink, reaping the ones whose port is proven gone.
    pub fn broadcast_json<T: Serialize + Debug>(&self, value: T) {
        let sinks: Vec<DartUpdateSink> = self.sinks.lock().values().copied().collect();
        if sinks.is_empty() {
            return;
        }
        let json = veilid_core::serialize_json(value);
        let mut dead = Vec::<i64>::new();
        for sink in sinks {
            // a failed update post is not proof the port is gone, see dart_port_is_live
            if !sink.item_json(json.clone()) && !dart_port_is_live(sink.port()) {
                dead.push(sink.port());
            }
        }
        if dead.is_empty() {
            return;
        }
        let mut sinks = self.sinks.lock();
        for port in dead {
            sinks.remove(&port);
        }
    }

    /// Drop every registered sink whose port is proven gone, leaving `keep` alone.
    ///
    /// A caller releasing its own reference has not proven its port is live, so `keep` is never
    /// reaped out from under it.
    pub fn reap_dead_except(&self, keep: i64) {
        let ports: Vec<i64> = self
            .sinks
            .lock()
            .keys()
            .copied()
            .filter(|port| *port != keep)
            .collect();
        let dead: Vec<i64> = ports
            .into_iter()
            .filter(|port| !dart_port_is_live(*port))
            .collect();
        if dead.is_empty() {
            return;
        }
        let mut sinks = self.sinks.lock();
        for port in dead {
            sinks.remove(&port);
        }
    }

    /// Deregister every sink and close its stream.
    pub fn close_all(&self) {
        let sinks = core::mem::take(&mut *self.sinks.lock());
        for sink in sinks.values() {
            sink.close();
        }
    }
}

/// A Dart port that has left the VM port map rejects every post.
///
/// The int32 arm of Dart's message serializer has no failure path, so a rejected primitive proves
/// the port is gone. A rejected update does not: Dart can fail to serialize a String for encoding
/// reasons (invalid utf8, invalid string length) on a perfectly live port.
pub fn dart_port_is_live(port: i64) -> bool {
    port != 0 && Isolate::new(port).post(0i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    // With no Dart VM attached every post fails, which is the dead-port path.
    const PORT: i64 = 42;

    #[test]
    fn register_and_retire() {
        let sinks = DartUpdateSinkSet::new();
        assert_eq!(sinks.len(), 0);
        assert!(!sinks.retire(PORT));

        assert!(sinks.register(PORT));
        assert!(!sinks.register(PORT));
        assert!(sinks.register(PORT + 1));
        assert_eq!(sinks.len(), 2);
        assert!(sinks.contains(PORT));

        assert!(sinks.retire(PORT));
        assert!(!sinks.retire(PORT));
        assert_eq!(sinks.len(), 1);

        sinks.close_all();
        assert_eq!(sinks.len(), 0);
    }

    #[test]
    fn reap_leaves_the_kept_port() {
        let sinks = DartUpdateSinkSet::new();
        sinks.register(PORT);
        sinks.register(PORT + 1);
        sinks.reap_dead_except(PORT);
        assert_eq!(sinks.len(), 1);
        assert!(sinks.contains(PORT));
    }

    #[test]
    fn broadcast_reaps_dead_ports() {
        let sinks = DartUpdateSinkSet::new();
        sinks.register(PORT);
        sinks.register(PORT + 1);
        sinks.broadcast_json("update");
        assert_eq!(sinks.len(), 0);
    }

    #[test]
    fn zero_port_is_never_live() {
        assert!(!dart_port_is_live(0));
        assert!(!dart_port_is_live(PORT));
    }
}
