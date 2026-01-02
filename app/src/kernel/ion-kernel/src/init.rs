//! Initialization for Ion OS
//! 
//! This module contains the [`init`] functions, which initializes the following
//! - The I.D.T. (Interrupt Descriptor Table)
//! - GDT and TSS tables
//! - PIC8259 interrupts.
//! - Keyboard handler.
//! 
//! # Errors
//! Currently, initialization is infallible, but this is subject to change, and can cause breaking changes
//! if we do not plan accordingly!
//! 
//! Luckily, we planned for this, and created [`InitErr`].
//! 
//! Details will be put here. 

use core::fmt::Display;

use x86_64::structures::paging::{Size4KiB, mapper::MapToError};

use crate::{c_lib::BootInfo, interrupts, lib_alloc::init_heap, mem, serial_println};

/// An error while Initializing the Kernel
/// 
/// # Possible Values
/// - HeapInitializationErr(MapToError<Size4KIB>)
///   - we failed to init the heap.
/// 
/// and the rest is TODO.
#[derive(Debug)]
pub enum InitErr {
    /// An Error while initializing the Heap.
    HeapInitializationErr(MapToError<Size4KiB>)
}

impl Display for InitErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::HeapInitializationErr(e) => write!(f, "Heap Initialization Error: {e:?}")
        }
    }
}

impl InitErr {
    /// Returns wether this err is fatal
    /// 
    /// Returns true for:
    /// 
    /// - Heap Initialization Error.
    /// 
    /// The rest returns false
    pub fn is_fatal(&self) -> bool {
        match self {
            Self::HeapInitializationErr(_) => true
        }
    }
}

impl From<MapToError<Size4KiB>> for InitErr {
    fn from(value: MapToError<Size4KiB>) -> Self {
        Self::HeapInitializationErr(value)
    }
}

/// Initializes the kernel.
/// 
/// The Full list:
/// - The I.D.T. (Interrupt Descriptor Table)
/// - GDT and TSS tables
/// - PIC8259 interrupts.
/// - Keyboard handler.
/// 
/// and the rest is TODO.
/// # Errors
/// returns the first error, as an [`InitErr`]
pub fn init(boot_info: BootInfo) -> Result<(), InitErr> {
    // serial_println!("Now Initializing GDT and TSS.");
    // interrupts::init_gdt_tss();
    serial_println!("Now Initializing IDT.");
    interrupts::init_interrupt_operations();

    // allocation

    let mut mapper = unsafe { mem::init() };
    let mut f_alloc = unsafe { mem::BootInfoFrameAllocator::init(boot_info.mem_map_addr) };

    init_heap(&mut mapper, &mut f_alloc)?;

    // interrupts::enable();

    serial_println!("Initializing Done.");
    Ok(())
}