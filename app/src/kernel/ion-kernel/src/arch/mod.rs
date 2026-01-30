//! Arch-agnostic tools for Ion OS
//! 
//! This module allows us to prevent frequent [`cfg_if`]s matching the target arch.
//! 
//! The currently supported archs are:
//! - x86_64

mod shared;
pub use shared::*;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        mod x86_64;
        pub use x86_64::*;
    } else {
        compile_error!("Unsupported arch used. Please use one of the pre-made targets in `targets/`")
    }
}

// static assertions

const _: () = {
    assert!(defs::PAGE_SIZE == 2 * 1024 * 1024, "Page size is not 2 MiB - but that is the only supported page size (for now).");
};