//! Initialization for Ion OS
//!
//! This module contains the [`init`] functions, which initializes the following
//! - The I.D.T. (Interrupt Descriptor Table)
//! - GDT and TSS tables
//! - PIC8259 interrupts.
//! - Keyboard handler.
//! - Threads.
//!
//! # Errors
//! Currently, initialization is infallible, but this is subject to change, and can cause breaking changes
//! if we do not plan accordingly!
//!
//! Luckily, we planned for this, and created [`InitErr`].
//!
//! Details will be put here.

use core::fmt::Display;

use crossbeam_queue::ArrayQueue;

use crate::{arch::{self, interrupts::{self, keyboard::SC_QUEUE}, syscall::init_syscalls}, c_lib::BootInfo, lib_alloc::init_heap, privilege::init_users, serial_println};

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
    HeapInitializationErr,
}

impl Display for InitErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::HeapInitializationErr => write!(f, "Heap Initialization Error"),
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
            Self::HeapInitializationErr => true,
        }
    }
}


/// Initializes the kernel.
///
/// The Full list:
/// - The I.D.T. (Interrupt Descriptor Table)
/// - GDT and TSS tables
/// - PIC8259 interrupts.
/// - Keyboard handler.
/// - Threads.
///
/// and the rest is TODO.
/// # Errors
/// returns the first error, as an [`InitErr`]
#[allow(static_mut_refs)]
pub fn init(boot_info: BootInfo) -> Result<(), InitErr> {
    // serial_println!("Now Initializing GDT and TSS.");
    // interrupts::init_gdt_tss();
    serial_println!("Now Initializing IDT.");
    interrupts::init_interrupt_operations();

    // allocation

    // let mut mapper = unsafe { arch::mmu::init() };
    unsafe { arch::frame_allocator::init_global_frame_allocator(boot_info.mem_map_addr) };

    unsafe { arch::mmu::init_kernel_heap() };

    unsafe { init_heap() };

    unsafe { SC_QUEUE.write(ArrayQueue::new(100)) };

    init_users();
    unsafe { init_syscalls() };

    x86_64::instructions::interrupts::enable();

    serial_println!("Initializing Done.");
    Ok(())
}
