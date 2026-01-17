// pointers

use crate::arch::PointerWidth;

/// The width of pointers.
pub const POINTER_WIDTH_RAW: u8 = 64; // pointer width cannot exceed 8 bits, as it is only 16, 32 or 64 (well, for now)

/// The width of pointers, represented as [`PointerWidth`]
pub const POINTER_WIDTH: PointerWidth = PointerWidth::SixtyFour;

/// Convenience type for converting from usize to it's underlying type
pub type PointerTy = u64;

/// Page size in bytes
pub const PAGE_SIZE: usize = 2 * 1024 * 1024; // 2 MiB

/// Canonical address bits
pub const CANONICAL_BITS: usize = 48;

/// Architecture's name
pub const ARCH_NAME: &str = "x86_64";