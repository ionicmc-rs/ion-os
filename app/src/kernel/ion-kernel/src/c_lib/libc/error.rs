//! Module for Errors.
use core::{cell::Cell, ops::Deref};

/// Represents the last KERNEL Error.
#[thread_local]
static ERRNO: Cell<i32> = Cell::new(0);

/// Returns a pointer to the Err Number
#[unsafe(no_mangle)]
pub extern "C" fn __errno_location() -> *mut i32 {
    // C convention: return a pointer to errno
    ERRNO.as_ptr()
}

/// Set the Error Number
// Helper for Rust side
pub fn set_errno(e: i32) {
    ERRNO.set(e);
}

/// Get Errno
pub fn get_errno() -> ErrCode {
    ErrCode(ERRNO.get())
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