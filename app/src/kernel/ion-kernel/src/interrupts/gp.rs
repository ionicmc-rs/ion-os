use x86_64::structures::idt::InterruptStackFrame;

use crate::{serial_println, test::exit};

pub extern "x86-interrupt" fn protection_fault(_frame: InterruptStackFrame, _code: u64) {
    x86_64::instructions::interrupts::disable();
    serial_println!("General Protection Fault");
    exit(crate::test::QemuExitCode::Failed)
}
