use crate::*;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        use keyvaluedb_web::*;
        use keyvaluedb::*;
    } else {
        use keyvaluedb_sqlite::*;
        use keyvaluedb::*;
    }
}

impl_veilid_log_facility!("tstore");

pub(super) const VERSION_KEY: &[u8] = b"__veilid_schema_version";
pub(super) const CURRENT_SCHEMA_VERSION: &[u8] = &[1];

pub(super) const ENCRYPTION_KEY_HASH_KEY: &[u8] = b"__veilid_encryption_key_hash";
pub(super) const MIGRATION_PROGRESS_KEY: &[u8] = b"__veilid_schema_migration_progress";
/// Keys the table store keeps in column 0 of every table, unencrypted
pub(super) const PLAINTEXT_INTERNAL_KEYS: &[&[u8]] =
    &[VERSION_KEY, ENCRYPTION_KEY_HASH_KEY, MIGRATION_PROGRESS_KEY];

/// Reject a stored key that would collide with one the table store keeps in plaintext.
///
/// Takes the key as stored, after encryption, since that is what sits next to the internal keys.
pub(crate) fn check_plaintext_internal_key(key: &[u8]) -> VeilidAPIResult<()> {
    for k in PLAINTEXT_INTERNAL_KEYS {
        if key == *k {
            return Err(VeilidAPIError::internal("internal TableDB key"));
        }
    }

    Ok(())
}

#[must_use]
#[derive(Clone, Debug)]
struct CryptInfo {
    secret: SharedSecret,
}
impl CryptInfo {
    pub fn new(secret: SharedSecret) -> Self {
        Self { secret }
    }
}

/// Shared state behind a `TableDB`: the open database, commit serialization lock, and encryption keys.
#[must_use]
pub(super) struct TableDBUnlockedInner {
    registry: VeilidComponentRegistry,
    table: String,
    database: Database,
    // Lock to serialize commits so they don't cause SQLITE_BUSY or similar errors
    commit_lock: AsyncMutex<()>,
    // Encryption and decryption key will be the same unless configured for an in-place migration
    encrypt_info: Option<CryptInfo>,
    decrypt_info: Option<CryptInfo>,
}

impl fmt::Debug for TableDBUnlockedInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TableDBUnlockedInner(table={})", self.table)
    }
}

/// A handle to an opened encrypted key-value table. Cheap to clone; clones share the same underlying database.
#[derive(Debug, Clone)]
#[must_use]
pub struct TableDB {
    opened_column_count: u32,
    unlocked_inner: Arc<TableDBUnlockedInner>,
}

impl VeilidComponentRegistryAccessor for TableDB {
    fn registry(&self) -> VeilidComponentRegistry {
        self.unlocked_inner.registry.clone()
    }
}

impl TableDB {
    pub(super) fn new(
        table: String,
        registry: VeilidComponentRegistry,
        database: Database,
        encryption_key: Option<SharedSecret>,
        decryption_key: Option<SharedSecret>,
        opened_column_count: u32,
    ) -> Self {
        let encrypt_info = encryption_key.map(CryptInfo::new);
        let decrypt_info = decryption_key.map(CryptInfo::new);

        let total_columns = database.num_columns().unwrap_or_log();

        Self {
            opened_column_count: if opened_column_count == 0 {
                total_columns
            } else {
                opened_column_count
            },
            unlocked_inner: Arc::new(TableDBUnlockedInner {
                registry,
                table,
                database,
                commit_lock: AsyncMutex::new(()),
                encrypt_info,
                decrypt_info,
            }),
        }
    }

    pub(super) fn new_from_unlocked_inner(
        unlocked_inner: Arc<TableDBUnlockedInner>,
        opened_column_count: u32,
    ) -> Self {
        let db = &unlocked_inner.database;
        let total_columns = db.num_columns().unwrap_or_log();
        Self {
            opened_column_count: if opened_column_count == 0 {
                total_columns
            } else {
                opened_column_count
            },
            unlocked_inner,
        }
    }

