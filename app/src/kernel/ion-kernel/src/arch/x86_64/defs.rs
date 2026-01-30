// pointers

use crate::arch::{PointerWidth, port::{Port, PortData, Read, ReadWrite, Write}};

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

// Ports

/// Debug port used in qemu
pub const DEBUG_PORT: WritePort<u8> = Port::new(0xE9);

/// Port used for reading key input.
pub const KEYBOARD_READ: ReadPort<u8> = Port::new(0x60);

/// Port for sending keyboard commands
/// 
/// this is handled by the keyboard handler thread - so you don't need to use this
pub const KEYBOARD_WRITE: WritePort<u8> = Port::new(0x64);

pub type WritePort<T: PortData> = Port<T, Write>;
pub type ReadPort<T: PortData> = Port<T, Read>;
pub type RWPort<T: PortData> = Port<T, ReadWrite>;