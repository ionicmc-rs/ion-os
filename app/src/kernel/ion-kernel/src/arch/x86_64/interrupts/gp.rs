use x86_64::structures::idt::InterruptStackFrame;

use crate::c_lib::libc::{self, OsError};

pub extern "x86-interrupt" fn protection_fault(frame: InterruptStackFrame, _code: u64) {
    x86_64::instructions::interrupts::disable();
    libc::set_os_error(OsError::CpuException); // may be useful to let the underlying error appear instead
    panic!("General Protection Fault:\n{frame:#?}")
}
