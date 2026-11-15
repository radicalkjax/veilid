//! Refuse to run as a second veilid_flutter image in one process.
//!
//! A rebuild can swap the library file on disk under a running process; a later dlopen then
//! loads the new file as a SECOND image with its own uninitialized statics, and calls into it
//! hang forever. dlopen cannot be made to fail from inside the library, so
//! `initialize_veilid_flutter` asks the loader whether another resident image already exports
//! it and aborts loudly if so. Only one veilid-core image (of any version) may be loaded per
//! process; re-initialization of the SAME image stays allowed (initialize is idempotent).
//!
//! The check runs on the initializing thread, never in a load-time constructor: the loader
//! lock is not held, nothing is written to the environment or any other process-wide state,
//! and every loader handle taken here is released before returning.

use std::ffi::{c_void, CStr, CString};
use std::sync::OnceLock;

const ENTRY_SYMBOL: &CStr = c"initialize_veilid_flutter";
const VERSION_SYMBOL: &CStr = c"veilid_version_string";

static DUPLICATE_OF: OnceLock<Option<String>> = OnceLock::new();

/// Some(description) when another veilid_flutter image is already loaded in this process
pub fn duplicate_image() -> Option<&'static str> {
    DUPLICATE_OF.get_or_init(find_other_image).as_deref()
}

fn our_entry() -> *const c_void {
    crate::dart_ffi::initialize_veilid_flutter as *const c_void
}

fn find_other_image() -> Option<String> {
    find_other_image_than(our_entry())
}

/// Description of a resident image exporting the entry point at an address other than `ours`
fn find_other_image_than(ours: *const c_void) -> Option<String> {
    for name in loaded_image_names() {
        // SAFETY: RTLD_NOLOAD never loads anything; a handle only comes back for an image that
        // is already resident, and it is released below so its refcount is unchanged
        let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOLOAD | libc::RTLD_LAZY) };
        if handle.is_null() {
            continue;
        }
        // SAFETY: handle is live and the symbol name is NUL-terminated
        let entry = unsafe { libc::dlsym(handle, ENTRY_SYMBOL.as_ptr()) };
        let description = (!entry.is_null() && !std::ptr::eq(entry, ours))
            .then(|| describe_other_image(handle, entry));
        // SAFETY: balances the dlopen above
        unsafe { libc::dlclose(handle) };
        if let Some(first) = description {
            return Some(format!(
                "first image: {}; this image: {} (veilid-core {})",
                first,
                image_path_of(ours),
                veilid_core::veilid_version_string(),
            ));
        }
    }
    None
}

/// Path and version of the other image, read through its own exported version function
fn describe_other_image(handle: *mut c_void, entry: *const c_void) -> String {
    // SAFETY: handle is live; the symbol has the stable `extern "C" fn() -> *mut c_char`
    // prototype every veilid-flutter build exports, and its result is leaked because freeing
    // across images is a mismatched allocator
    let version = unsafe {
        let ver = libc::dlsym(handle, VERSION_SYMBOL.as_ptr());
        if ver.is_null() {
            "<unknown version>".to_owned()
        } else {
            let f: extern "C" fn() -> *mut libc::c_char = std::mem::transmute(ver);
            let s = f();
            if s.is_null() {
                "<unknown version>".to_owned()
            } else {
                CStr::from_ptr(s).to_string_lossy().into_owned()
            }
        }
    };
    format!("{} (veilid-core {})", image_path_of(entry), version)
}

fn image_path_of(addr: *const c_void) -> String {
    // SAFETY: dladdr fills info only on success, and dli_fname is then a loader-owned
    // NUL-terminated path that outlives this call
    unsafe {
        let mut info: libc::Dl_info = std::mem::zeroed();
        if libc::dladdr(addr, &mut info) != 0 && !info.dli_fname.is_null() {
            return CStr::from_ptr(info.dli_fname)
                .to_string_lossy()
                .into_owned();
        }
    }
    "<unknown image>".to_owned()
}

// mach-o/dyld.h
#[cfg(target_vendor = "apple")]
extern "C" {
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_name(image_index: u32) -> *const libc::c_char;
}

