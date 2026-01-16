use x86_64::structures::idt::InterruptStackFrame;

pub extern "x86-interrupt" fn protection_fault(frame: InterruptStackFrame, _code: u64) {
    x86_64::instructions::interrupts::disable();
    panic!("General Protection Fault:\n{frame:#?}")
}
