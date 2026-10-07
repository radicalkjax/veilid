use super::*;

use std::io;
use std::path::Path;

//////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Assert that an expression evaluates to `Err(..)`, panicking with the `Ok` value otherwise.
#[macro_export]
macro_rules! assert_err {
    ($ex:expr) => {
        if let Ok(v) = $ex {
            panic!("assertion failed, expected Err(..), got {:?}", v);
        }
    };
}

/// Build an `io::Error` of kind `Other` from anything `ToString`.
#[macro_export]
macro_rules! io_error_other {
    ($msg:expr) => {
        io::Error::new(io::ErrorKind::Other, $msg.to_string())
    };
}

/// Wrap any error in an `io::Error` of kind `Other`.
pub fn to_io_error_other<E: std::error::Error + Send + Sync + 'static>(x: E) -> io::Error {
    io::Error::other(x)
}

/// Return early with an `io::Error` of kind `Other` built from anything `ToString`.
#[macro_export]
macro_rules! bail_io_error_other {
    ($msg:expr) => {
        return io::Result::Err(io::Error::new(io::ErrorKind::Other, $msg.to_string()))
    };
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////

cfg_if! {
    if #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))] {
        /// Number of available hardware threads, falling back to 1 if it cannot be determined.
        #[must_use]
        pub fn get_concurrency() -> u32 {
            std::thread::available_parallelism()
                .map(|x| x.get())
                .unwrap_or_else(|e| {
                    warn!("unable to get concurrency defaulting to single core: {}", e);
                    1
                }) as u32
        }
    }
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Convert a microsecond duration to seconds as `f64`, shedding least-significant bits if the value is too large to represent exactly.
#[must_use]
pub fn timestamp_duration_to_secs(dur: u64) -> f64 {
    // Downshift precision until it fits, lose least significant bits
    let mut mul: f64 = 1.0f64 / 1_000_000.0f64;
    let mut usec = dur;
    while usec > (u32::MAX as u64) {
        usec >>= 1;
        mul *= 2.0f64;
    }
    f64::from(usec as u32) * mul
}

/// Convert seconds as `f64` to a microsecond duration.
#[must_use]
pub fn secs_to_timestamp_duration(secs: f64) -> u64 {
    (secs * 1000000.0f64) as u64
}

/// Convert milliseconds to microseconds.
#[must_use]
pub fn ms_to_us(ms: u32) -> u64 {
    (ms as u64) * 1000u64
}

/// Convert microseconds to milliseconds, erroring if the result overflows `u32`.
///
/// Errors with a `String` describing the conversion failure if the millisecond count exceeds `u32::MAX`.
pub fn us_to_ms(us: u64) -> Result<u32, String> {
    u32::try_from(us / 1000u64).map_err(|e| format!("could not convert microseconds: {}", e))
}

/// Decide whether a retry is due, with logarithmic falloff between `interval_start_us` and `interval_max_us`.
///
/// Returns `false` before `interval_start_us` of the reliable period has elapsed, `true` once `interval_max_us`
/// has passed since `last_us`, and otherwise spaces retries out exponentially by `interval_multiplier_us`.
#[must_use]
pub fn retry_falloff_log(
    last_us: u64,
    cur_us: u64,
    interval_start_us: u64,
    interval_max_us: u64,
    interval_multiplier_us: f64,
) -> bool {
    //
    if cur_us < interval_start_us {
        // Don't require a retry within the first 'interval_start_us' microseconds of the reliable time period
        false
    } else if cur_us >= last_us + interval_max_us {
        // Retry at least every 'interval_max_us' microseconds
        true
    } else {
        // Exponential falloff between 'interval_start_us' and 'interval_max_us' microseconds
        last_us
            <= secs_to_timestamp_duration(
                timestamp_duration_to_secs(cur_us) / interval_multiplier_us,
            )
    }
}

/// Apply `closure` to items in order until one returns `Some`, giving up after `max` failures.
///
/// Synchronous; blocks only as far as `closure` does.
pub fn try_at_most_n_things<T, I, C, R>(max: usize, things: I, closure: C) -> Option<R>
where
    I: IntoIterator<Item = T>,
    C: Fn(T) -> Option<R>,
{
    let mut fails = 0usize;
    for thing in things.into_iter() {
        if let Some(r) = closure(thing) {
            return Some(r);
        }
        fails += 1;
        if fails >= max {
            break;
        }
    }
    None
}

/// Await `closure` on items in order until one returns `Some`, giving up after `max` failures.
///
/// Awaits each `closure` future sequentially, not concurrently; total wait is the sum.
pub async fn async_try_at_most_n_things<T, I, C, R, F>(
    max: usize,
    things: I,
    closure: C,
) -> Option<R>
where
    I: IntoIterator<Item = T>,
    C: Fn(T) -> F,
    F: Future<Output = Option<R>>,
{
    let mut fails = 0usize;
    for thing in things.into_iter() {
        if let Some(r) = closure(thing).await {
            return Some(r);
        }
        fails += 1;
        if fails >= max {
            break;
        }
    }
    None
}

/// In-place `min`/`max` assignment for ordered types.
pub trait CmpAssign {
    /// Replace `self` with `other` if `other` is smaller.
    fn min_assign(&mut self, other: Self);
    /// Replace `self` with `other` if `other` is larger.
    fn max_assign(&mut self, other: Self);
}

impl<T> CmpAssign for T
where
    T: core::cmp::Ord,
{
    fn min_assign(&mut self, other: Self) {
        if &other < self {
            *self = other;
        }
    }
    fn max_assign(&mut self, other: Self) {
        if &other > self {
            *self = other;
        }
    }
}

/// Unspecified address and port zero matching the address family of `socket_addr`.
#[must_use]
pub fn compatible_unspecified_socket_addr(socket_addr: &SocketAddr) -> SocketAddr {
    match socket_addr {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)), 0),
    }
}

