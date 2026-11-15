use super::*;

#[derive(Debug)]
#[must_use]
pub struct LoadAction {
    registry: VeilidComponentRegistry,

    subkey_table: TableDB,
    subkey_table_key: SubkeyTableKey,
    opt_cached_record_data: Option<RecordData>,
    peek: bool,
    finished: bool,
}

impl LoadAction {
    pub(super) fn new(
        registry: VeilidComponentRegistry,
        subkey_table: TableDB,
        subkey_table_key: SubkeyTableKey,
        opt_cached_record_data: Option<RecordData>,
        peek: bool,
    ) -> Self {
        Self {
            registry,
            subkey_table,
            subkey_table_key,
            opt_cached_record_data,
            peek,
            finished: false,
        }
    }

    pub async fn load(&mut self) -> VeilidAPIResult<Option<RecordData>> {
        if let Some(cached_record_data) = &self.opt_cached_record_data {
            return Ok(Some(cached_record_data.clone()));
        }

        let serialized = self
            .subkey_table
            .load_serialized(0, Bytes::from(self.subkey_table_key.bytes()))
            .await?;

        let table_store = self.registry.table_store();
        let opt_data = serialized
            .map(|v| table_store.try_deserialize(&v))
            .transpose()?;

        self.opt_cached_record_data = opt_data.clone();

        Ok(opt_data)
    }

    pub(super) fn is_peek(&self) -> bool {
        self.peek
    }

    pub(super) fn into_cached_record_data(mut self) -> (SubkeyTableKey, Option<RecordData>) {
        self.finished = true;
        (
            self.subkey_table_key.clone(),
            self.opt_cached_record_data.take(),
        )
    }
}

impl Drop for LoadAction {
    fn drop(&mut self) {
        if !self.finished {
            error!(target:"stor", "LoadAction dropped without being finished: {}", self.subkey_table_key);
        }
    }
}

#[derive(Debug)]
#[must_use]
pub enum LoadActionResult {
    NoRecord,
    NoSubkey {
        descriptor: Arc<SignedValueDescriptor>,
    },
    Subkey {
        descriptor: Arc<SignedValueDescriptor>,
        load_action: LoadAction,
    },
}
