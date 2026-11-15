use super::*;

/// Keys veilid reserves for itself; applications can neither see nor write them.
pub(crate) static KNOWN_PROTECTED_STORE_KEYS: [&str; 2] = ["device_encryption_key", "_test_key"];

fn is_reserved_key(key: &str) -> bool {
    KNOWN_PROTECTED_STORE_KEYS.contains(&key)
}

fn check_key_access(key: &str) -> VeilidAPIResult<()> {
    if is_reserved_key(key) {
        apibail_invalid_argument!("protected store key is reserved by veilid", "key", key);
    }
    Ok(())
}

/// One simulated node's user secrets, held in memory rather than a keychain.
#[derive(Default)]
pub(crate) struct MockProtectedStore {
    secrets: HashMap<String, Vec<u8>>,
}

impl MockProtectedStore {
    pub fn save_user_secret(&mut self, key: &str, value: &[u8]) -> VeilidAPIResult<bool> {
        check_key_access(key)?;
        Ok(self
            .secrets
            .insert(key.to_owned(), value.to_vec())
            .is_some())
    }

    pub fn load_user_secret(&self, key: &str) -> VeilidAPIResult<Option<Vec<u8>>> {
        check_key_access(key)?;
        Ok(self.secrets.get(key).cloned())
    }

    pub fn remove_user_secret(&mut self, key: &str) -> VeilidAPIResult<bool> {
        check_key_access(key)?;
        Ok(self.secrets.remove(key).is_some())
    }

    pub fn list_user_secrets(&self) -> Vec<String> {
        self.secrets
            .keys()
            .filter(|k| !is_reserved_key(k))
            .cloned()
            .collect()
    }

    pub fn delete_all_user_secrets(&mut self) {
        self.secrets.retain(|k, _| is_reserved_key(k));
    }
}