cfg_if! {
    if #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))] {
        use std::net::UdpSocket;

        static IPV6_IS_SUPPORTED: Mutex<Option<bool>> = Mutex::new(None);

        /// Whether IPv6 is usable locally, tested once by binding a loopback UDP socket and cached.
        pub fn is_ipv6_supported() -> bool {
            let mut opt_supp = IPV6_IS_SUPPORTED.lock();
            if let Some(supp) = *opt_supp {
                return supp;
            }
            // Not exhaustive but for our use case it should be sufficient. If no local ports are available for binding, Veilid isn't going to work anyway :P
            let supp = UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)).is_ok();
            *opt_supp = Some(supp);
            supp
        }

        static IPV4_IS_SUPPORTED: Mutex<Option<bool>> = Mutex::new(None);

        /// Whether IPv4 is usable locally, tested once by binding a loopback UDP socket and cached.
        pub fn is_ipv4_supported() -> bool {
            let mut opt_supp = IPV4_IS_SUPPORTED.lock();
            if let Some(supp) = *opt_supp {
                return supp;
            }
            // Not exhaustive but for our use case it should be sufficient. If no local ports are available for binding, Veilid isn't going to work anyway :P
            let supp = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST,  0)).is_ok();
            *opt_supp = Some(supp);
            supp
        }

    }
}

/// Unspecified addresses for every locally supported IP family (IPv4 always, IPv6 if available).
#[must_use]
pub fn available_unspecified_addresses() -> Vec<IpAddr> {
    if is_ipv6_supported() {
        vec![
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        ]
    } else {
        vec![IpAddr::V4(Ipv4Addr::UNSPECIFIED)]
    }
}

