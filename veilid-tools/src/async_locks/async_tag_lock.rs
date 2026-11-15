//! AsyncTagLock
//!
//! A keyed lock: each distinct tag has its own mutex, created on first use and
//! removed once no guards for it remain. Locking one tag never blocks another.
use super::*;

use core::fmt::Debug;
use core::hash::Hash;

/// A guard holding the lock for one tag; releases it on drop.
#[derive(Clone, Debug)]
pub struct AsyncTagLockGuard<T>
where
    T: Hash + Eq + Clone + Debug,
{
    inner: Arc<AsyncTagLockGuardInner<T>>,
}

impl<T> AsyncTagLockGuard<T>
where
    T: Hash + Eq + Clone + Debug,
{
    /// The tag this guard holds.
    #[must_use]
    pub fn tag(&self) -> T {
        self.inner.tag()
    }
}

#[derive(Debug)]
struct AsyncTagLockGuardInner<T>
where
    T: Hash + Eq + Clone + Debug,
{
    table: AsyncTagLockTable<T>,
    tag: T,
    guard: Option<AsyncMutexGuardArc<()>>,
}

impl<T> AsyncTagLockGuardInner<T>
where
    T: Hash + Eq + Clone + Debug,
{
    fn new(table: AsyncTagLockTable<T>, tag: T, guard: AsyncMutexGuardArc<()>) -> Self {
        Self {
            table,
            tag,
            guard: Some(guard),
        }
    }

    fn tag(&self) -> T {
        self.tag.clone()
    }
}

impl<T> Drop for AsyncTagLockGuardInner<T>
where
    T: Hash + Eq + Clone + Debug,
{
    fn drop(&mut self) {
        let mut inner = self.table.inner.lock();
        // Inform the table we're dropping this guard
        let guards = {
            // Get the table entry, it must exist since we have a guard locked
            let entry = inner.table.get_mut(&self.tag).unwrap_or_log();
            // Decrement the number of guards
            entry.guards -= 1;
            // Return the number of guards left
            entry.guards
        };
        // If there are no guards left, we remove the tag from the table
        if guards == 0 {
            inner.table.remove(&self.tag).unwrap_or_log();
        }
        // Proceed with releasing guard, which may cause some concurrent tag lock to acquire
        drop(self.guard.take());
    }
}

#[derive(Clone, Debug)]
struct AsyncTagLockTableEntry {
    mutex: Arc<AsyncMutex<()>>,
    guards: usize,
}

#[derive(Debug)]
struct AsyncTagLockTableInner<T>
where
    T: Hash + Eq + Clone + Debug,
{
    table: HashMap<T, AsyncTagLockTableEntry>,
}

/// A table of per-tag async locks, sharable across tasks.
#[derive(Clone, Debug)]
pub struct AsyncTagLockTable<T>
where
    T: Hash + Eq + Clone + Debug,
{
    inner: Arc<Mutex<AsyncTagLockTableInner<T>>>,
}

impl<T> AsyncTagLockTable<T>
where
    T: Hash + Eq + Clone + Debug,
{
    /// Create an empty tag lock table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AsyncTagLockTableInner {
                table: HashMap::new(),
            })),
        }
    }

    /// Whether no tags are currently locked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.lock();
        inner.table.is_empty()
    }

    /// The number of tags currently locked.
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock();
        inner.table.len()
    }

    /// Acquire the lock for `tag`, waiting until it is free.
    ///
    /// Blocks until this tag's lock is free; other tags never block this one. The returned
    /// guard holds the tag until dropped, at which point the tag's entry is removed if no
    /// guards remain.
    pub async fn lock_tag(&self, tag: T) -> AsyncTagLockGuard<T> {
        // Get or create a tag lock entry
        let mutex = {
            let mut inner = self.inner.lock();

            // See if this tag is in the table
            // and if not, add a new mutex for this tag
            let entry = inner
                .table
                .entry(tag.clone())
                .or_insert_with(|| AsyncTagLockTableEntry {
                    mutex: Arc::new(AsyncMutex::new(())),
                    guards: 0,
                });

            // Increment the number of guards
            entry.guards += 1;

            // Return the mutex associated with the tag
            entry.mutex.clone()

            // Drop the table guard
        };

        // Lock the tag lock
        let guard = mutex.lock_arc().await;

        // Return the locked guard
        AsyncTagLockGuard {
            inner: Arc::new(AsyncTagLockGuardInner::new(self.clone(), tag, guard)),
        }
    }

    /// Try to acquire the lock for `tag` without waiting, or `None` if it is already held.
    ///
    /// Never blocks. On success the guard holds the tag until dropped.
    pub fn try_lock_tag(&self, tag: T) -> Option<AsyncTagLockGuard<T>> {
        // Get or create a tag lock entry
        let mut inner = self.inner.lock();

        // See if this tag is in the table
        // and if not, add a new mutex for this tag
        let entry = inner.table.entry(tag.clone());

        // Lock the tag lock
        let guard = match entry {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let e = o.get_mut();
                let guard = e.mutex.try_lock_arc()?;
                e.guards += 1;
                guard
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                let mutex = Arc::new(AsyncMutex::new(()));
                let guard = mutex.try_lock_arc().unwrap_or_log();
                v.insert(AsyncTagLockTableEntry { mutex, guards: 1 });
                guard
            }
        };
        // Return guard
        Some(AsyncTagLockGuard {
            inner: Arc::new(AsyncTagLockGuardInner::new(self.clone(), tag, guard)),
        })
    }
}

impl<T> Default for AsyncTagLockTable<T>
where
    T: Hash + Eq + Clone + Debug,
{
    fn default() -> Self {
        Self::new()
    }
}
