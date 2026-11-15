//! Schema v1: rows are stored as written, encrypted only.
//!
//! A v0 store LZ4-compressed every key and value before encrypting it, and the core wrote its
//! own values as bare JSON. The migration decompresses and re-encrypts every row of every column,
//! so app values come out byte for byte as they went in; `TableStore::try_deserialize` still
//! reads the bare JSON the core's own tables keep until they are rewritten. The routing table is
//! the one exception: its entry format changed with the schema, so its rows are dropped and the
//! node bootstraps once after the upgrade.
//!
//! Each table records its v0 rows in a progress key before it starts. A batch moves its rows in
//! one transaction, so an interrupted run resumes by skipping the recorded rows that are gone,
//! and the all-tables version key is written last so a crash anywhere re-runs the tables that
//! are still at v0.

use super::*;
use crate::table_store::table_db::{ENCRYPTION_KEY_HASH_KEY, MIGRATION_PROGRESS_KEY, VERSION_KEY};
use crate::table_store::{hash_table_encryption_key, maybe_decrypt, maybe_encrypt};

impl_veilid_log_facility!("tstore");

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        use keyvaluedb_web::*;
        use keyvaluedb::*;
    } else {
        use keyvaluedb_sqlite::*;
        use keyvaluedb::*;
    }
}

const MAX_KEYS_IN_BATCH: usize = 1024;
const MAX_BYTES_IN_BATCH: usize = 16 * 1024 * 1024;
const VERSION_1_TAG: &[u8] = &[1];

/// Its entry format changed with the schema, so its rows are dropped (logical name, un-namespaced)
const ROUTING_TABLE_NAME: &str = "routing_table";

pub struct V1;

impl Migration for V1 {
    async fn is_needed(
        ts: &TableStore,
        _device_encryption_key: &Option<SharedSecret>,
    ) -> VeilidAPIResult<bool> {
        let all_tables_db = open_all_tables_db(ts).await?;
        Ok(all_tables_db.get(0, VERSION_KEY).await?.is_none())
    }

    async fn migrate(
        ts: &TableStore,
        device_encryption_key: &Option<SharedSecret>,
    ) -> Result<(), MigrationError> {
        let started = Timestamp::now();
        let all_tables_db = open_all_tables_db(ts).await?;
        let all_table_names = load_all_table_names(&all_tables_db).await?;

        let max_value_size = ts.config().table_store.max_value_size_mb as usize * 1024 * 1024;
        let routing_table_name = ts.namespaced_name(ROUTING_TABLE_NAME)?;

        let mut tables = 0usize;
        let mut rows = 0usize;
        for (table_name, file_name) in &all_table_names {
            let db = open_all_columns(ts, file_name).await?;

            if db.get(0, VERSION_KEY).await?.is_some() {
                veilid_log!(ts debug "table {} ({}) already at schema v1", table_name, file_name);
                continue;
            }

            veilid_log!(ts debug "migrating table {} ({}) to schema v1", table_name, file_name);
            rows += migrate_table(
                ts,
                &db,
                table_name,
                table_name == &routing_table_name,
                device_encryption_key,
                max_value_size,
            )
            .await?;
            tables += 1;
        }

        let mut tx = DBTransaction::with_capacity(1);
        tx.put(0, VERSION_KEY, VERSION_1_TAG);
        all_tables_db
            .write(tx)
            .await
            .map_err(|e| VeilidAPIError::from(e.error))?;

        veilid_log!(ts info
            "table store migrated to schema v1: {} tables, {} rows, {:#}",
            tables, rows, TimestampDuration::since(started)
        );

        Ok(())
    }
}

/// The v0 rows of one table, recorded once before its first batch
#[derive(Debug, Serialize, Deserialize)]
struct MigrationV1Progress {
    /// (column, stored key) of each row in the v0 encoding when the table's migration began
    remaining: Vec<(u32, Vec<u8>)>,
}

async fn open_all_tables_db(ts: &TableStore) -> VeilidAPIResult<Database> {
    ts.table_store_driver
        .open("__veilid_all_tables", 1, 1)
        .await
        .inspect_err(|e| veilid_log!(ts error "failed to open all tables table: {}", e))
}

async fn load_all_table_names(
    all_tables_db: &Database,
) -> VeilidAPIResult<BTreeMap<String, String>> {
    let Some(all_table_names_json) = all_tables_db.get(0, table_store::ALL_TABLE_NAMES).await?
    else {
        return Ok(BTreeMap::new());
    };
    deserialize_json_bytes::<BTreeMap<String, String>>(&all_table_names_json)
}

/// Open a table with every column it has on disk
async fn open_all_columns(ts: &TableStore, file_name: &str) -> VeilidAPIResult<Database> {
    let db = ts.table_store_driver.open(file_name, 1, 1).await?;
    let column_count = db.num_columns().map_err(VeilidAPIError::from)?;
    if column_count <= 1 {
        return Ok(db);
    }
    drop(db);
    ts.table_store_driver.open(file_name, column_count, 1).await
}

