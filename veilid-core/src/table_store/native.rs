use super::*;
pub use keyvaluedb_sqlite::*;
use std::path::PathBuf;

/// Above this size the open-time integrity check would hold up startup (it
/// reads every page; server record stores reach many GB), so it is skipped
/// and runtime repair-and-retry covers corruption instead
const REPAIR_CHECK_BYTE_LIMIT: u64 = 64 * 1024 * 1024;

/// Exclusive advisory lock on this namespace's table store, held for the
/// core's lifetime. The kernel releases the lock when the process dies,
/// however it dies, so a crash can never leave the namespace wedged; the
/// lock file itself is inert and is deliberately never deleted, because
/// unlinking a locked path would let a second instance lock a fresh inode
/// at the same path.
#[must_use]
pub(in crate::table_store) struct TableStoreLock {
    _file: std::fs::File,
}

/// Take the exclusive advisory lock on an open file.
///
/// std's `File::try_lock` grew an android implementation only in Rust 1.98, and the build pins
/// an older toolchain, so on android it answers Unsupported and every core silently starts
/// unguarded. Calling flock here makes the lock independent of which std compiled the build.
#[cfg(target_os = "android")]
fn try_lock_exclusive(file: &std::fs::File) -> Result<(), std::fs::TryLockError> {
    use std::os::fd::AsRawFd;
    // SAFETY: `file` owns the descriptor and keeps it open across the call
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    if e.kind() == std::io::ErrorKind::WouldBlock {
        Err(std::fs::TryLockError::WouldBlock)
    } else {
        Err(std::fs::TryLockError::Error(e))
    }
}

#[cfg(not(target_os = "android"))]
fn try_lock_exclusive(file: &std::fs::File) -> Result<(), std::fs::TryLockError> {
    file.try_lock()
}

/// Whether a lock error means the filesystem cannot lock at all, as opposed
/// to the lock being held; error-code set matches cargo's flock handling
fn lock_unsupported(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(unix)]
    if e.raw_os_error()
        .is_some_and(|c| c == libc::ENOTSUP || c == libc::EOPNOTSUPP || c == libc::ENOSYS)
    {
        return true;
    }
    #[cfg(windows)]
    if e.raw_os_error() == Some(1 /* ERROR_INVALID_FUNCTION */) {
        return true;
    }
    false
}

