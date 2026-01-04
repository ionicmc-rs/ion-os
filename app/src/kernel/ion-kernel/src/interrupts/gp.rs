use x86_64::{registers::control::Cr2, structures::idt::InterruptStackFrame};

pub extern "x86-interrupt" fn protection_fault(frame: InterruptStackFrame, code: u64) {
    let cr2 = Cr2::read();
    panic!("General Protection Fault (Cr2: {cr2:?}): {code}\n{frame:#?}");
}