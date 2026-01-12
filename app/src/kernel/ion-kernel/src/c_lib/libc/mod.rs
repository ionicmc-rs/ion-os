//! libc by Rust but for Ion OS
//! 
//! In reality, `libc` is a "`Raw FFI bindings`" crate, while in `Ion OS`, we create all the functionality, so we are simply mapping to
//! things that already exist in our kernel. So simply put, this module is more of a way for c developers to be able to contribute to our
//! kernel.
//! 
//! However, An actual benefit the rises from this module, is that we are setting language features such as [`malloc`], [`abs`], [`perror`].
//! So if we ever decide to link to `C` code in the future, we can use all those functions just fine.
//! # Examples
//! allocation with malloc
//! ```
//! // `unsafe` omitted for simplicity
//! 
//! let allocation = malloc(4).cast::<u32>();
//! allocation.write(42);
//! free(allocation);
//! 
//! let new = realloc(allocation, 8).cast::<u64>();
//! new.write(u64::MAX);
//! free(new);
//! 
//! let for_bytes = calloc(4, 1);
//! let slice = slice::from_raw_parts(for_bytes, 4);
//! assert_eq!(slice, &[0, 0, 0, 0]);
//! free(for_bytes)
//! ```
//! Get the last os error
//! ```
//! let error = get_errno();
//! if *error != 0 {
//!     let str = c"Error is not 0";
//!     perror(str.as_ptr());
//!     set_errno(0);
//! }
//! ```
//! and finally, math
//! ```
//! let abs = abs(-1); // 1
//! let div = ldiv(10, 5); // 2r0
//! ```
#![allow(
    renamed_and_removed_lints, // Keep this order.
    unknown_lints, // Keep this order.
    nonstandard_style,
    overflowing_literals,
    unused_macros,
    unused_macro_rules,
)]
#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    safe_packed_borrows
)]
// Prepare for a future upgrade
// Things missing for 2024 that are blocked on MSRV or breakage
#![allow(
    missing_unsafe_on_extern,
    edition_2024_expr_fragment_specifier,
    
)]

pub mod primitive;
pub mod mem;
pub mod malloc_wraps;
pub mod error;
pub mod math;

// prelude

pub use primitive::*;
pub use malloc_wraps::*;
pub use mem::*;
pub use error::*;
pub use math::*;