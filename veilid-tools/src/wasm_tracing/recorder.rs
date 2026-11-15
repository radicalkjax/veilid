use core::{
    default::Default,
    fmt::{self, Write},
};
use std::collections::HashMap;
use std::sync::Arc;

use tracing::field::{Field, Visit};

/// A filter for the WasmLayer that can be used to remove fields from the output
pub type WasmFieldFilter = Arc<dyn Fn(&'static str) -> bool + Send + Sync>;

pub struct StringRecorder {
    pub display: String,
    pub is_following_args: bool,
    pub show_fields: bool,
    pub fields: HashMap<String, String>,
    pub field_filter: Option<WasmFieldFilter>,
}
impl StringRecorder {
    pub fn new(show_fields: bool) -> Self {
        StringRecorder {
            show_fields,
            ..Default::default()
        }
    }
    pub fn with_field_filter(mut self, field_filter: Option<WasmFieldFilter>) -> Self {
        self.field_filter = field_filter;
        self
    }
}

impl Visit for StringRecorder {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if let Some(f) = &self.field_filter {
            if !f(field.name()) {
                return;
            }
        }

        if field.name() == "message" {
            if !self.display.is_empty() {
                self.display = format!("{value:?}\n{}", self.display)
            } else {
                self.display = format!("{value:?}")
            }
        } else if self.show_fields {
            if self.is_following_args {
                // following args
                writeln!(self.display).unwrap();
            } else {
                // first arg
                write!(self.display, " ").unwrap();
                self.is_following_args = true;
            }
            write!(self.display, "{} = {:?};", field.name(), value).unwrap();
            self.fields
                .insert(field.name().to_owned(), format!("{value:?}"));
        }
    }
}

impl core::fmt::Display for StringRecorder {
    fn fmt(&self, mut f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if !self.display.is_empty() {
            write!(&mut f, "{}", self.display)
        } else {
            Ok(())
        }
    }
}

impl core::default::Default for StringRecorder {
    fn default() -> Self {
        Self {
            display: String::new(),
            is_following_args: false,
            show_fields: true,
            fields: HashMap::new(),
            field_filter: None,
        }
    }
}
