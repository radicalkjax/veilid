use std::sync::atomic::AtomicUsize;

use tracing::{Level, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use super::{
    date_now, debug1, debug4, error1, error4, log1, log4, mark, mark_name, measure, prelude::*,
    recorder::StringRecorder, thread_display_suffix, warn1, warn4,
};

#[doc = r#"
Implements [tracing_subscriber::layer::Layer] which uses [wasm_bindgen] for marking and measuring via `window.performance` and `window.console`

If composing a subscriber, provide `WasmLayer` as such:

```notest
use tracing_subscriber::prelude::*;
use tracing::Subscriber;

pub struct MySubscriber {
    // ...
}

impl Subscriber for MySubscriber {
    // ...
}

let subscriber = MySubscriber::new()
    .with(WasmLayer::default());

tracing::subscriber::set_global_default(subscriber);
```
"#]
pub struct WasmLayer {
    last_event_id: AtomicUsize,
    config: WasmLayerConfig,
    field_filter: Option<WasmFieldFilter>,
}

impl WasmLayer {
    /// Create a new [Layer] with the provided config
    #[must_use]
    pub fn new(config: WasmLayerConfig) -> Self {
        WasmLayer {
            last_event_id: AtomicUsize::new(0),
            config,
            field_filter: None,
        }
    }
    /// Filter which fields are recorded, by field name.
    #[must_use]
    pub fn with_field_filter(mut self, field_filter: Option<WasmFieldFilter>) -> Self {
        self.field_filter = field_filter;
        self
    }
}

impl Default for WasmLayer {
    fn default() -> Self {
        WasmLayer::new(WasmLayerConfig::default())
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for WasmLayer {
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _: Context<'_, S>) -> bool {
        let level = metadata.level();
        level <= &self.config.max_level
    }

    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: Context<'_, S>,
    ) {
        let mut new_debug_record = StringRecorder::new(self.config.show_fields)
            .with_field_filter(self.field_filter.clone());
        attrs.record(&mut new_debug_record);

        if let Some(span_ref) = ctx.span(id) {
            span_ref
                .extensions_mut()
                .insert::<StringRecorder>(new_debug_record);
        }
    }

    fn on_record(&self, id: &tracing::Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        if let Some(span_ref) = ctx.span(id) {
            let mut extensions = span_ref.extensions_mut();
            if let Some(debug_record) = extensions.get_mut::<StringRecorder>() {
                values.record(debug_record);
            }
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        if !self.config.enabled {
            return;
        }

        let mut recorder = StringRecorder::new(self.config.show_fields)
            .with_field_filter(self.field_filter.clone());
        event.record(&mut recorder);
        let meta = event.metadata();
        let level = meta.level();

        if self.config.report_logs_in_timings {
            let mark_name = format!(
                "c{:x}",
                self.last_event_id
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed)
            );
            let measure_name = format!(
                "{} {}{} {}",
                level,
                meta.module_path().unwrap_or("..."),
                thread_display_suffix(),
                recorder,
            );
            // mark and measure so you can see a little blip in the profile
            mark(&mark_name);
            let _ = measure(measure_name, mark_name);
        }

        if self.config.report_logs_in_console {
            let timestamp = if self.config.show_timestamp {
                // milliseconds since epoch
                let now_msecs = date_now() as u64;

                // convert time to log timestamp
                let msecs = now_msecs % 1000;
                let now_seconds = now_msecs / 1000;
                let seconds = now_seconds % 60;
                let now_minutes = now_seconds / 60;
                let minutes = now_minutes % 60;
                let now_hours = now_minutes / 60;
                let hours = now_hours % 24;
                format!("{:02}:{:02}:{:02}.{:03} ", hours, minutes, seconds, msecs)
            } else {
                String::new()
            };

            let origin = if self.config.show_origin {
                meta.file()
                    .and_then(|file| {
                        meta.line().map(|ln| {
                            format!(
                                "{}{}:{}",
                                self.config.origin_base_url.as_deref().unwrap_or_default(),
                                file,
                                ln
                            )
                        })
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };

            let fields = ctx
                .lookup_current()
                .and_then(|span| {
                    span.extensions()
                        .get::<StringRecorder>()
                        .map(|span_recorder| {
                            span_recorder
                                .fields
                                .iter()
                                .map(|(key, value)| format!("\n\t{key}: {value}"))
                                .collect::<Vec<_>>()
                                .join("")
                        })
                })
                .unwrap_or_default();
            if self.config.color {
                log_with_color(
                    format!(
                        "{}%c{}%c {}{}%c {}{}",
                        timestamp,
                        level,
                        origin,
                        thread_display_suffix(),
                        recorder,
                        fields
                    ),
                    level,
                    self.config.use_console_methods,
                );
            } else {
                log(
                    format!(
                        "{}{} {}{} {}{}",
                        timestamp,
                        level,
                        origin,
                        thread_display_suffix(),
                        recorder,
                        fields
                    ),
                    level,
                    self.config.use_console_methods,
                );
            }
        }
    }

    fn on_enter(&self, id: &tracing::Id, _ctx: Context<'_, S>) {
        if self.config.report_logs_in_timings {
            mark(&mark_name(id));
        }
    }

    fn on_exit(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        if !self.config.report_logs_in_timings {
            return;
        }

        if let Some(span_ref) = ctx.span(id) {
            let meta = span_ref.metadata();
            let measure_name_string =
                if let Some(debug_record) = span_ref.extensions().get::<StringRecorder>() {
                    format!(
                        "\"{}\"{} {} {}",
                        meta.name(),
                        thread_display_suffix(),
                        meta.module_path().unwrap_or("..."),
                        debug_record,
                    )
                } else {
                    format!(
                        "\"{}\"{} {}",
                        meta.name(),
                        thread_display_suffix(),
                        meta.module_path().unwrap_or("..."),
                    )
                };
            let _ = measure(measure_name_string, mark_name(id));
        }
    }
}

fn log(message: String, level: &Level, use_console_methods: bool) {
    if use_console_methods {
        match *level {
            Level::TRACE | Level::DEBUG => debug1(message),
            Level::INFO => log1(message),
            Level::WARN => warn1(message),
            Level::ERROR => error1(message),
        }
    } else {
        log1(message)
    }
}

fn log_with_color(message: String, level: &Level, use_console_methods: bool) {
    let level_log = if use_console_methods {
        match *level {
            Level::TRACE | Level::DEBUG => debug4,
            Level::INFO => log4,
            Level::WARN => warn4,
            Level::ERROR => error4,
        }
    } else {
        log4
    };
    level_log(
        message,
        level.color(),
        "color: gray; font-style: italic",
        "color: inherit",
    );
}

trait LevelExt {
    fn color(&self) -> &'static str;
}

impl LevelExt for Level {
    fn color(&self) -> &'static str {
        match *self {
            Level::TRACE => "color: dodgerblue; background: #444",
            Level::DEBUG => "color: lawngreen; background: #444",
            Level::INFO => "color: whitesmoke; background: #444",
            Level::WARN => "color: orange; background: #444",
            Level::ERROR => "color: red; background: #444",
        }
    }
}
