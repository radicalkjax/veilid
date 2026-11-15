use super::*;

use core::fmt::Debug;
use core::hash::Hash;

/// Holds the lock for one tag in a `TagLockTable`. The tag is released and its
/// table entry reclaimed when the last clone of the guard drops.
#[derive(Clone, Debug)]
pub struct TagLockGuard<T>
where
    T: Hash + Eq + Clone + Debug,
{
    inner: Arc<TagLockGuardInner<T>>,
}

impl<T> TagLockGuard<T>
where
    T: Hash + Eq + Clone + Debug,
{
    /// The tag this guard holds.
    #[must_use]
    pub fn tag(&self) -> T {
        self.inner.tag()
    }
}

struct TagLockGuardInner<T>
where
    T: Hash + Eq + Clone + Debug,
{
    table: TagLockTable<T>,
    tag: T,
    guard: Option<ArcMutexGuard<RawMutex, ()>>,
}

impl<T> fmt::Debug for TagLockGuardInner<T>
where
    T: Hash + Eq + Clone + Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TagLockGuardInner")
            .field("tag", &self.tag)
            .finish()
    }
}

impl<T> TagLockGuardInner<T>
where
    T: Hash + Eq + Clone + Debug,
{
    fn new(table: TagLockTable<T>, tag: T, guard: ArcMutexGuard<RawMutex, ()>) -> Self {
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

impl<T> Drop for TagLockGuardInner<T>
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
struct TagLockTableEntry {
    mutex: Arc<Mutex<()>>,
    guards: usize,
}

struct TagLockTableInner<T>
where
    T: Hash + Eq + Clone + Debug,
{
    table: HashMap<T, TagLockTableEntry>,
}

/// A table of mutexes keyed by tag. Locking a tag blocks only other lockers of
/// the same tag; distinct tags never contend. Entries are created on first lock
/// and removed when their last guard drops.
#[derive(Clone)]
pub struct TagLockTable<T>
where
    T: Hash + Eq + Clone + Debug,
{
    inner: Arc<Mutex<TagLockTableInner<T>>>,
}

impl<T> fmt::Debug for TagLockTable<T>
where
    T: Hash + Eq + Clone + Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TagLockTable").finish()
    }
}

impl<T> TagLockTable<T>
where
    T: Hash + Eq + Clone + Debug,
{
    /// Create an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TagLockTableInner {
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

    /// Number of tags currently locked.
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock();
        inner.table.len()
    }

    /// Lock `tag`, blocking until it is available, and return a guard holding it.
    /// Blocks on the per-tag mutex; the tag stays locked until the returned guard
    /// (and all its clones) drop.
    pub fn lock_tag(&self, tag: T) -> TagLockGuard<T> {
        // Get or create a tag lock entry
        let mutex = {
            let mut inner = self.inner.lock();

            // See if this tag is in the table
            // and if not, add a new mutex for this tag
            let entry = inner
                .table
                .entry(tag.clone())
                .or_insert_with(|| TagLockTableEntry {
                    mutex: Arc::new(Mutex::new(())),
                    guards: 0,
                });

            // Increment the number of guards
            entry.guards += 1;

            // Return the mutex associated with the tag
            entry.mutex.clone()

            // Drop the table guard
        };

        // Lock the tag lock
        let guard = mutex.lock_arc();

        // Return the locked guard
        TagLockGuard {
            inner: Arc::new(TagLockGuardInner::new(self.clone(), tag, guard)),
        }
    }

    /// Lock `tag` without blocking, returning `None` if it is already held.
    /// On success the tag stays locked until the returned guard (and all its
    /// clones) drop.
    pub fn try_lock_tag(&self, tag: T) -> Option<TagLockGuard<T>> {
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
                let mutex = Arc::new(Mutex::new(()));
                let guard = mutex.try_lock_arc().unwrap_or_log();
                v.insert(TagLockTableEntry { mutex, guards: 1 });
                guard
            }
        };
        // Return guard
        Some(TagLockGuard {
            inner: Arc::new(TagLockGuardInner::new(self.clone(), tag, guard)),
        })
    }
}

impl<T> Default for TagLockTable<T>
where
    T: Hash + Eq + Clone + Debug,
{
    fn default() -> Self {
        Self::new()
    }
}
