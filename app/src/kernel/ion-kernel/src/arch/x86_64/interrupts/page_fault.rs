use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptStackFrame, PageFaultErrorCode};

use crate::c_lib::libc::{self, OsError};

pub(super) extern "x86-interrupt" fn page_fault(
    frame: InterruptStackFrame,
    error: PageFaultErrorCode,
) {
    // disable interrupts as soon as we can to prevent thread switching.
    let addr = Cr2::read();
    let tls = crate::arch::tls::read_ptr();
    libc::set_os_error(OsError::PermissionDenied); // may be useful to let underlying error appear instead
    panic!("Page Fault @ {:?} (tls: {tls:?}) ec={:?}\n{:#?}", addr, error, frame);
}

pub const PAGE_FAULT_IST_INDEX: u16 = 1;
