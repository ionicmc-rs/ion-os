use x86_64::structures::idt::{InterruptStackFrame, PageFaultErrorCode};
use x86_64::registers::control::Cr2;

use crate::serial_println;

pub(super) extern "x86-interrupt" fn page_fault(
    frame: InterruptStackFrame,
    error: PageFaultErrorCode,
) {
    // disable interrupts as soon as we can to prevent thread switching.
    x86_64::instructions::interrupts::disable();
    let addr = Cr2::read();
    serial_println!("page faulting at {addr:?} ({error:?})\n{frame:?}");
    panic!("Page Fault @ {:?} ec={:?}\n{:#?}", addr, error, frame);
}

pub const PAGE_FAULT_IST_INDEX: u16 = 1;