    pub(super) fn unlocked_inner(&self) -> Arc<TableDBUnlockedInner> {
        self.unlocked_inner.clone()
    }

    /// Get the internal name of the table
    #[must_use]
    pub fn table_name(&self) -> String {
        self.unlocked_inner.table.clone()
    }

    /// Get the io stats for the table
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    #[must_use]
    pub fn io_stats(&self, kind: IoStatsKind) -> IoStats {
        self.unlocked_inner.database.io_stats(kind)
    }

    /// Cleanup the database
    ///
    /// Blocks on on-disk database maintenance (vacuum).
    ///
    /// Errors with `VeilidAPIError::Internal` if the backing-store vacuum fails.
    pub async fn cleanup(&self) -> VeilidAPIResult<()> {
        self.unlocked_inner
            .database
            .cleanup()
            .measure_debug(
                TimestampDuration::new_secs(1),
                veilid_log_dbg!(self, "TableDB::cleanup {}", self.table_name()),
            )
            .await
            .map_err(VeilidAPIError::internal)
    }

    /// Get the total number of columns in the TableDB.
    /// Not the number of columns that were opened, rather the total number that could be opened.
    ///
    /// Errors with `VeilidAPIError::Generic` if the backing store fails to report its column count.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub fn get_column_count(&self) -> VeilidAPIResult<u32> {
        let db = &self.unlocked_inner.database;
        db.num_columns().map_err(VeilidAPIError::from)
    }

    /// Estimate the storage size for a table entry
    /// Rough guess for sqlite based on their file format. Other databases may vary.
    ///
    /// Infallible on all supported targets (the `usize`→`u64` conversion only widens).
    pub fn estimate_storage_size(
        &self,
        _col: u32,
        key: &[u8],
        value: &[u8],
    ) -> VeilidAPIResult<u64> {
        // TODO
        let size =
            // Count of fields byte
            1 +
            // Type of field byte
            1 +
            // Length of key times two because it uses hex encoding sometimes
            key.len() * 2 +
            // Length of key length
            4 +
            // Length of value
            value.len() +
            // Length of value length
            4 +
            // Extra padding for max length and whatever else
            // XXX: at some point we should measure this on disk to figure out a better estimate :P
            4;
        // TODO
        size.try_into().map_err(VeilidAPIError::internal)
    }

    /// Get the list of keys in a column of the TableDB
    ///
    /// Blocks on on-disk reads.
    ///
    /// Errors with `VeilidAPIError::Generic` if `col` is at or above the opened column count, if the backing-store read fails, or if a stored key is too short to decrypt (corrupt data).
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub async fn get_keys(&self, col: u32) -> VeilidAPIResult<Vec<Bytes>> {
        if col >= self.opened_column_count {
            apibail_generic!(
                "Column exceeds opened column count {} >= {}",
                col,
                self.opened_column_count
            );
        }
        let db = self.unlocked_inner.database.clone();
        let out = Vec::new();
        let (out, _) = db
            .iter_keys(col, None, out, |out, ekey| {
                out.push(Bytes::copy_from_slice(ekey));
                Ok(Option::<()>::None)
            })
            .await
            .map_err(VeilidAPIError::from)?;

        #[cfg(feature = "verbose-tracing")]
        veilid_log!(self debug "TableDB::get_keys({}) col={}: read {} raw keys", self.unlocked_inner.table, col, out.len());

        let crypto = self.crypto();
        let dk = &self.unlocked_inner.decrypt_info.clone().map(|ei| ei.secret);

        let mut keys = Vec::with_capacity(out.len());
        for k in out {
            if check_plaintext_internal_key(&k).is_ok() {
                keys.push(table_store::maybe_decrypt(&crypto, dk, k).await?);
            }
        }

        Ok(keys)
    }

    /// Get the number of keys in a column of the TableDB
    ///
    /// Blocks on on-disk reads.
    ///
    /// Errors with `VeilidAPIError::Generic` if `col` is at or above the opened column count or the backing-store read fails.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub async fn get_key_count(&self, col: u32) -> VeilidAPIResult<u64> {
        if col >= self.opened_column_count {
            apibail_generic!(
                "Column exceeds opened column count {} >= {}",
                col,
                self.opened_column_count
            );
        }
        let db = self.unlocked_inner.database.clone();
        let mut key_count = db.num_keys(col).await.map_err(VeilidAPIError::from)?;
        if col == 0 {
            for internal_key in PLAINTEXT_INTERNAL_KEYS {
                if db
                    .has_key(0, internal_key)
                    .await
                    .map_err(VeilidAPIError::from)?
                {
                    key_count = key_count.saturating_sub(1);
                }
            }
        }
        Ok(key_count)
    }

    /// Start a TableDB write transaction. The transaction object must be committed or rolled back before dropping.
    ///
    /// Returns a handle whose `commit` or `rollback` the caller must call; dropping it uncompleted logs an error and silently discards the writes.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    #[must_use]
    pub fn transact(&self) -> TableDBTransaction {
        let dbt = self.unlocked_inner.database.transaction();
        TableDBTransaction::new(self.clone(), dbt)
    }

    /// Store a key with a value in a column in the TableDB. Performs a single transaction immediately.
    ///
    /// Blocks on the on-disk write.
    ///
    /// Errors with `VeilidAPIError::Generic` if `col` is at or above the opened column count or the backing-store write fails.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub async fn store(&self, col: u32, key: Bytes, value: Bytes) -> VeilidAPIResult<()> {
        if col >= self.opened_column_count {
            apibail_generic!(
                "Column exceeds opened column count {} >= {}",
                col,
                self.opened_column_count
            );
        }

        let crypto = self.crypto();
        let ek = &self.unlocked_inner.encrypt_info.clone().map(|ei| ei.secret);
        let encrypted_key = table_store::maybe_encrypt(&crypto, ek, key, true).await;
        check_plaintext_internal_key(&encrypted_key)?;
        let encrypted_value = table_store::maybe_encrypt(&crypto, ek, value, false).await;

        let db = self.unlocked_inner.database.clone();
        let mut dbt = db.transaction();
        dbt.put_owned(col, Vec::from(encrypted_key), Vec::from(encrypted_value));
        db.write(dbt)
            .await
            .map_err(|e| VeilidAPIError::generic(format!("failed to store: {}", e)))
    }

    /// Store a key in serialized format with a value in a column in the TableDB. Performs a single transaction immediately.
    ///
    /// Blocks on the on-disk write.
    ///
    /// Returns same errors as `store`.
    #[instrument(level = "trace", target = "tstore", skip_all)]
    pub(crate) async fn store_serialized(
        &self,
        col: u32,
        key: Bytes,
        value: TableDBSerializedData,
    ) -> VeilidAPIResult<()> {
        self.store(col, key, value.0).await
    }

    /// Read a key from a column in the TableDB immediately.
    ///
    /// Blocks on the on-disk read.
    ///
    /// Returns `Ok(None)` if the key is absent. Errors with `VeilidAPIError::Generic` if `col` is at or above the opened column count, if the backing-store read fails, or if the stored value is too short to decrypt (corrupt data).
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub async fn load(&self, col: u32, key: Bytes) -> VeilidAPIResult<Option<Bytes>> {
        if col >= self.opened_column_count {
            apibail_generic!(
                "Column exceeds opened column count {} >= {}",
                col,
                self.opened_column_count
            );
        }

        let crypto = self.crypto();
        let ek = &self.unlocked_inner.encrypt_info.clone().map(|ei| ei.secret);
        let dk = &self.unlocked_inner.decrypt_info.clone().map(|di| di.secret);
        let encrypted_key = table_store::maybe_encrypt(&crypto, ek, key, true).await;
        check_plaintext_internal_key(&encrypted_key)?;

        let db = self.unlocked_inner.database.clone();
        match db
            .get(col, &encrypted_key)
            .await
            .map_err(VeilidAPIError::from)?
        {
            Some(v) => Ok(Some(
                table_store::maybe_decrypt(&crypto, dk, Bytes::from(v)).await?,
            )),
            None => Ok(None),
        }
    }

    /// Read a serialized value from a column in the TableDB immediately
    ///
    /// Blocks on the on-disk read.
    ///
    /// Returns same errors as `load`.
    #[instrument(level = "trace", target = "tstore", skip_all)]
    pub(crate) async fn load_serialized(
        &self,
        col: u32,
        key: Bytes,
    ) -> VeilidAPIResult<Option<TableDBSerializedData>> {
        self.load(col, key)
            .await
            .map(|opt_value| opt_value.map(TableDBSerializedData))
    }

    /// Delete key with from a column in the TableDB
    ///
    /// Blocks on the on-disk write.
    ///
    /// Returns `Ok(None)` if the key was absent. Errors with `VeilidAPIError::Generic` if `col` is at or above the opened column count, if the backing-store delete fails, or if the prior value is too short to decrypt (corrupt data).
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub async fn delete(&self, col: u32, key: Bytes) -> VeilidAPIResult<Option<Bytes>> {
        if col >= self.opened_column_count {
            apibail_generic!(
                "Column exceeds opened column count {} >= {}",
                col,
                self.opened_column_count
            );
        }

        let crypto = self.crypto();
        let ek = &self.unlocked_inner.encrypt_info.clone().map(|ei| ei.secret);
        let dk = &self.unlocked_inner.decrypt_info.clone().map(|di| di.secret);
        let encrypted_key = table_store::maybe_encrypt(&crypto, ek, key, true).await;
        check_plaintext_internal_key(&encrypted_key)?;

        let db = self.unlocked_inner.database.clone();

        match db
            .delete(col, &encrypted_key)
            .await
            .map_err(VeilidAPIError::from)?
        {
            Some(v) => Ok(Some(
                table_store::maybe_decrypt(&crypto, dk, Bytes::from(v)).await?,
            )),
            None => Ok(None),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

struct TableDBTransactionInner {
    registry: VeilidComponentRegistry,
    dbt: Option<DBTransaction>,
}

impl fmt::Debug for TableDBTransactionInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TableDBTransactionInner({})",
            match &self.dbt {
                Some(dbt) => format!("len={}", dbt.ops.len()),
                None => "".to_owned(),
            }
        )
    }
}