/// Every image dyld has loaded, by the path dyld knows it under
#[cfg(target_vendor = "apple")]
fn loaded_image_names() -> Vec<CString> {
    let mut names = Vec::new();
    // SAFETY: indexes below the reported count are valid, and dyld returns null for
    // any index that stopped being valid
    unsafe {
        for i in 0.._dyld_image_count() {
            let name = _dyld_get_image_name(i);
            if !name.is_null() {
                names.push(CStr::from_ptr(name).to_owned());
            }
        }
    }
    names
}

/// The leading fields of `struct dl_phdr_info`, identical on glibc, musl and bionic
#[cfg(any(target_os = "linux", target_os = "android"))]
#[repr(C)]
struct DlPhdrInfoPrefix {
    dlpi_addr: usize,
    dlpi_name: *const libc::c_char,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
type DlPhdrCallback =
    unsafe extern "C" fn(*mut DlPhdrInfoPrefix, usize, *mut c_void) -> libc::c_int;

#[cfg(any(target_os = "linux", target_os = "android"))]
extern "C" {
    fn dl_iterate_phdr(callback: Option<DlPhdrCallback>, data: *mut c_void) -> libc::c_int;
}

/// Every object the dynamic loader has mapped, by the name it was loaded under
#[cfg(any(target_os = "linux", target_os = "android"))]
fn loaded_image_names() -> Vec<CString> {
    unsafe extern "C" fn collect(
        info: *mut DlPhdrInfoPrefix,
        _size: usize,
        data: *mut c_void,
    ) -> libc::c_int {
        // SAFETY: the loader passes a valid info for each object and our Vec as data;
        // nothing here can panic, so no unwind crosses the C frame
        unsafe {
            let names = &mut *(data as *mut Vec<CString>);
            let name = (*info).dlpi_name;
            if !name.is_null() {
                let name = CStr::from_ptr(name);
                if !name.is_empty() {
                    names.push(name.to_owned());
                }
            }
        }
        0
    }
    let mut names: Vec<CString> = Vec::new();
    // SAFETY: collect only reads the info it is handed and appends to names
    unsafe {
        dl_iterate_phdr(Some(collect), (&mut names as *mut Vec<CString>).cast());
    }
    names
}

/// No loader enumeration on this platform, so a duplicate load goes undetected
#[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
fn loaded_image_names() -> Vec<CString> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_image_is_not_a_duplicate() {
        assert!(duplicate_image().is_none());
        assert!(duplicate_image().is_none(), "cached answer is stable");
    }

    #[test]
    fn any_other_exporter_of_the_entry_point_is_a_duplicate() {
        // SAFETY: a symbol lookup with a NUL-terminated name and no other side effects
        let visible = unsafe { libc::dlsym(libc::RTLD_DEFAULT, ENTRY_SYMBOL.as_ptr()) };
        if visible.is_null() {
            // The test binary does not export the entry point, so no image can "duplicate" it
            return;
        }
        let foreign = libc::malloc as *const c_void;
        let desc = find_other_image_than(foreign).expect("our own image counts as the other one");
        assert!(desc.contains("first image:"), "{desc}");
        assert!(desc.contains("veilid-core"), "{desc}");
    }

    #[test]
    fn loader_enumeration_sees_this_process() {
        let names = loaded_image_names();
        assert!(!names.is_empty());
        assert!(names.iter().all(|n| !n.as_bytes().is_empty()));
    }

    #[test]
    fn describes_the_image_holding_our_entry() {
        let path = image_path_of(our_entry());
        assert_ne!(path, "<unknown image>");
        assert!(!path.is_empty());
        // SAFETY: a null path opens the main program, which is always resident; closed below
        let handle = unsafe { libc::dlopen(std::ptr::null(), libc::RTLD_NOLOAD | libc::RTLD_LAZY) };
        assert!(!handle.is_null());
        let desc = describe_other_image(handle, our_entry());
        assert!(desc.contains("veilid-core"), "{desc}");
        // SAFETY: balances the dlopen above
        unsafe { libc::dlclose(handle) };
    }
}
