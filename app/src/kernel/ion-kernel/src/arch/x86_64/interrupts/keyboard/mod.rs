//! Handles Keyboard inputs
//!
//! You do not use this module at all directly.
//!
//! This is handled once in the keyboard handler thread.

use core::mem::MaybeUninit;

use crate::arch::interrupts::apic::handlers::notify;
use crossbeam_queue::ArrayQueue;
use spin::Mutex;
use x86_64::{instructions::port::Port, structures::idt::InterruptStackFrame};

use pc_keyboard::{
    HandleControl, Keyboard, ScancodeSet1,
    layouts::{self, Us104Key},
};

lazy_static::lazy_static! {
    /// Global keyboard
    pub static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = {
        // let mut data = Port::new(0x60);
        // let mut write = Port::new(0x64);

        // QEMU's legacy PS/2 controller supplies translated Set 1 scancodes.
        // Keep the decoder instance in Keyboard so it can preserve prefix
        // state across bytes (for example the E0 prefix on arrow keys).
        Mutex::new(Keyboard::new(ScancodeSet1::new(), Us104Key, HandleControl::Ignore))
    };
}

/// Scancode Queue
pub static mut SC_QUEUE: MaybeUninit<ArrayQueue<u8>> = MaybeUninit::zeroed();
static mut KBD_PORT: Port<u8> = Port::new(0x60);

/// Handler Keyboard Input
#[allow(static_mut_refs)]
pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let sc = unsafe { KBD_PORT.read() };

    // log::debug!("keyboard interrupt: {_stack_frame:#?}");

    // Never block or panic in interrupt context. If the queue is full, drop
    // this byte and keep the keyboard interrupt path alive.
    if unsafe { SC_QUEUE.assume_init_ref() }.push(sc).is_ok() {
        notify!(unsafe Keyboard);
    }
    // log::debug!("restoring rflags: {:?}", _stack_frame.cpu_flags);
}

mod ps2;