/// Resolve a listen-address string to socket addresses.
///
/// A bare port (`:8080` or `8080`) expands to every supported unspecified address; a host or host:port is parsed
/// or resolved directly and only the given port is used.
///
/// Errors with a `String` if a bare port does not parse as a `u16`, or if the host:port form fails
/// to parse (wasm32) or to resolve via DNS (native).
pub fn listen_address_to_socket_addrs(listen_address: &str) -> Result<Vec<SocketAddr>, String> {
    // If no address is specified, but the port is, use ipv4 and ipv6 unspecified
    // If the address is specified, only use the specified port and fail otherwise

    let ip_addrs = available_unspecified_addresses();

    Ok(if let Some(portstr) = listen_address.strip_prefix(':') {
        let port = portstr
            .parse::<u16>()
            .map_err(|e| format!("Invalid port format in udp listen address: {}", e))?;
        ip_addrs.iter().map(|a| SocketAddr::new(*a, port)).collect()
    } else if let Ok(port) = listen_address.parse::<u16>() {
        ip_addrs.iter().map(|a| SocketAddr::new(*a, port)).collect()
    } else {
        let listen_address_with_port = if listen_address.contains(':') {
            listen_address.to_string()
        } else {
            format!("{}:0", listen_address)
        };
        cfg_if! {
            if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                use core::str::FromStr;
                vec![SocketAddr::from_str(&listen_address_with_port).map_err(|e| format!("Unable to parse address: {}",e))?]
            } else {
                listen_address_with_port
                    .to_socket_addrs()
                    .map_err(|e| format!("Unable to resolve address: {}", e))?
                    .collect()
            }
        }
    })
}

/// Dedup, but doesn't require a sorted vec, and keeps the element order
pub trait RemoveDuplicates<T: PartialEq> {
    /// Remove duplicate elements in place, keeping the first occurrence of each.
    fn remove_duplicates(&mut self);
}

impl<T: PartialEq + Ord> RemoveDuplicates<T> for Vec<T> {
    fn remove_duplicates(&mut self) {
        let mut firsts = Vec::<bool>::with_capacity(self.len());
        {
            let mut seen = BTreeSet::<&T>::new();
            for item in self.iter() {
                firsts.push(seen.insert(item));
            }
        }
        let mut index = 0;
        self.retain(|_| {
            let first = firsts[index];
            index += 1;
            first
        });
    }
}

/// Check for duplicates but doesn't require a sorted vec
pub trait HasDuplicates<T: PartialEq> {
    /// Whether any element appears more than once.
    fn has_duplicates(&self) -> bool;
}

impl<T: PartialEq + Ord> HasDuplicates<T> for Vec<T> {
    fn has_duplicates(&self) -> bool {
        let mut seen = BTreeSet::<&T>::new();
        for item in self.iter() {
            if !seen.insert(item) {
                return true;
            }
        }
        false
    }
}

