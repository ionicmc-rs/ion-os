//! Handles Keyboard inputs
//!
//! Currently: The following issues exist:
//! - FIXME(sync): keyboard inputs are handle synchronously
//! - FIXME(size): interrupt handler is too big and prone to errors.
//! - FIXME(scset): Scancode Set is not being queried and set.
//!
//! You do not use this module

use core::cell::OnceCell;

use crossbeam_queue::ArrayQueue;
use x86_64::{instructions::port::Port, structures::idt::InterruptStackFrame};

use crate::{
    interrupts::pic8259::handlers::notify,
    log::warn,
    thread::sync::{Mutex, TryLockError},
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
pub static SC_QUEUE: Mutex<OnceCell<ArrayQueue<u8>>> = Mutex::new(OnceCell::new());

/// Handler Keyboard Input
pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let binding = match SC_QUEUE.try_lock() {
        Ok(v) => v,
        Err(e) => match e {
            TryLockError::Locked => {
                warn!("keyboard input dropped: input is currently being handled.");
                notify!(unsafe Keyboard);
                return;
            }
            TryLockError::Poisoned => {
                panic!("scancode queue was poisoned - this indicates it was improperly used.")
            }
        },
    };
    let queue = binding.get_or_init(|| ArrayQueue::new(100));

    let mut data = Port::new(0x60);

    let scancode = unsafe { data.read() };

    match queue.push(scancode) {
        Ok(()) => (),
        Err(e) => warn!("keyboard input `{e}` dropped: not enough space."),
    }

    // this is left here to avoid confusion
    //
    // do NOT call this - this is only called AFTER hooks are ran
    // by another thread.
    // unsafe { scheduler::wake_kbd() };

    drop(binding);
    notify!(unsafe Keyboard);
}

mod ps2;