impl Drop for TableDBTransactionInner {
    fn drop(&mut self) {
        if self.dbt.is_some() {
            let registry = &self.registry;
            veilid_log!(registry error "Dropped transaction without commit or rollback");
        }
    }
}

/// A TableDB transaction
/// Atomically commits a group of writes or deletes to the TableDB
#[derive(Debug, Clone)]
pub struct TableDBTransaction {
    db: TableDB,
    inner: Arc<Mutex<TableDBTransactionInner>>,
}

impl VeilidComponentRegistryAccessor for TableDBTransaction {
    fn registry(&self) -> VeilidComponentRegistry {
        self.db.registry()
    }
}

impl TableDBTransaction {
    fn new(db: TableDB, dbt: DBTransaction) -> Self {
        let registry = db.registry();
        Self {
            db,
            inner: Arc::new(Mutex::new(TableDBTransactionInner {
                registry,
                dbt: Some(dbt),
            })),
        }
    }

    /// Commit the transaction. Performs all actions atomically.
    ///
    /// Consumes the transaction handle; committing an already-completed clone errors with "transaction already completed". An empty transaction commits as a no-op. Blocks on the serialized commit lock and the on-disk write.
    ///
    /// Errors with `VeilidAPIError::Generic` if this transaction (or a clone) was already committed or rolled back, or if the atomic backing-store write fails (the buffered writes are then lost).
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub async fn commit(self) -> VeilidAPIResult<()> {
        let dbt = {
            let mut inner = self.inner.lock();
            inner
                .dbt
                .take()
                .ok_or_else(|| VeilidAPIError::generic("transaction already completed"))?
        };

        if dbt.ops.is_empty() {
            // Empty transactions are effectively rollbacks, so just return
            return Ok(());
        }

        let db = self.db.unlocked_inner.database.clone();
        let _commit_lock = self
            .db
            .unlocked_inner
            .commit_lock
            .lock()
            .measure_debug(
                TimestampDuration::new_ms(200),
                veilid_log_dbg!(
                    self,
                    "TableDBTransaction({})::commit lock",
                    self.db.table_name()
                ),
            )
            .await;
        db.write(dbt).await.map_err(|e| {
            veilid_log!(self error "commit failed, transaction lost: {:?}", e);
            VeilidAPIError::generic(format!("commit failed, transaction lost: {}", e))
        })
    }

