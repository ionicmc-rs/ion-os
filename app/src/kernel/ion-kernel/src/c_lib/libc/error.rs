//! Kernel error / errno support.
//!
//! This module provides a libc-compatible `errno` implementation,
//! supporting both global (kernel / early boot) and thread-local
//! (user / normal kernel threads) storage.

use core::mem::variant_count;

use crate::{
    thread::{self, tls::tcb},
};

/// Global errno (used before threading / TLS is initialized)
static mut GLOBAL_ERRNO: i32 = 0;

/// Returns a pointer to the current errno location.
///
/// libc ABI compatibility:
/// ```c
/// int *__errno_location(void);
/// ```
#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn __errno_location() -> *mut i32 {
    if thread::current().is_some() {
        &raw mut tcb().errno
    } else {
        &raw mut GLOBAL_ERRNO
    }
}

/// Set errno to a raw value.
#[inline(always)]
pub fn set_errno(e: i32) {
    unsafe { *__errno_location() = e };
}

/// Set errno from an OS error.
#[inline(always)]
pub fn set_os_error(e: OsError) {
    set_errno(e as i32);
}

/// Get the current errno.
#[inline(always)]
pub fn get_errno() -> ErrCode {
    ErrCode(unsafe { *__errno_location() })
}

/// Wrapper type for an error number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct ErrCode(i32);

/// Kernel / OS error codes.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsError {
    Ok = 0,
    HardwareFail = 1,
    MemoryError = 2,
    CpuException = 3,
    ProcessFail = 4,
    AllocFail = 5,
    InvalidInput = 6,
    Unsupported = 7,
    NotFound = 8,
    PermissionDenied = 9,
    AlreadyExists = 10,
    Busy = 11,
    Timeout = 12,
    Interrupted = 13,
    IsADirectory = 14,
}

impl From<OsError> for ErrCode {
    #[inline(always)]
    fn from(e: OsError) -> Self {
        ErrCode(e as i32)
    }
}

impl ErrCode {
    /// Return a human-readable meaning for this error code.
    pub const fn meaning(&self) -> Option<&'static str> {
        match self.0 {
            x if x == OsError::Ok as i32 => Some("Ok"),
            x if x == OsError::HardwareFail as i32 => Some("Hardware Fail"),
            x if x == OsError::MemoryError as i32 => Some("Out of Memory"),
            x if x == OsError::CpuException as i32 => Some("CPU Exception"),
            x if x == OsError::ProcessFail as i32 => Some("Process Failure"),
            x if x == OsError::AllocFail as i32 => Some("Allocation Failure"),
            x if x == OsError::InvalidInput as i32 => Some("Invalid Input"),
            x if x == OsError::Unsupported as i32 => Some("Unsupported"),
            x if x == OsError::NotFound as i32 => Some("Not Found"),
            x if x == OsError::PermissionDenied as i32 => Some("Permission Denied"),
            x if x == OsError::AlreadyExists as i32 => Some("Already Exists"),
            x if x == OsError::Busy as i32 => Some("Busy"),
            x if x == OsError::Timeout as i32 => Some("Timeout"),
            x if x == OsError::Interrupted as i32 => Some("Interrupted"),
            x if x == OsError::IsADirectory as i32 => Some("Is a Directory"),
            _ => None,
        }
    }

    /// Converts this error code
    pub const fn to_os_error(&self) -> Option<OsError> {
        if self.code() > variant_count::<OsError>() as i32 {
            panic!("invalid value for error code created")
        }

        if self.code() < 0 {
            // codes less than 0 are reserved for user use, and have no specific meaning

            return None;
        }

        // Safety: we just ensured we are in the valid range
        unsafe { core::mem::transmute(*self) }
    }

    #[inline(always)]
    pub const fn is_ok(self) -> bool {
        self.0 == 0
    }

    #[inline(always)]
    pub const fn code(self) -> i32 {
        self.0
    }
}

impl core::ops::Deref for ErrCode {
    type Target = i32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
