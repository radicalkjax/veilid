use super::*;

use core::fmt::Debug;
use core::hash::Hash;
use hashlink::LruCache;

/// A bounded keyed cache where each key has its own async lock.
///
/// [`lookup`](AsyncKeyedCache::lookup) acquires the key's lock and returns an
/// [`AsyncKeyedCacheEntry`] that holds it for the entry's lifetime. While the entry is
/// held, `get`/`insert`/`remove`/`take` operate on that key's slot atomically with
/// respect to other lookups of the *same* key; other keys are never blocked.
///
/// This encapsulates the common "lock the key, check the cache, maybe fill it, all
/// without racing another filler" pattern in one place, so the lock and the cache can't
/// drift out of sync.
#[derive(Clone, Debug)]
pub struct AsyncKeyedCache<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone,
{
    lock_table: AsyncTagLockTable<K>,
    cache: Arc<Mutex<LruCache<K, V>>>,
}

impl<K, V> AsyncKeyedCache<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone,
{
    /// Create a new keyed cache holding at most `capacity` entries (LRU eviction).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            lock_table: AsyncTagLockTable::new(),
            cache: Arc::new(Mutex::new(LruCache::new(capacity))),
        }
    }

    /// Lock `key` and return an entry for operating on its cache slot. The key stays
    /// locked until the returned entry is dropped; other keys are not blocked.
    pub async fn lookup(&self, key: K) -> AsyncKeyedCacheEntry<K, V> {
        let guard = self.lock_table.lock_tag(key.clone()).await;
        AsyncKeyedCacheEntry {
            _guard: guard,
            cache: self.cache.clone(),
            key,
        }
    }

    /// Clear the cache entries if the cache is not in use
    /// Returns true if the cache was cleared
    /// Returns false if the cache was not cleared because it is in use
    #[must_use]
    pub fn purge(&self) -> bool {
        let mut cache = self.cache.lock();
        if !self.lock_table.is_empty() {
            return false;
        }
        cache.clear();
        true
    }
}

/// A locked handle to one key's slot in an [`AsyncKeyedCache`].
///
/// Holds the key's lock for its lifetime (released on drop). All accessors operate on
/// the slot for [`key`](AsyncKeyedCacheEntry::key).
pub struct AsyncKeyedCacheEntry<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone,
{
    // Held for its lifetime to keep the key locked; released on drop.
    _guard: AsyncTagLockGuard<K>,
    cache: Arc<Mutex<LruCache<K, V>>>,
    key: K,
}

impl<K, V> AsyncKeyedCacheEntry<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone,
{
    /// The key this entry is locked on.
    #[must_use]
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Get a clone of the cached value for this key, if present.
    #[must_use]
    pub fn get(&self) -> Option<V> {
        self.cache.lock().get(&self.key).cloned()
    }

    /// Insert (or replace) the cached value for this key, returning the previous value if any.
    pub fn insert(&self, value: V) -> Option<V> {
        self.cache.lock().insert(self.key.clone(), value)
    }

    /// Remove and return the cached value for this key, if present.
    pub fn remove(&self) -> Option<V> {
        self.cache.lock().remove(&self.key)
    }
}