async fn migrate_table(
    ts: &TableStore,
    db: &Database,
    table_name: &str,
    drop_rows: bool,
    device_encryption_key: &Option<SharedSecret>,
    max_value_size: usize,
) -> Result<usize, MigrationError> {
    let crypto = ts.crypto();
    // Rewriting the record with every batch would cost time quadratic in the table's rows
    let mut progress = match db.get(0, MIGRATION_PROGRESS_KEY).await? {
        Some(value) => serde_json::from_slice::<MigrationV1Progress>(&value)
            .map_err(VeilidAPIError::internal)?,
        None => {
            let progress = MigrationV1Progress {
                remaining: collect_rows(db).await?,
            };
            let progress_json = serde_json::to_vec(&progress).map_err(VeilidAPIError::internal)?;
            let mut tx = DBTransaction::with_capacity(1);
            tx.put(0, MIGRATION_PROGRESS_KEY, &progress_json);
            db.write(tx)
                .await
                .map_err(|e| VeilidAPIError::from(e.error))?;
            progress
        }
    };

    let mut migrated = 0usize;
    while !progress.remaining.is_empty() {
        let mut keys_processed = 0usize;
        let mut bytes_processed = 0usize;
        let mut tx = DBTransaction::with_capacity(MAX_KEYS_IN_BATCH * 2);

        while keys_processed < MAX_KEYS_IN_BATCH && bytes_processed < MAX_BYTES_IN_BATCH {
            let Some((col, key)) = progress.remaining.pop() else {
                break;
            };
            let key = Bytes::from(key);
            // Gone only if an interrupted run moved it: nothing else opens the table until done
            let Some(value) = db.get(col, &key).await? else {
                continue;
            };
            let value = Bytes::from(value);
            keys_processed += 1;
            bytes_processed += key.len() + value.len();

            tx.delete(col, &key);
            if drop_rows {
                continue;
            }

            // A key that does not decode is a key mismatch; a value that does not is one bad row
            let plain_key = decode_v0(&crypto, device_encryption_key, key, max_value_size)
                .await
                .map_err(|reason| MigrationError::InvalidEncryptionKey {
                    table: table_name.to_owned(),
                    reason,
                })?;
            let plain_value = match decode_v0(&crypto, device_encryption_key, value, max_value_size)
                .await
            {
                Ok(v) => v,
                Err(reason) => {
                    veilid_log!(ts warn "dropping unreadable row in table {}: {}", table_name, reason);
                    continue;
                }
            };

            let new_key = maybe_encrypt(&crypto, device_encryption_key, plain_key, true).await;
            let new_value = maybe_encrypt(&crypto, device_encryption_key, plain_value, false).await;
            tx.put_owned(col, Vec::from(new_key), Vec::from(new_value));
        }

        migrated += keys_processed;
        db.write(tx)
            .await
            .map_err(|e| VeilidAPIError::from(e.error))?;
    }

    let mut tx = DBTransaction::with_capacity(3);
    tx.delete(0, MIGRATION_PROGRESS_KEY);
    tx.put(0, VERSION_KEY, VERSION_1_TAG);
    if let Some(ek) = device_encryption_key {
        tx.put(
            0,
            ENCRYPTION_KEY_HASH_KEY,
            hash_table_encryption_key(&crypto, ek).await,
        );
    }
    db.write(tx)
        .await
        .map_err(|e| VeilidAPIError::from(e.error))?;

    Ok(migrated)
}

/// Every (column, stored key) of the table
async fn collect_rows(db: &Database) -> VeilidAPIResult<Vec<(u32, Vec<u8>)>> {
    let column_count = db.num_columns().map_err(VeilidAPIError::from)?;
    let mut rows = Vec::new();
    for col in 0..column_count {
        let (keys, _) = db
            .iter_keys(col, None, Vec::new(), |keys, key| {
                keys.push(key.to_vec());
                Ok(Option::<()>::None)
            })
            .await?;
        rows.extend(keys.into_iter().map(|key| (col, key)));
    }
    Ok(rows)
}

/// Undo the v0 encoding of a stored key or value: decrypt, then LZ4 decompress
async fn decode_v0(
    crypto: &VeilidComponentGuard<'_, Crypto>,
    device_encryption_key: &Option<SharedSecret>,
    data: Bytes,
    max_value_size: usize,
) -> Result<Bytes, String> {
    let decrypted = maybe_decrypt(crypto, device_encryption_key, data)
        .await
        .map_err(|e| e.to_string())?;
    decompress_size_prepended(&decrypted, max_value_size)
        .map(Bytes::from)
        .map_err(|e| e.to_string())
}
