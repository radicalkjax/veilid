#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]

use super::*;

/// Total physical memory in bytes, or None if not determinable on this platform.
#[must_use]
pub fn total_memory_bytes() -> Option<u64> {
    cfg_if! {
        if #[cfg(any(target_os = "linux", target_os = "android"))] {
            let mut si: libc::sysinfo = unsafe { core::mem::zeroed() };
            if unsafe { libc::sysinfo(&mut si) } != 0 {
                return None;
            }
            // c_ulong is u64 on 64-bit targets, u32 on 32-bit; the cast is identity or a widen
            #[allow(clippy::unnecessary_cast)]
            let total = si.totalram as u64;
            #[allow(clippy::unnecessary_cast)]
            let unit = si.mem_unit as u64;
            total.checked_mul(unit)
        } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {
            let mut mem: u64 = 0;
            let mut len = core::mem::size_of::<u64>();
            let mut mib = [libc::CTL_HW, libc::HW_MEMSIZE];
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
        } else if #[cfg(target_os = "windows")] {
            use winapi::um::sysinfoapi::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
            let mut status: MEMORYSTATUSEX = unsafe { core::mem::zeroed() };
            status.dwLength = core::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
                return None;
            }
            Some(status.ullTotalPhys)
        } else {
            None
        }
    }
}
