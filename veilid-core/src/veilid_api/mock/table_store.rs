use super::*;

/// A table's columns, shared by every handle opened on the same name.
#[derive(Debug, Default)]
struct MockTableInner {
    columns: Vec<HashMap<Bytes, Bytes>>,
}

impl MockTableInner {
    fn ensure_columns(&mut self, column_count: u32) {
        let column_count = column_count as usize;
        if self.columns.len() < column_count {
            self.columns.resize_with(column_count, HashMap::new);
        }
    }

    fn column(&self, col: u32) -> VeilidAPIResult<&HashMap<Bytes, Bytes>> {
        self.columns
            .get(col as usize)
            .ok_or_else(|| VeilidAPIError::generic("invalid column"))
    }

    fn column_mut(&mut self, col: u32) -> VeilidAPIResult<&mut HashMap<Bytes, Bytes>> {
        self.columns
            .get_mut(col as usize)
            .ok_or_else(|| VeilidAPIError::generic("invalid column"))
    }
}

/// A handle to an in-memory table.
///
/// Clones share one table, and the table counts as opened while any clone is alive, matching how
/// the real store refuses to delete an open table.
#[derive(Clone, Debug)]
pub struct TableDB {
    name: String,
    column_count: u32,
    inner: Arc<Mutex<MockTableInner>>,
}

impl TableDB {
    /// A handle reaches only the columns it was opened with, even when the table has since grown
    fn check_col(&self, col: u32) -> VeilidAPIResult<()> {
        if col >= self.column_count {
            apibail_generic!("invalid column");
        }
        Ok(())
    }

    /// Get the internal name of the table.
    #[must_use]
    pub fn table_name(&self) -> String {
        self.name.clone()
    }

    /// Get the total number of columns in the table.
    pub fn get_column_count(&self) -> VeilidAPIResult<u32> {
        Ok(self.column_count)
    }

    /// Estimate the storage size for a table entry.
    ///
    /// Matches the real store's estimate so callers that budget against it see the same numbers.
    pub fn estimate_storage_size(
        &self,
        _col: u32,
        key: &[u8],
        value: &[u8],
    ) -> VeilidAPIResult<u64> {
        let size = 1 + 1 + key.len() * 2 + 4 + value.len() + 4 + 4;
        size.try_into().map_err(VeilidAPIError::internal)
    }

    /// Get the list of keys in a column.
    #[expect(clippy::unused_async)]
    pub async fn get_keys(&self, col: u32) -> VeilidAPIResult<Vec<Bytes>> {
        self.check_col(col)?;
        let inner = self.inner.lock();
        Ok(inner.column(col)?.keys().cloned().collect())
    }

    /// Get the number of keys in a column.
    #[expect(clippy::unused_async)]
    pub async fn get_key_count(&self, col: u32) -> VeilidAPIResult<u64> {
        self.check_col(col)?;
        let inner = self.inner.lock();
        Ok(inner.column(col)?.len() as u64)
    }

    /// Start a transaction against this table.
    #[must_use]
    pub fn transact(&self) -> TableDBTransaction {
        TableDBTransaction::new(self.clone())
    }

    /// Store a key with a value in a column.
    #[expect(clippy::unused_async)]
    pub async fn store(&self, col: u32, key: Bytes, value: Bytes) -> VeilidAPIResult<()> {
        self.check_col(col)?;
        let mut inner = self.inner.lock();
        inner.column_mut(col)?.insert(key, value);
        Ok(())
    }

    /// Read a key from a column, or `None` if it is absent.
    #[expect(clippy::unused_async)]
    pub async fn load(&self, col: u32, key: Bytes) -> VeilidAPIResult<Option<Bytes>> {
        self.check_col(col)?;
        let inner = self.inner.lock();
        Ok(inner.column(col)?.get(&key).cloned())
    }

    /// Delete a key from a column, returning the value that was there.
    #[expect(clippy::unused_async)]
    pub async fn delete(&self, col: u32, key: Bytes) -> VeilidAPIResult<Option<Bytes>> {
        self.check_col(col)?;
        let mut inner = self.inner.lock();
        Ok(inner.column_mut(col)?.remove(&key))
    }
}

enum MockTableOp {
    Store(u32, Bytes, Bytes),
    Delete(u32, Bytes),
}

/// A batch of writes applied atomically on commit.
#[derive(Clone, Debug)]
pub struct TableDBTransaction {
    db: TableDB,
    ops: Arc<Mutex<Option<Vec<MockTableOp>>>>,
}

