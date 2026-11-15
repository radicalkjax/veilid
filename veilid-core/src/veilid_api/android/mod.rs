use super::*;

use jni::{errors::Result as JniResult, objects::Global, objects::JObject, EnvUnowned, JavaVM};
use lazy_static::*;

/// The Android JavaVM and application `Context` that veilid-core holds onto for
/// JNI calls back into the host app (e.g. protected-store keystore access).
pub struct AndroidGlobals {
    /// The Java virtual machine of the host Android application.
    pub vm: JavaVM,
    /// A global reference to the host application's Android `Context`.
    pub ctx: Global<JObject<'static>>,
}

impl Drop for AndroidGlobals {
    fn drop(&mut self) {
        // Ensure we're attached before dropping Global
        self.vm
            .attach_current_thread(|_| JniResult::Ok(()))
            .unwrap_or_log();
    }
}

lazy_static! {
    /// Process-wide storage for the Android JavaVM and `Context`, populated by
    /// [veilid_core_setup_android] before the node starts.
    pub static ref ANDROID_GLOBALS: Arc<Mutex<Option<AndroidGlobals>>> = Arc::new(Mutex::new(None));
}

/// Register the host Android application's JNI environment and `Context` with
/// veilid-core. Must be called from the app (via JNI) before startup.
///
/// Idempotent. Every Flutter engine creation attaches the plugin and calls this again, including
/// background engines a running node knows nothing about, so a call made with globals already
/// registered checks the incoming `Context` and returns without replacing them.
///
/// A `Context` that is not the registered one is ignored, not adopted. The expected argument is the
/// process-wide application `Context`, which is a per-process singleton, so a mismatch means the
/// caller passed something narrower and shorter-lived, and swapping it would hand a running node's
/// JNI calls a `Context` that can be torn down under it.
pub fn veilid_core_setup_android(mut env: EnvUnowned, ctx: JObject) {
    env.with_env(|env| -> JniResult<()> {
        let mut globals_locked = ANDROID_GLOBALS.lock();
        if let Some(globals) = globals_locked.as_ref() {
            if !env.is_same_object(&ctx, globals.ctx.as_obj())? {
                warn!("veilid_core_setup_android called with a different Context, keeping the registered one");
            }
            return Ok(());
        }
        let ctx = env.new_global_ref(ctx)?;
        let vm = env.get_java_vm()?;
        *globals_locked = Some(AndroidGlobals { vm, ctx });
        Ok(())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

/// Whether [veilid_core_setup_android] has run and the Android globals are set.
#[must_use]
pub fn is_android_ready() -> bool {
    ANDROID_GLOBALS.lock().is_some()
}

/// Get a thread-attached copy of the Android JavaVM and `Context` globals.
/// Panics if [veilid_core_setup_android] has not been called.
#[must_use]
pub fn get_android_globals() -> (JavaVM, Global<JObject<'static>>) {
    let globals_locked = ANDROID_GLOBALS.lock();
    let globals = globals_locked.as_ref().unwrap_or_log();
    globals
        .vm
        .attach_current_thread(|env| {
            let vm = env.get_java_vm().unwrap_or_log();
            let ctx = env.new_global_ref(globals.ctx.as_obj()).unwrap_or_log();
            JniResult::Ok((vm, ctx))
        })
        .unwrap_or_log()
}
