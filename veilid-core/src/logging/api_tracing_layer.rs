use super::api_log_queue::{ApiLogQueue, QueuedLog};
use crate::core_context::*;
use crate::veilid_api::*;
use crate::*;
use core::fmt::Write;
#[cfg(feature = "backtrace")]
use std::backtrace::Backtrace;
use tracing_subscriber::*;

struct ApiTracingLayerInner {
    update_callbacks: HashMap<String, UpdateCallback>,
}

/// API Tracing layer for 'tracing' subscribers
///
/// API Tracing is responsible for producing the `VeilidUpdate::Log` update callbacks
/// from internal tracing events for veilid-core and its registered components and the external crates enabled
/// via the VeilidLayerFilter that is places
///
/// For normal application use one should call ApiTracingLayer::init() and insert the
/// layer into your subscriber before calling api_startup() or api_startup_json().
///
/// For apps that call api_startup() many times concurrently with different 'namespace' or
/// 'program_name', you may want to disable api tracing as it can slow the system down
/// considerably. In those cases, deferring to buffered disk-based logging files is probably a better idea.
/// At the very least, no more verbose than info-level logging is recommended when using API tracing
/// with many copies of Veilid running.
///
/// Example:
///
/// ```rust,no_run
/// # use veilid_core::{*, tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt}};
///    let filter = VeilidLayerFilter::default();
///    let layer = ApiTracingLayer::init().with_filter(filter);
///    let subscriber = tracing_subscriber::Registry::default().with(layer);
///    subscriber.try_init().expect("logs failed to initialize");
/// ```

#[derive(Clone)]
#[must_use]
pub struct ApiTracingLayer {}

static API_LOGGER_INNER: Mutex<Option<ApiTracingLayerInner>> = Mutex::new(None);
static API_LOGGER_ENABLED: AtomicBool = AtomicBool::new(false);
/// The layer's queue. Outside `API_LOGGER_INNER` on purpose: every emitting thread touches it on
/// every event, and that mutex is held across shutdown.
static API_LOG_QUEUE: ApiLogQueue = ApiLogQueue::new();

impl ApiTracingLayer {
    /// Initialize an ApiTracingLayer singleton
    ///
    /// This must be inserted into your tracing subscriber before you
    /// call api_startup() or api_startup_json() if you are going to use api tracing.
    pub fn init() -> ApiTracingLayer {
        ApiTracingLayer {}
    }

    fn new_inner() -> ApiTracingLayerInner {
        ApiTracingLayerInner {
            update_callbacks: HashMap::new(),
        }
    }

    pub(crate) fn add_callback(
        log_key: String,
        update_callback: UpdateCallback,
    ) -> VeilidAPIResult<()> {
        let mut inner = API_LOGGER_INNER.lock();
        if inner.is_none() {
            *inner = Some(Self::new_inner());
        }
        if inner
            .as_ref()
            .unwrap_or_log()
            .update_callbacks
            .contains_key(&log_key)
        {
            apibail_already_initialized!();
        }
        inner
            .as_mut()
            .unwrap_or_log()
            .update_callbacks
            .insert(log_key, update_callback);

        API_LOGGER_ENABLED.store(true, Ordering::Release);
        API_LOG_QUEUE.start(Arc::new(Self::deliver_queued));

        Ok(())
    }

    pub(crate) fn remove_callback(log_key: String) -> VeilidAPIResult<()> {
        let mut inner = API_LOGGER_INNER.lock();
        if inner.is_none() {
            apibail_not_initialized!();
        }
        if inner
            .as_mut()
            .unwrap_or_log()
            .update_callbacks
            .remove(&log_key)
            .is_none()
        {
            apibail_not_initialized!();
        }
        if inner.as_mut().unwrap_or_log().update_callbacks.is_empty() {
            *inner = None;
            API_LOGGER_ENABLED.store(false, Ordering::Release);
            API_LOG_QUEUE.stop();
        }

        Ok(())
    }

    /// Hand one drained event to the callbacks registered for it.
    ///
    /// Resolved at delivery rather than at enqueue, so an event queued by a node that has since
    /// shut down does not call back into it.
    fn deliver_queued(log: QueuedLog) {
        let QueuedLog {
            log_key,
            log_level,
            message,
            backtrace,
        } = log;

        let update = VeilidUpdate::Log(Box::new(VeilidLog {
            log_level,
            message,
            backtrace,
        }));

        for update_cb in Self::callbacks_for(&log_key) {
            update_cb(update.clone());
        }
    }

    /// The callbacks the drain should hand a `log_key`'s events to.
    ///
    /// Cloned out under the lock and invoked by the caller without it: a callback is application
    /// code that may block indefinitely, and holding the lock across it would put shutdown behind it.
    fn callbacks_for(log_key: &str) -> Vec<UpdateCallback> {
        let Some(inner) = &*API_LOGGER_INNER.lock() else {
            return Vec::new();
        };
        if log_key.is_empty() {
            inner.update_callbacks.values().cloned().collect()
        } else {
            inner
                .update_callbacks
                .get(log_key)
                .cloned()
                .into_iter()
                .collect()
        }
    }

