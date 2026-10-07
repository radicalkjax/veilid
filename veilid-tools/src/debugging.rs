#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]

use super::*;

/// Debugger attachment state of the current process.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DebuggerState {
    /// No debugger is attached
    Detached,
    /// A debugger is attached
    Attached,
    /// Attachment cannot be determined on this platform
    Unknown,
}

/// Determine whether a debugger is attached to the current process.
#[must_use]
#[allow(unreachable_code)]
pub fn debugger_state() -> DebuggerState {
    #[cfg(windows)]
    unsafe {
        return if winapi::um::debugapi::IsDebuggerPresent() == 0 {
            DebuggerState::Detached
        } else {
            DebuggerState::Attached
        };
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        use libc::{c_int, c_void, getpid, size_t, sysctl, CTL_KERN, KERN_PROC, KERN_PROC_PID};
        // kinfo_proc.kp_proc.p_flag & P_TRACED
        const P_TRACED: u32 = 0x00800;
        unsafe {
            let mut info = [0u8; 648];
            let mut size: size_t = info.len();
            let mut mib: [c_int; 4] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()];
            let ret = sysctl(
                mib.as_mut_ptr(),
                4,
                info.as_mut_ptr() as *mut c_void,
                &mut size as *mut size_t,
                core::ptr::null_mut(),
                0,
            );
            if ret == 0 {
                return if (u32::from_le_bytes(info[32..36].try_into().unwrap()) & P_TRACED) != 0 {
                    DebuggerState::Attached
                } else {
                    DebuggerState::Detached
                };
            }
        }
    }

    // TracerPid is nonzero when a debugger is attached; works on linux, android, some BSDs
    #[cfg(unix)]
    {
        if let Ok(file) = std::fs::File::open("/proc/self/status") {
            let file = std::io::BufReader::new(file);
            use std::io::BufRead;
            for line in file.lines().map_while(Result::ok) {
                let line = line.trim();
                if line.starts_with("TracerPid:") {
                    return match [" 0", "\t0", ":0"].iter().any(|zero| line.ends_with(zero)) {
                        true => DebuggerState::Detached,
                        false => DebuggerState::Attached,
                    };
                }
            }
        }
    }

    DebuggerState::Unknown
}

/// Block until a debugger attaches, or until the optional timeout elapses.
pub fn wait_until_debugger_attached<T: Into<Option<Duration>>>(
    timeout: T,
) -> Result<(), &'static str> {
    let timeout = timeout.into().map(|dur| std::time::Instant::now() + dur);
    loop {
        match debugger_state() {
            DebuggerState::Attached => return Ok(()),
            DebuggerState::Unknown => {
                return Err("debugger state is unknown, cannot wait for attach")
            }
            DebuggerState::Detached => {
                if let Some(timeout) = timeout {
                    if std::time::Instant::now() >= timeout {
                        return Err("wait for debugger attach timed out");
                    }
                }
                std::thread::sleep(Duration::from_millis(16));
            }
        }
    }
}