cfg_if::cfg_if! {
    if #[cfg(unix)] {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::prelude::PermissionsExt;

        /// Ensure `path`, if it exists as a file, is mode 0o600 and owned by the current user and group.
        ///
        /// Errors with a `String` if the metadata read or the `chmod` to 0o600 fails (io error wrapped in the
        /// message), or if the file's owner/group does not match the current user. A path that is not a file is Ok.
        pub fn ensure_file_private_owner<P:AsRef<Path>>(path: P) -> Result<(), String>
        {
            let path = path.as_ref();
            if !path.is_file() {
                return Ok(());
            }

            let uid = unsafe { libc::geteuid() };
            let gid = unsafe { libc::getegid() };
            let meta = std::fs::metadata(path).map_err(|e| format!("unable to get metadata for path: {}", e))?;

            if meta.mode() != 0o600 {
                std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o600)).map_err(|e| format!("unable to set correct permissions on path: {}", e))?;
            }
            if meta.uid() != uid || meta.gid() != gid {
                return Err("path has incorrect owner/group".to_owned());
            }
            Ok(())
        }

        /// Ensure `path`, if it exists as a directory, is mode 0o700 (or 0o750 when `group_read`) and owned by the current user and group.
        ///
        /// Errors with a `String` if the metadata read or the `chmod` fails (io error wrapped in the message),
        /// or if the directory's owner/group does not match the current user. A path that is not a directory is Ok.
        pub fn ensure_directory_private_owner<P:AsRef<Path>>(path: P, group_read: bool) -> Result<(), String>
        {
            let path = path.as_ref();
            if !path.is_dir() {
                return Ok(());
            }

            let uid = unsafe { libc::geteuid() };
            let gid = unsafe { libc::getegid() };
            let meta = std::fs::metadata(path).map_err(|e| format!("unable to get metadata for path: {}", e))?;

            let perm = if group_read {
                0o750
            } else {
                0o700
            };

            if meta.mode() != perm {
                std::fs::set_permissions(path,std::fs::Permissions::from_mode(perm)).map_err(|e| format!("unable to set correct permissions on path: {}", e))?;
            }
            if meta.uid() != uid || meta.gid() != gid {
                return Err("path has incorrect owner/group".to_owned());
            }
            Ok(())
        }
    } else if #[cfg(windows)] {
        //use std::os::windows::fs::MetadataExt;
        //use windows_permissions::*;

        /// Ensure `path`, if it exists as a file, has private ownership. No-op on this platform.
        pub fn ensure_file_private_owner<P:AsRef<Path>>(path: P) -> Result<(), String>
        {
            let path = path.as_ref();
            if !path.is_file() {
                return Ok(());
            }

            Ok(())
        }

        /// Ensure `path`, if it exists as a directory, has private ownership. No-op on this platform.
        pub fn ensure_directory_private_owner<P:AsRef<Path>>(path: P, _group_read: bool) -> Result<(), String>
        {
            let path = path.as_ref();
            if !path.is_dir() {
                return Ok(());
            }

            Ok(())
        }

    } else {
        /// Ensure `path`, if it exists as a file, has private ownership. No-op on this platform.
        pub fn ensure_file_private_owner<P:AsRef<Path>>(path: P) -> Result<(), String>
        {
            let path = path.as_ref();
            if !path.is_file() {
                return Ok(());
            }

            Ok(())
        }

        /// Ensure `path`, if it exists as a directory, has private ownership. No-op on this platform.
        pub fn ensure_directory_private_owner<P:AsRef<Path>>(path: P, _group_read: bool) -> Result<(), String>
        {
            let path = path.as_ref();
            if !path.is_dir() {
                return Ok(());
            }

            Ok(())
        }
    }
}

#[repr(C, align(8))]
struct AlignToEight([u8; 8]);

/// # Safety
/// Ensure you immediately initialize this vector as it could contain sensitive data
#[must_use]
pub unsafe fn aligned_8_u8_vec_uninit(n_bytes: usize) -> Vec<u8> {
    let n_units = n_bytes.div_ceil(mem::size_of::<AlignToEight>());
    let mut aligned: Vec<AlignToEight> = Vec::with_capacity(n_units);
    let ptr = aligned.as_mut_ptr();
    let cap_units = aligned.capacity();
    mem::forget(aligned);

    Vec::from_raw_parts(
        ptr as *mut u8,
        n_bytes,
        cap_units * mem::size_of::<AlignToEight>(),
    )
}

/// # Safety
/// Ensure you immediately initialize this vector as it could contain sensitive data
#[must_use]
pub unsafe fn unaligned_u8_vec_uninit(n_bytes: usize) -> Vec<u8> {
    let mut unaligned: Vec<u8> = Vec::with_capacity(n_bytes);
    let ptr = unaligned.as_mut_ptr();
    mem::forget(unaligned);

    Vec::from_raw_parts(ptr, n_bytes, n_bytes)
}

/// Type name of a value as a string, without needing to name the type.
pub fn type_name_of_val<T: ?Sized>(_val: &T) -> &'static str {
    std::any::type_name::<T>()
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Scope guard that prints to stderr and bumps a shared counter on entry, then decrements and prints on drop.
pub struct DebugGuard {
    name: &'static str,
    counter: &'static AtomicUsize,
}

impl DebugGuard {
    /// Increment `counter`, print the entry count, and return a guard that prints the exit count when dropped.
    pub fn new(name: &'static str, counter: &'static AtomicUsize) -> Self {
        let c = counter.fetch_add(1, Ordering::SeqCst);
        eprintln!("{} entered: {}", name, c + 1);
        Self { name, counter }
    }
}

impl Drop for DebugGuard {
    fn drop(&mut self) {
        let c = self.counter.fetch_sub(1, Ordering::SeqCst);
        eprintln!("{} exited: {}", self.name, c - 1);
    }
}
