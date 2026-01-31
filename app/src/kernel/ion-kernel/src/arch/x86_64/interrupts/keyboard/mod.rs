//! Handles Keyboard inputs
//!
//! You do not use this module at all directly.
//! 
//! This is handled once in the keyboard handler thread.

use core::{mem::MaybeUninit};

use crossbeam_queue::ArrayQueue;
use x86_64::{ instructions::port::Port, structures::idt::InterruptStackFrame};
use spin::Mutex;
use crate::{
    arch::interrupts::pic8259::handlers::notify, log::warn,
};

use pc_keyboard::{
    HandleControl, Keyboard, layouts::{self, Us104Key}
};

lazy_static::lazy_static! {
    /// Global keyboard
    pub static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ps2::ScancodeSet>> = {
        // let mut data = Port::new(0x60);
        // let mut write = Port::new(0x64);

        Mutex::new(Keyboard::new(ps2::ScancodeSet::None, Us104Key, HandleControl::Ignore))
    };
}

/// Scancode Queue
// it is way safer to have this zeroed so that it isn't accident read incorrectly
pub static mut SC_QUEUE: MaybeUninit<ArrayQueue<u8>> = MaybeUninit::zeroed();
static mut KBD_PORT: Port<u8> = Port::new(0x60);

/// Handler Keyboard Input
#[allow(static_mut_refs)]
pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let sc = unsafe { KBD_PORT.read() };

    // before interrupts are ever enabled - SC_QUEUE should be enabled.
    if unsafe { SC_QUEUE.assume_init_mut().push(sc) }.is_err() {
        warn!("keyboard: input dropped due to lack of space.");
    }
    
    notify!(unsafe Keyboard);
}

mod ps2;
