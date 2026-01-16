use x86_64::structures::idt::InterruptStackFrame;

use crate::{hlt_loop, serial_println};

/// Index of a Double Fault in the IST.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

pub(super) extern "x86-interrupt" fn double_fault(_frame: InterruptStackFrame, _err: u64) -> ! {
    x86_64::instructions::interrupts::disable();
    // to prevent other threads continuing after a double fault, we halt the CPU forever. this (will) allow other CPUs to execute, however.
    serial_println!("double faulted - halting.");
    hlt_loop()
}