/// flock on Linux NFS is known broken: it can block forever even with the
/// non-blocking flag, so NFS mounts run unguarded, as cargo does
#[cfg(all(target_os = "linux", not(target_env = "musl")))]
fn is_nfs_mount(path: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(cpath) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: all-zero is a valid statfs: plain integers and byte arrays
    let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: cpath is NUL-terminated and buf is a writable statfs for the call
    let r = unsafe { libc::statfs(cpath.as_ptr(), &mut buf) };
    r == 0 && buf.f_type as u32 == libc::NFS_SUPER_MAGIC as u32
}
#[cfg(not(all(target_os = "linux", not(target_env = "musl"))))]
fn is_nfs_mount(_path: &std::path::Path) -> bool {
    false
}

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

    fn get_dbpath(&self, table: &str) -> VeilidAPIResult<PathBuf> {
        let config = self.config();
        let tablestoredir = config.table_store.directory.clone();
        std::fs::create_dir_all(&tablestoredir).map_err(VeilidAPIError::from)?;

        let namespace = config.namespace.clone();
        let dbpath: PathBuf = if namespace.is_empty() {
            [tablestoredir, String::from(table)].iter().collect()
        } else {
            [tablestoredir, format!("{}_{}", namespace, table)]
                .iter()
                .collect()
        };
        Ok(dbpath)
    }

    /// Take the exclusive lock for this namespace, failing if another live
    /// veilid core, in this process or any other, already holds it. Returns
    /// None, running unguarded, on filesystems that cannot lock.
    #[expect(clippy::unused_async)]
    pub async fn acquire_namespace_lock(&self) -> VeilidAPIResult<Option<TableStoreLock>> {
        let lockpath = self.get_dbpath("__veilid_lock")?;
        if is_nfs_mount(lockpath.parent().unwrap_or(&lockpath)) {
            veilid_log!(self warn
                "table store is on an NFS mount; running without the instance lock");
            return Ok(None);
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lockpath)
            .map_err(VeilidAPIError::from)?;
        ensure_file_private_owner(&lockpath).map_err(VeilidAPIError::internal)?;
        match try_lock_exclusive(&file) {
            Ok(()) => Ok(Some(TableStoreLock { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Err(VeilidAPIError::generic(format!(
                "another veilid instance is using table store namespace '{}' ({:?})",
                self.config().namespace,
                lockpath
            ))),
            Err(std::fs::TryLockError::Error(e)) if lock_unsupported(&e) => {
                veilid_log!(self warn
                    "table store filesystem does not support locking; running without the instance lock: {}", e);
                Ok(None)
            }
            Err(std::fs::TryLockError::Error(e)) => Err(VeilidAPIError::from(e)),
        }
    }

    #[expect(clippy::unused_async)]
    pub async fn open(
        &self,
        table_name: &str,
        column_count: u32,
        concurrency: usize,
    ) -> VeilidAPIResult<Database> {
        let dbpath = self.get_dbpath(table_name)?;

        // Ensure permissions are correct
        ensure_file_private_owner(&dbpath).map_err(VeilidAPIError::internal)?;

        let on_repair: RepairCallback = {
            let registry = self.registry();
            let table_name = table_name.to_string();
            Arc::new(move |report: &RepairReport| {
                veilid_log!(registry warn
                    "table store '{}' was corrupt and was rebuilt: {} rows recovered ({} partial tables), damaged files kept at {:?}: {}",
                    table_name, report.rows_recovered, report.partial_tables, report.corrupt_path, report.detected);
            })
        };
        let cfg = DatabaseConfig::new()
            .with_columns(column_count)
            .with_num_conns(concurrency)
            .with_vacuum_mode(VacuumMode::None)
            .with_repair_on_corrupt(true)
            .with_repair_check(RepairCheck::Full)
            .with_repair_check_byte_limit(REPAIR_CHECK_BYTE_LIMIT)
            .with_on_repair(on_repair);
        let db = Database::open(&dbpath, cfg).map_err(VeilidAPIError::from)?;

        // Ensure permissions are correct
        ensure_file_private_owner(&dbpath).map_err(VeilidAPIError::internal)?;

        veilid_log!(self trace
            "opened table store '{}' at path '{:?}' with {} columns",
            table_name,
            dbpath,
            column_count
        );
        Ok(db)
    }

    #[expect(clippy::unused_async)]
    pub async fn delete(&self, table_name: &str) -> VeilidAPIResult<bool> {
        let dbpath = self.get_dbpath(table_name)?;
        if !dbpath.exists() {
            veilid_log!(self debug "TableStore::delete '{}' at path '{:?}' not deleted", table_name, dbpath);
            return Ok(false);
        }

        veilid_log!(self trace "TableStore::delete '{}' at path '{:?}' deleted", table_name, dbpath);
        std::fs::remove_file(&dbpath).map_err(VeilidAPIError::from)?;

        // sqlite replays a stale -wal at a reused path into the next database created there
        for suffix in ["-wal", "-shm"] {
            let mut sibling = dbpath.clone().into_os_string();
            sibling.push(suffix);
            let _ = std::fs::remove_file(sibling);
        }

        Ok(true)
    }

    /// Delete every table file for the currently configured namespace.
    #[expect(clippy::unused_async)]
    pub async fn delete_all_in_namespace(&self) -> VeilidAPIResult<usize> {
        let config = self.config();
        let tablestoredir = config.table_store.directory.clone();
        let namespace = config.namespace.clone();
        let dir = std::path::PathBuf::from(&tablestoredir);
        if !dir.exists() {
            return Ok(0);
        }

        let prefix = if namespace.is_empty() {
            String::new()
        } else {
            format!("{}_", namespace)
        };

        // A bare prefix match is not enough: the default namespace's empty prefix matches
        // everything, and namespace "a" prefix-matches namespace "a_b"'s files
        fn owned_by_namespace(fname: &str, prefix: &str) -> bool {
            let Some(rest) = fname.strip_prefix(prefix) else {
                return false;
            };
            let rest = rest
                .strip_suffix("-wal")
                .or_else(|| rest.strip_suffix("-shm"))
                .unwrap_or(rest);
            let rest = rest
                .strip_suffix(".corrupt")
                .or_else(|| rest.strip_suffix(".repairing"))
                .unwrap_or(rest);
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
        for entry in std::fs::read_dir(&dir).map_err(VeilidAPIError::from)? {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !owned_by_namespace(fname, &prefix) {
                continue;
            }
            if let Err(e) = std::fs::remove_file(&path) {
                veilid_log!(self warn "delete_all_in_namespace: failed to remove '{}': {}", fname, e);
                continue;
            }
            deleted += 1;
            veilid_log!(self debug "delete_all_in_namespace: removed '{}'", fname);
        }
        Ok(deleted)
    }
}
