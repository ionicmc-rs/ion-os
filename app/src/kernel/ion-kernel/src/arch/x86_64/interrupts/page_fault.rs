use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptStackFrame, PageFaultErrorCode};

pub(super) extern "x86-interrupt" fn page_fault(
    frame: InterruptStackFrame,
    error: PageFaultErrorCode,
) {
    // disable interrupts as soon as we can to prevent thread switching.
    x86_64::instructions::interrupts::disable();
    let addr = Cr2::read();
    let tls = crate::arch::tls::read_ptr();
    panic!("Page Fault @ {:?} (tls: {tls:?}) ec={:?}\n{:#?}", addr, error, frame);
}

pub const PAGE_FAULT_IST_INDEX: u16 = 1;
