use super::*;

/// The state associated with a local record when it is opened
/// This is not serialized to storage as it is ephemeral for the lifetime of the opened record
#[derive(Clone, Debug)]
pub(in crate::storage_manager) struct OpenedRecord {
    /// Key pair signing subkey writes on this record; without it every set_value() fails
    writer: Option<KeyPair>,

    /// The safety selection in current use
    safety_selection: SafetySelection,

    /// Encryption key, for newer records
    encryption_key: Option<BareSharedSecret>,

    /// Opened by a background transaction, not the application; a foreground open preempts it
    is_background: bool,
}

impl OpenedRecord {
    pub fn new(
        writer: Option<KeyPair>,
        safety_selection: SafetySelection,
        encryption_key: Option<BareSharedSecret>,
    ) -> Self {
        Self {
            writer,
            safety_selection,
            encryption_key,
            is_background: false,
        }
    }

    pub fn new_background(
        writer: Option<KeyPair>,
        safety_selection: SafetySelection,
        encryption_key: Option<BareSharedSecret>,
    ) -> Self {
        Self {
            writer,
            safety_selection,
            encryption_key,
            is_background: true,
        }
    }

    pub fn is_background(&self) -> bool {
        self.is_background
    }

    pub fn writer(&self) -> Option<&KeyPair> {
        self.writer.as_ref()
    }
    pub fn set_writer(&mut self, writer: Option<KeyPair>) {
        self.writer = writer;
    }

    pub fn safety_selection(&self) -> SafetySelection {
        self.safety_selection.clone()
    }
    pub fn set_safety_selection(&mut self, safety_selection: SafetySelection) {
        self.safety_selection = safety_selection;
    }

    pub fn encryption_key(&self) -> Option<BareSharedSecret> {
        self.encryption_key.clone()
    }
    pub fn set_encryption_key(&mut self, encryption_key: Option<BareSharedSecret>) {
        self.encryption_key = encryption_key;
    }
}