    fn emit_log(&self, meta: &'static Metadata<'static>, log_key: &str, message: &str) {
        // Checked before formatting (an error also walks a backtrace): a full queue would drop it anyway
        if API_LOG_QUEUE.is_full() {
            API_LOG_QUEUE.note_dropped();
            return;
        }

        let level = *meta.level();
        let target = meta.target();
        let log_level: VeilidLogLevel = level.into();

        let origin = match level {
            Level::ERROR | Level::WARN => meta
                .file()
                .and_then(|file| {
                    meta.line()
                        .map(|ln| format!("{}:{}", simplify_file(file), ln))
                })
                .unwrap_or_default(),
            Level::INFO => "".to_owned(),
            Level::DEBUG | Level::TRACE => meta
                .file()
                .and_then(|file| {
                    meta.line().map(|ln| {
                        format!(
                            "{}{}:{}",
                            if target.is_empty() {
                                "".to_owned()
                            } else {
                                format!("[{}] ", target)
                            },
                            simplify_file(file),
                            ln
                        )
                    })
                })
                .unwrap_or_default(),
        };

        // Dart can't handle the Unicode Replacement Character
        // and it causes crashes on multiple Flutter applications
        // see: https://gitlab.com/veilid/veilid/-/issues/473
        // We sanitize the logs here because it is generally a good idea to
        // ensure only valid UTF8 strings are returned to applications

        let message = format!("{} {}", origin, message)
            .trim()
            .replace(char::REPLACEMENT_CHARACTER, "");

        #[cfg(feature = "backtrace")]
        let backtrace = if log_level <= VeilidLogLevel::Error {
            let bt = Backtrace::force_capture();
            Some(bt.to_string().replace(char::REPLACEMENT_CHARACTER, ""))
        } else {
            None
        };
        #[cfg(not(feature = "backtrace"))]
        let backtrace = None;

        API_LOG_QUEUE.enqueue(QueuedLog {
            log_key: log_key.to_owned(),
            log_level,
            message,
            backtrace,
        });
    }
}

fn simplify_file(file: &'static str) -> &'static str {
    file.static_transform(|file| {
        let out = {
            let path = std::path::Path::new(file);
            let path_component_count = path.iter().count();
            if path.ends_with("mod.rs") && path_component_count >= 2 {
                let outpath: std::path::PathBuf =
                    path.iter().skip(path_component_count - 2).collect();
                outpath.to_string_lossy().to_string()
            } else if let Some(filename) = path.file_name() {
                filename.to_string_lossy().to_string()
            } else {
                file.to_string()
            }
        };
        out.to_static_str()
    })
}

impl<S: Subscriber + for<'a> registry::LookupSpan<'a>> Layer<S> for ApiTracingLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: layer::Context<'_, S>,
    ) {
        if !API_LOGGER_ENABLED.load(Ordering::Acquire) {
            // Optimization if api logger has no callbacks
            return;
        }

        let Some(span_ref) = ctx.span(id) else {
            return;
        };

        let mut new_debug_record = VeilidKeyedStringRecorder::new();
        attrs.record(&mut new_debug_record);

        if new_debug_record.log_key().is_empty() {
            if let Some(parent) = span_ref.parent() {
                let extensions = parent.extensions();
                if let Some(parent_debug_record) = extensions.get::<VeilidKeyedStringRecorder>() {
                    if !parent_debug_record.log_key().is_empty() {
                        new_debug_record.log_key = parent_debug_record.log_key().to_string();
                    }
                }
            }
        }

        let mut extensions_mut = span_ref.extensions_mut();
        extensions_mut.insert::<VeilidKeyedStringRecorder>(new_debug_record);
    }

    fn on_record(
        &self,
        id: &tracing::Id,
        values: &tracing::span::Record<'_>,
        ctx: layer::Context<'_, S>,
    ) {
        if !API_LOGGER_ENABLED.load(Ordering::Acquire) {
            // Optimization if api logger has no callbacks
            return;
        }
        let Some(span_ref) = ctx.span(id) else { return };

        let mut extensions_mut = span_ref.extensions_mut();
        if let Some(debug_record) = extensions_mut.get_mut::<VeilidKeyedStringRecorder>() {
            values.record(debug_record);
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: layer::Context<'_, S>) {
        if !API_LOGGER_ENABLED.load(Ordering::Acquire) {
            // Optimization if api logger has no callbacks
            return;
        }

        let mut event_recorder = VeilidKeyedStringRecorder::new();
        event.record(&mut event_recorder);

        let meta = event.metadata();

        // If the log key is not on the event, get it from the span this event is associated with
        if event_recorder.log_key() == "" {
            if let Some(span) = ctx.event_span(event) {
                if let Some(span_recorder) = span.extensions().get::<VeilidKeyedStringRecorder>() {
                    if span_recorder.log_key() != "" {
                        self.emit_log(
                            meta,
                            span_recorder.log_key(),
                            &format!("{} {}", event_recorder.display(), span_recorder.display()),
                        );
                        return;
                    }
                }
            }
        }
        self.emit_log(meta, event_recorder.log_key(), event_recorder.display());
    }
}

struct VeilidKeyedStringRecorder {
    log_key: String,
    display: String,
}
impl VeilidKeyedStringRecorder {
    fn new() -> Self {
        VeilidKeyedStringRecorder {
            log_key: String::new(),
            display: String::new(),
        }
    }
    fn display(&self) -> &str {
        &self.display
    }
    fn log_key(&self) -> &str {
        &self.log_key
    }
}

impl tracing::field::Visit for VeilidKeyedStringRecorder {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == VEILID_LOG_KEY_FIELD {
            self.log_key = value.to_owned();
        } else {
            self.record_debug(field, &value)
        }
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn core::fmt::Debug) {
        if field.name() == "message" {
            if !self.display.is_empty() {
                self.display = format!("{:?}\n{}", value, self.display)
            } else {
                self.display = format!("{:?}", value)
            }
        } else {
            write!(self.display, " ").unwrap_or_log();
            write!(self.display, "{} = {:?};", field.name(), value).unwrap_or_log();
        }
    }
}
