//! libc by Rust but for Ion OS
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
    // Allowed globally, the warning is enabled in individual modules as we work through them
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