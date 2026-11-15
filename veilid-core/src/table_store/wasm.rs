use super::*;
pub use keyvaluedb_web::*;

/// No cross-instance guard on wasm: browser storage is origin-scoped and
/// there is no process to crash-release a lock from
#[must_use]
pub(in crate::table_store) struct TableStoreLock;

#[derive(Clone)]
#[must_use]
pub(in crate::table_store) struct TableStoreDriver {
    registry: VeilidComponentRegistry,
}

impl_veilid_component_accessors!(TableStoreDriver);

impl TableStoreDriver {
    pub fn new(registry: VeilidComponentRegistry) -> Self {
        Self { registry }
    }

    fn get_namespaced_table_name(&self, table: &str) -> String {
        let config = self.registry().config();
        let namespace = config.namespace.clone();
        if namespace.is_empty() {
            table.to_owned()
        } else {
            format!("{}_{}", namespace, table)
        }
    }

    #[expect(clippy::unused_async)]
    pub async fn acquire_namespace_lock(&self) -> VeilidAPIResult<Option<TableStoreLock>> {
        Ok(Some(TableStoreLock))
    }

    pub async fn open(
        &self,
        table_name: &str,
        column_count: u32,
        _concurrency: usize,
    ) -> VeilidAPIResult<Database> {
        let namespaced_table_name = self.get_namespaced_table_name(table_name);
        let db = Database::open(&namespaced_table_name, column_count, false)
            .await
            .map_err(|e| VeilidAPIError::generic(format!("failed to open table store: {}", e)))?;

        veilid_log!(self trace
            "opened table store '{}' with {} columns",
            namespaced_table_name,
            column_count
        );
        Ok(db)
    }

    /// Delete a TableDB table by name. Returns whether one was removed.
    pub async fn delete(&self, table_name: &str) -> VeilidAPIResult<bool> {
        if is_browser() {
            let namespaced_table_name = self.get_namespaced_table_name(table_name);
            let out = Database::delete(&namespaced_table_name)
                .await
                .map_err(|e| {
                    VeilidAPIError::generic(format!(
                        "failed to delete table '{}': {}",
                        namespaced_table_name, e
                    ))
                })?;
            if out {
                veilid_log!(self trace "TableStore::delete {} deleted", namespaced_table_name);
            } else {
                veilid_log!(self debug "TableStore::delete {} not deleted", namespaced_table_name);
            }
            Ok(out)
        } else {
            unimplemented!();
        }
    }

    /// Delete every table in the currently configured namespace.
    pub async fn delete_all_in_namespace(&self) -> VeilidAPIResult<usize> {
        let namespace = self.config().namespace.clone();
        let opt_prefix = if namespace.is_empty() {
            None
        } else {
            Some(format!("{}_", namespace))
        };

        let entries = Database::list(opt_prefix.as_deref())
            .await
            .map_err(VeilidAPIError::from)?;

        // A bare prefix match is not enough: the default namespace's empty prefix matches
        // everything, and namespace "a" prefix-matches namespace "a_b"'s tables
        let prefix = opt_prefix.unwrap_or_default();
        fn owned_by_namespace(name: &str, prefix: &str) -> bool {
            let Some(rest) = name.strip_prefix(prefix) else {
                return false;
            };
            if rest == "__veilid_all_tables" {
                return true;
            }
            // Table real-names are 32 random bytes as base64url: 43 chars
            rest.len() == 43
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        }

        let mut deleted = 0usize;
        for (name, _ver) in entries {
            if !owned_by_namespace(&name, &prefix) {
                continue;
            }
            match Database::delete(&name).await {
                Ok(removed) => {
                    if removed {
                        deleted += 1;
                    }
                    veilid_log!(self debug "delete_all_in_namespace: removed '{}'", name);
                }
                Err(e) => {
                    veilid_log!(self warn "delete_all_in_namespace: failed to remove '{}': {}", name, e);
                }
            }
        }
        Ok(deleted)
    }
}
