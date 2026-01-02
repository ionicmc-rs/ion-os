//! Module for Kernel Errors.
use crate::serial_println;

/// Represents the last KERNEL Error.
static mut ERRNO: i32 = 0;

/// Returns a pointer to the Err Number
#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn __errno_location() -> *mut i32 {
    unsafe { &mut ERRNO }
}

use core::{ffi::{CStr, c_char}, ops::Deref};

fn read_prefix<'a>(prefix: *const c_char) -> &'a str {
    if prefix.is_null() {
        return "";
    }
    // Wrap the raw pointer in a CStr
    let c_str = unsafe { CStr::from_ptr(prefix) };
    // Convert to Rust &str (lossy if invalid UTF‑8)
    c_str.to_str().unwrap_or("<invalid utf8>")
}

/// Prints the error with the given message.
/// 
/// prints: {msg}: {error meaning if found} (os error {code})
#[unsafe(no_mangle)]
pub extern "C" fn perror(text: *mut c_char) {
    let string = read_prefix(text);
    let errno = get_errno();
    if let Some(v) = errno.meaning() {
        serial_println!("{}: {} (os error {})", string, v, *errno)
    } else {
        serial_println!("{}: (os error {})", string, *errno)
    }
}

/// Set the Error Number
// Helper for Rust side
#[allow(static_mut_refs)]
pub fn set_errno(e: i32) {
    serial_println!("Setting Error Code: {} to {:?}", e, unsafe { &ERRNO as *const i32 });
    unsafe { ERRNO = e };
}

/// Get Errno
#[allow(static_mut_refs)]
pub fn get_errno() -> ErrCode {
    ErrCode(unsafe { ERRNO })
}

// typing

/// An Error Code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ErrCode(pub i32);

impl ErrCode {
    /// Returns meaning of error codes.
    pub const fn meaning(&self) -> Option<&'static str> {
        // This is the internal unstable API for Error Codes.
        match self.0 {
            0 => Some("Ok"),
            1 => Some("Failed"),
            2 => Some("Memory Corruption"),
            3 => Some("CPU Exception"),
            4 => Some("Process Failure"),
            5 => Some("Allocation Failure"),
            6 => Some("Invalid Input"),
            7 => Some("Missing Feature"),
            _ => None
        }
    }
}

impl Deref for ErrCode {
    type Target = i32;
    fn deref(&self) -> &Self::Target {
        &self.0 
    }
}