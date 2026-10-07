#![allow(non_snake_case)]
use super::*;

/// A handle to an opened encrypted key-value table.
#[wasm_bindgen()]
pub struct VeilidTableDB {
    inner_table_db: Option<TableDB>,
    tableName: String,
    columnCount: u32,
}

#[wasm_bindgen()]
impl VeilidTableDB {
    /// If the column count is greater than an existing TableDB's column count,
    /// the database will be upgraded to add the missing columns.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(tableName: String, columnCount: u32) -> Self {
        Self {
            inner_table_db: None,
            tableName,
            columnCount,
        }
    }

    fn getTableDB(&self) -> VeilidAPIResult<TableDB> {
        let Some(table_db) = &self.inner_table_db else {
            return VeilidAPIResult::Err(veilid_core::VeilidAPIError::generic(
                "Unable to getTableDB instance. Ensure you've called openTable().",
            ));
        };
        Ok(table_db.clone())
    }

    /// Get or create the TableDB database table.
    /// This is called automatically when performing actions on the TableDB.
    ///
    /// Holds the opened table handle on this object until `deleteTable` or the object is dropped. Re-opening when already open replaces the handle. Awaits a disk open.
    ///
    /// Throws the same error its core twin `TableStore::open` returns: `InvalidArgument` if `columnCount` is zero or `tableName` has characters other than alphanumeric, `_`, or `-`; `Generic` if the table is already open elsewhere with a smaller column count or the backing-store open fails; and `NotInitialized` if the node is not started or stored data fails to decrypt.
    pub async fn openTable(&mut self) -> VeilidAPIResult<()> {
        let veilid_api = get_veilid_api()?;
        let tstore = veilid_api.table_store()?;
        let table_db = tstore.open(&self.tableName, self.columnCount).await?;
        self.inner_table_db = Some(table_db);
        Ok(())
    }

    /// Delete this TableDB.
    ///
    /// Drops this object's open handle, then deletes the table from disk. Awaits a disk delete.
    ///
    /// Returns `false` if no table by that name exists. Throws the same error its core twin `TableStore::delete` returns: `InvalidArgument` if `tableName` has characters other than alphanumeric, `_`, or `-`; `Generic` if the table is still open elsewhere (drop all handles first) or the backing-store delete fails; and `NotInitialized` if the node is not started.
    pub async fn deleteTable(&mut self) -> VeilidAPIResult<bool> {
        self.inner_table_db = None;

        let veilid_api = get_veilid_api()?;
        let tstore = veilid_api.table_store()?;
        tstore.delete(&self.tableName).await
    }

    async fn ensureOpen(&mut self) {
        if self.inner_table_db.is_none() {
            let _ = self.openTable().await;
        }
    }

    /// Read a key from a column in the TableDB immediately.
    ///
    /// Opens the table if needed, then awaits a disk read.
    ///
    /// Returns `undefined` if the key is absent. Throws the same error its core twin `TableDB::load` returns: `Generic` if `columnId` is at or above the opened column count, the backing-store read fails, or the stored value fails to decrypt or decompress. Throws `Generic` if the table could not be opened, and `NotInitialized` if the node is not started.
    pub async fn load(
        &mut self,
        columnId: u32,
        key: Box<[u8]>,
    ) -> VeilidAPIResult<Option<Uint8Array>> {
        self.ensureOpen().await;
        let table_db = self.getTableDB()?;

        let out = table_db.load(columnId, &key).await?;
        let out = out.map(|out| Uint8Array::from(out.as_slice()));
        Ok(out)
    }

    /// Store a key with a value in a column in the TableDB.
    /// Performs a single transaction immediately.
    ///
    /// Opens the table if needed, then awaits a disk write.
    ///
    /// Throws the same error its core twin `TableDB::store` returns: `Generic` if `columnId` is at or above the opened column count or the backing-store write fails. Throws `Generic` if the table could not be opened, and `NotInitialized` if the node is not started.
    pub async fn store(
        &mut self,
        columnId: u32,
        key: Box<[u8]>,
        value: Box<[u8]>,
    ) -> VeilidAPIResult<()> {
        self.ensureOpen().await;
        let table_db = self.getTableDB()?;

        table_db.store(columnId, &key, &value).await
    }

    /// Delete key with from a column in the TableDB.
    ///
    /// Opens the table if needed, then awaits a disk write.
    ///
    /// Returns `undefined` if the key was absent. Throws the same error its core twin `TableDB::delete` returns: `Generic` if `columnId` is at or above the opened column count, the backing-store delete fails, or the prior value fails to decrypt or decompress. Throws `Generic` if the table could not be opened, and `NotInitialized` if the node is not started.
    pub async fn delete(
        &mut self,
        columnId: u32,
        key: Box<[u8]>,
    ) -> VeilidAPIResult<Option<Uint8Array>> {
        self.ensureOpen().await;
        let table_db = self.getTableDB()?;

        let out = table_db.delete(columnId, &key).await?;
        let out = out.map(|out| Uint8Array::from(out.as_slice()));
        Ok(out)
    }

    /// Get the list of keys in a column of the TableDB.
    ///
    /// Returns an array of Uint8Array keys.
    ///
    /// Opens the table if needed, then awaits a disk read.
    ///
    /// Throws the same error its core twin `TableDB::get_keys` returns: `Generic` if `columnId` is at or above the opened column count, the backing-store read fails, or a stored key fails to decrypt or decompress. Throws `Generic` if the table could not be opened, and `NotInitialized` if the node is not started.
    pub async fn getKeys(&mut self, columnId: u32) -> VeilidAPIResult<Uint8ArrayArray> {
        self.ensureOpen().await;
        let table_db = self.getTableDB()?;

        let keys = table_db.clone().get_keys(columnId).await?;
        let out: Vec<Uint8Array> = keys
            .into_iter()
            .map(|k| Uint8Array::from(k.as_slice()))
            .collect();

        let out = into_unchecked_uint8array_array(out);

        Ok(out)
    }

    /// Start a TableDB write transaction.
    /// The transaction object must be committed or rolled back before dropping.
    ///
    /// Release the returned handle via [VeilidTableDBTransaction::commit] or [VeilidTableDBTransaction::rollback]; dropping it without either logs an error. Local, does not block.
    ///
    /// Throws `Generic` if the table could not be opened, and `NotInitialized` if the node is not started.
    pub async fn createTransaction(&mut self) -> VeilidAPIResult<VeilidTableDBTransaction> {
        self.ensureOpen().await;
        let table_db = self.getTableDB()?;

        let transaction = table_db.transact();
        Ok(VeilidTableDBTransaction {
            inner_transaction: Some(transaction),
        })
    }
}