    /// Rollback the transaction. Does nothing to the TableDB.
    ///
    /// Consumes the transaction handle and discards the buffered writes locally without blocking.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub fn rollback(self) {
        let mut inner = self.inner.lock();
        inner.dbt = None;
    }

    /// Store a key with a value in a column in the TableDB
    ///
    /// Buffers the write into the transaction without touching disk; errors if the transaction is already committed or rolled back.
    ///
    /// Errors with `VeilidAPIError::Generic` if `col` is at or above the opened column count, or if this transaction (or a clone) was already committed or rolled back.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub async fn store(&self, col: u32, key: Bytes, value: Bytes) -> VeilidAPIResult<()> {
        if col >= self.db.opened_column_count {
            apibail_generic!(
                "Column exceeds opened column count {} >= {}",
                col,
                self.db.opened_column_count
            );
        }

        let crypto = self.db.crypto();
        let ek = &self
            .db
            .unlocked_inner
            .encrypt_info
            .clone()
            .map(|ei| ei.secret);
        let key = table_store::maybe_encrypt(&crypto, ek, key, true).await;
        check_plaintext_internal_key(&key)?;
        let value = table_store::maybe_encrypt(&crypto, ek, value, false).await;
        let mut inner = self.inner.lock();
        inner
            .dbt
            .as_mut()
            .ok_or_else(|| VeilidAPIError::generic("store failed, transaction already completed"))?
            .put_owned(col, Vec::from(key), Vec::from(value));
        Ok(())
    }

    /// Store a key in serialized format with a value in a column in the TableDB
    ///
    /// Buffers the write into the transaction without touching disk; errors if the transaction is already committed or rolled back.
    ///
    /// Returns same errors as `store`.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub(crate) async fn store_serialized(
        &self,
        col: u32,
        key: Bytes,
        value: TableDBSerializedData,
    ) -> VeilidAPIResult<()> {
        self.store(col, key, value.0).await
    }

    /// Delete key with from a column in the TableDB
    ///
    /// Buffers the delete into the transaction without touching disk; errors if the transaction is already committed or rolled back.
    ///
    /// Errors with `VeilidAPIError::Generic` if `col` is at or above the opened column count, or if this transaction (or a clone) was already committed or rolled back.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "tstore", skip_all)
    )]
    pub async fn delete(&self, col: u32, key: Bytes) -> VeilidAPIResult<()> {
        if col >= self.db.opened_column_count {
            apibail_generic!(
                "Column exceeds opened column count {} >= {}",
                col,
                self.db.opened_column_count
            );
        }

        let crypto = self.db.crypto();
        let ek = &self
            .db
            .unlocked_inner
            .encrypt_info
            .clone()
            .map(|ei| ei.secret);
        let key = table_store::maybe_encrypt(&crypto, ek, key, true).await;
        check_plaintext_internal_key(&key)?;
        let mut inner = self.inner.lock();
        inner
            .dbt
            .as_mut()
            .ok_or_else(|| VeilidAPIError::generic("delete failed, transaction already completed"))?
            .delete_owned(col, Vec::from(key));
        Ok(())
    }
}
