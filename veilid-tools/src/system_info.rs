#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]

use super::*;

/// Total physical memory in bytes, or None if not determinable on this platform.
#[must_use]
pub fn total_memory_bytes() -> Option<u64> {
    cfg_if! {
        if #[cfg(any(target_os = "linux", target_os = "android"))] {
            total_memory_bytes_sysinfo()
        } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {
            total_memory_bytes_sysctl()
        } else if #[cfg(target_os = "windows")] {
            total_memory_bytes_global_memory_status()
        } else {
            None
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn total_memory_bytes_sysinfo() -> Option<u64> {
    // SAFETY: sysinfo is plain data, so all zeroes is a valid value the call overwrites
    let mut si: libc::sysinfo = unsafe { core::mem::zeroed() };
    // SAFETY: si is a complete sysinfo the call fills in place
    if unsafe { libc::sysinfo(&mut si) } != 0 {
        return None;
    }
    // c_ulong is u64 on 64-bit targets, u32 on 32-bit; the cast is identity or a widen
    #[allow(clippy::unnecessary_cast)]
    let total = si.totalram as u64;
    #[allow(clippy::unnecessary_cast)]
    let unit = si.mem_unit as u64;
    total.checked_mul(unit)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn total_memory_bytes_sysctl() -> Option<u64> {
    let mut mem: u64 = 0;
    let mut len = core::mem::size_of::<u64>();
    let mut mib = [libc::CTL_HW, libc::HW_MEMSIZE];
    // SAFETY: mib has mib.len() entries and len holds the size of mem, which the call writes
    let res = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            (&mut mem) as *mut u64 as *mut libc::c_void,
            &mut len,
            core::ptr::null_mut(),
            0,
        )
    };
    if res != 0 {
        return None;
    }
    Some(mem)
}

#[cfg(target_os = "windows")]
fn total_memory_bytes_global_memory_status() -> Option<u64> {
    use winapi::um::sysinfoapi::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: MEMORYSTATUSEX is plain data, so all zeroes is a valid value the call overwrites
    let mut status: MEMORYSTATUSEX = unsafe { core::mem::zeroed() };
    status.dwLength = core::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: status is a complete MEMORYSTATUSEX with dwLength set, as the call requires
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return None;
    }
    Some(status.ullTotalPhys)
}