/// A TableDB write transaction. Atomically commits a group of writes or deletes to the TableDB.
#[wasm_bindgen]
pub struct VeilidTableDBTransaction {
    inner_transaction: Option<TableDBTransaction>,
}

#[wasm_bindgen]
impl VeilidTableDBTransaction {
    fn getTransaction(&self) -> VeilidAPIResult<TableDBTransaction> {
        let Some(transaction) = &self.inner_transaction else {
            return VeilidAPIResult::Err(veilid_core::VeilidAPIError::generic(
                "Unable to getTransaction instance. inner_transaction is None.",
            ));
        };
        Ok(transaction.clone())
    }

    /// Commit the transaction. Performs all actions atomically.
    ///
    /// The release for [VeilidTableDB::createTransaction] (the other half is [VeilidTableDBTransaction::rollback]). An empty transaction commits as a no-op; otherwise blocks on the serialized commit lock and the disk write. Calling commit or rollback again errors with "transaction already completed".
    ///
    /// Throws the same error its core twin `TableDBTransaction::commit` returns: `Generic` if this transaction was already committed or rolled back, or if the atomic backing-store write fails (the buffered writes are then lost).
    pub async fn commit(&self) -> VeilidAPIResult<()> {
        let transaction = self.getTransaction()?;
        transaction.commit().await
    }

    /// Rollback the transaction. Does nothing to the TableDB.
    ///
    /// The release for [VeilidTableDB::createTransaction] (the other half is [VeilidTableDBTransaction::commit]). Discards the buffered writes locally without blocking.
    #[allow(clippy::unused_async)]
    pub async fn rollback(&self) -> VeilidAPIResult<()> {
        let transaction = self.getTransaction()?;
        transaction.rollback();
        Ok(())
    }

    /// Store a key with a value in a column in the TableDB.
    /// Does not modify TableDB until `.commit()` is called.
    ///
    /// Throws the same error its core twin `TableDBTransaction::store` returns: `Generic` if `col` is at or above the opened column count, or if this transaction was already committed or rolled back.
    pub async fn store(&self, col: u32, key: Box<[u8]>, value: Box<[u8]>) -> VeilidAPIResult<()> {
        let transaction = self.getTransaction()?;
        transaction.store(col, &key, &value).await
    }

    /// Delete key with from a column in the TableDB
    /// Does not modify TableDB until `.commit()` is called.
    ///
    /// Throws the same error its core twin `TableDBTransaction::delete` returns: `Generic` if `col` is at or above the opened column count, or if this transaction was already committed or rolled back.
    pub async fn deleteKey(&self, col: u32, key: Box<[u8]>) -> VeilidAPIResult<()> {
        let transaction = self.getTransaction()?;
        transaction.delete(col, &key).await
    }
}
