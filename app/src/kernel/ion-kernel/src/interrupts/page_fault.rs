// use x86_64::structures::idt::{InterruptStackFrame, PageFaultErrorCode};
// use x86_64::registers::control::Cr2;

// use crate::serial_println;

// pub(super) extern "x86-interrupt" fn page_fault(
//     frame: InterruptStackFrame,
//     error: PageFaultErrorCode,
// ) {
//     let addr = Cr2::read();
//     serial_println!("Page Fault @ {:?} ec={:?}\n{:#?}", addr, error, frame);
//     loop { x86_64::instructions::hlt(); }
// }