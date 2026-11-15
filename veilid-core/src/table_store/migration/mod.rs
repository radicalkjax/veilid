//! Table store schema migrations, run once at startup before any table is opened.

use crate::*;

pub(super) mod v1;

/// Why a migration could not finish
pub(super) enum MigrationError {
    /// A row did not decrypt and decompress: the device encryption key does not fit the store
    InvalidEncryptionKey { table: String, reason: String },
    /// Any other failure; the migration resumes on the next start
    Other(VeilidAPIError),
}

impl From<VeilidAPIError> for MigrationError {
    fn from(e: VeilidAPIError) -> Self {
        Self::Other(e)
    }
}

impl From<std::io::Error> for MigrationError {
    fn from(e: std::io::Error) -> Self {
        Self::Other(VeilidAPIError::from(e))
    }
}

pub(super) trait Migration {
    /// Whether the store is at the schema version this migration starts from
    async fn is_needed(
        ts: &TableStore,
        device_encryption_key: &Option<SharedSecret>,
    ) -> VeilidAPIResult<bool>;

    /// Bring the store to this migration's schema version; only called after `is_needed`
    async fn migrate(
        ts: &TableStore,
        device_encryption_key: &Option<SharedSecret>,
    ) -> Result<(), MigrationError>;
}

pub(super) async fn migrate(
    ts: &TableStore,
    device_encryption_key: &Option<SharedSecret>,
) -> Result<(), MigrationError> {
    if v1::V1::is_needed(ts, device_encryption_key).await? {
        v1::V1::migrate(ts, device_encryption_key).await?;
    }

    Ok(())
}
