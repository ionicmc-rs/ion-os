//! Handles Keyboard inputs
//!
//! Currently: The following issues exist:
//! - FIXME(sync): keyboard inputs are handle synchronously
//! - FIXME(size): interrupt handler is too big and prone to errors.
//! - FIXME(scset): Scancode Set is not being queried and set.
//!
//! You do not use this module

use x86_64::structures::idt::InterruptStackFrame;

use crate::{interrupts::pic8259::handlers::notify, scheduler};

use pc_keyboard::{
    layouts::{self, Us104Key},
    HandleControl, Keyboard,
};
use spin::Mutex;

lazy_static::lazy_static! {
    static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ps2::ScancodeSet>> = {
        // let mut data = Port::new(0x60);
        // let mut write = Port::new(0x64);

        Mutex::new(Keyboard::new(ps2::ScancodeSet::None, Us104Key, HandleControl::Ignore))
    };
}

#[allow(unused)]
struct Once {
    init: *const ps2::ScancodeSet,
}

#[allow(unused)]
impl Once {
    pub const fn new(init: &ps2::ScancodeSet) -> Self {
        Self {
            init: init as *const ps2::ScancodeSet,
        }
    }

    pub const fn new_ptr(init: *const ps2::ScancodeSet) -> Self {
        Self { init }
    }

    pub fn set(&self, set: ps2::ScancodeSet) {
        let ptr = self.init as *mut ps2::ScancodeSet;
        unsafe { *ptr = set }
    }

    pub fn query(&self) -> ps2::ScancodeSet {
        unsafe { *self.init }
    }
}

unsafe impl Sync for Once {}

// for now, allow unused.
#[allow(unused)]
static mut SCAN_CODE_SET_IS_SET: ps2::ScancodeSet = ps2::ScancodeSet::None;

#[allow(unused)]
static SCAN_CODE_SET_QUERIED: Once = Once::new_ptr(&raw const SCAN_CODE_SET_IS_SET);

/// Handler Keyboard Input
pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe { scheduler::wake_kbd() };

    notify!(unsafe Keyboard);
}

mod ps2;
