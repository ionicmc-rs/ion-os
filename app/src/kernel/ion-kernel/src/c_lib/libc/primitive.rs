//! Primitive types for C.

pub use core::ffi::{
    c_char, c_double, c_float, c_int, c_long, c_longlong, c_ptrdiff_t, c_schar, c_short, c_size_t, c_ssize_t, 
};
use core::ptr::null;

/// NULL pointer.
pub const NULL: *const () = null();