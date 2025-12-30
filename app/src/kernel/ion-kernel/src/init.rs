use core::fmt::Display;

use crate::{interrupts, serial_println};

/// An error while Initializing the Kernel
/// 
/// Full List:
/// - IDT init err.
/// 
/// and the res is TODO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitErr {
    // todo
}

impl Display for InitErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "InitErr") // for now
    }
}

impl InitErr {
    /// Returns wether this err is fatal
    pub fn is_fatal(&self) -> bool {
        false
    }
}

/// Initializes the kernel.
/// 
/// The Full list:
/// - IDT Table
/// 
/// and the rest is TODO.
/// # Error
/// returns the first error, as an [`InitErr`]
pub fn init() -> Result<(), InitErr> {
    // serial_println!("Now Initializing GDT and TSS.");
    // interrupts::init_gdt_tss();
    serial_println!("Now Initializing IDT.");
    interrupts::init_idt();

    // interrupts::enable();

    serial_println!("Initializing Done.");
    Ok(())
}