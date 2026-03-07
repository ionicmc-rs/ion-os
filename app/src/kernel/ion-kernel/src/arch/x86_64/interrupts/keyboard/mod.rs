//! Handles Keyboard inputs
//!
//! You do not use this module at all directly.
//! 
//! This is handled once in the keyboard handler thread.

use core::{any::Any, mem::MaybeUninit, sync::atomic::AtomicUsize};

use x86_64::{ instructions::port::Port, structures::idt::InterruptStackFrame};
use spin::Mutex;
use crate::{
    arch::interrupts::pic8259::handlers::notify, c_lib::libc::{self, OsError}, log::{self, warn}
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

pub struct ScancodeQueue {
    buffer: [Option<u8>; 50]
}

impl ScancodeQueue {
    pub fn new() -> Self {
        Self {
            buffer: [None; 50]
        }
    }

    pub fn push(&mut self, sc: u8) -> Result<(), u8> {
        for x in 0..50 {
            if self.buffer[x].is_none() {
                self.buffer[x] = Some(sc);
                return Ok(());
            }
        }
        Err(sc)
    }

    pub fn pop(&mut self) -> Option<u8> {
        if self.buffer[0].is_some() {
            let res = self.buffer[0].take();
            self.shift_back();
            return res;
        } else {
            return None;
        }
    }

    fn shift_back(&mut self) {
        for x in 0..49 {
            self.buffer[x] = self.buffer[x + 1];
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buffer[0].is_none()
    }
}


pub static SCANCODES_PUSHED: AtomicUsize = AtomicUsize::new(0);

/// Scancode Queue
// it is way safer to have this zeroed so that it isn't accident read incorrectly
pub static mut SC_QUEUE: MaybeUninit<Mutex<ScancodeQueue>> = MaybeUninit::zeroed();
static mut KBD_PORT: Port<u8> = Port::new(0x60);

/// Handler Keyboard Input
#[allow(static_mut_refs)]
pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let sc = unsafe { KBD_PORT.read() };

    // log::debug!("keyboard interrupt: {_stack_frame:#?}");

    // before interrupts are ever enabled - SC_QUEUE should be enabled.
    if let Some(mut lock) = unsafe {
        SC_QUEUE.assume_init_ref().try_lock()
    } {
        lock.push(sc).expect("should not have more than 100 scan codes without consumption");
    } else {
        warn!("cannot scan code push at this time.")
    }
    
    SCANCODES_PUSHED.fetch_add(1, core::sync::atomic::Ordering::SeqCst);

    notify!(unsafe Keyboard);
    // log::debug!("restoring rflags: {:?}", _stack_frame.cpu_flags);
}

mod ps2;
