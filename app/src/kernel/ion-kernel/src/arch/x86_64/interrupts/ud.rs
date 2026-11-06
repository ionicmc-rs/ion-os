use x86_64::structures::idt::InterruptStackFrame;

use crate::c_lib::libc;

pub extern "x86-interrupt" fn invalid_opcode(frame: InterruptStackFrame) {
    libc::set_os_error(libc::OsError::Unsupported);
    panic!("invalid opcode: \n{frame:#?}")
}