impl fmt::Debug for MockTableOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(col, key, _) => write!(f, "Store({}, {} bytes)", col, key.len()),
            Self::Delete(col, key) => write!(f, "Delete({}, {} bytes)", col, key.len()),
        }
    }
}

impl TableDBTransaction {
    fn new(db: TableDB) -> Self {
        Self {
            db,
            ops: Arc::new(Mutex::new(Some(Vec::new()))),
        }
    }

    fn with_ops<R>(&self, f: impl FnOnce(&mut Vec<MockTableOp>) -> R) -> VeilidAPIResult<R> {
        let mut guard = self.ops.lock();
        let ops = guard.as_mut().ok_or_else(|| {
            VeilidAPIError::generic("transaction is already committed or rolled back")
        })?;
        Ok(f(ops))
    }

    /// Apply the transaction's writes.
    #[expect(clippy::unused_async)]
    pub async fn commit(self) -> VeilidAPIResult<()> {
        let ops = self.ops.lock().take().ok_or_else(|| {
            VeilidAPIError::generic("transaction is already committed or rolled back")
        })?;

        let mut inner = self.db.inner.lock();
        for op in ops {
            match op {
                MockTableOp::Store(col, key, value) => {
                    inner.column_mut(col)?.insert(key, value);
                }
                MockTableOp::Delete(col, key) => {
                    inner.column_mut(col)?.remove(&key);
                }
            }
        }
        Ok(())
    }

    /// Discard the transaction's writes.
    pub fn rollback(self) {
        *self.ops.lock() = None;
    }

    /// Queue a key-value store.
    #[expect(clippy::unused_async)]
    pub async fn store(&self, col: u32, key: Bytes, value: Bytes) -> VeilidAPIResult<()> {
        self.db.check_col(col)?;
        self.with_ops(|ops| ops.push(MockTableOp::Store(col, key, value)))
    }

    /// Queue a key delete.
    #[expect(clippy::unused_async)]
    pub async fn delete(&self, col: u32, key: Bytes) -> VeilidAPIResult<()> {
        self.db.check_col(col)?;
        self.with_ops(|ops| ops.push(MockTableOp::Delete(col, key)))
    }
}

/// One simulated node's set of tables.
#[derive(Default)]
pub(crate) struct MockTableStore {
    tables: HashMap<String, Arc<Mutex<MockTableInner>>>,
}

/// Reject the names the real store rejects, so name validation errors surface identically.
fn check_table_name(name: &str) -> VeilidAPIResult<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        apibail_invalid_argument!("invalid table name", "name", name);
    }
    Ok(())
}

impl MockTableStore {
    /// Open a table, growing it if the caller asked for more columns than it has.
    ///
    /// Growing a table held open elsewhere would widen it under those handles, so the real store
    /// refuses and asks the caller to close first. Narrowing is fine, since the new handle reaches
    /// fewer columns.
    pub fn open(&mut self, name: &str, column_count: u32) -> VeilidAPIResult<TableDB> {
        check_table_name(name)?;
        if column_count == 0 {
            apibail_invalid_argument!("column count must be nonzero", "column_count", column_count);
        }

        let entry = self.tables.entry(name.to_owned()).or_default();
        let opened_elsewhere = Arc::strong_count(entry) > 1;
        if opened_elsewhere && column_count as usize > entry.lock().columns.len() {
            apibail_generic!("Not resizing table that is still opened");
        }
        entry.lock().ensure_columns(column_count);

        Ok(TableDB {
            name: name.to_owned(),
            column_count,
            inner: entry.clone(),
        })
    }

    pub fn delete(&mut self, name: &str) -> VeilidAPIResult<bool> {
        check_table_name(name)?;
        let Some(inner) = self.tables.get(name) else {
            return Ok(false);
        };
        // The map itself holds one reference; anything beyond that is a live handle
        if Arc::strong_count(inner) > 1 {
            apibail_generic!("Not deleting table that is still opened");
        }
        self.tables.remove(name);
        Ok(true)
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> VeilidAPIResult<()> {
        check_table_name(old_name)?;
        check_table_name(new_name)?;
        if self.tables.contains_key(new_name) {
            return Err(VeilidAPIError::generic("new table already exists"));
        }
        let Some(inner) = self.tables.remove(old_name) else {
            return Err(VeilidAPIError::generic("table does not exist"));
        };
        self.tables.insert(new_name.to_owned(), inner);
        Ok(())
    }

    pub fn list_all(&self) -> Vec<(String, String)> {
        self.tables.keys().map(|k| (k.clone(), k.clone())).collect()
    }
}
