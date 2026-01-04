use x86_64::structures::idt::InterruptStackFrame;


/// Index of a Double Fault in the IST.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

pub(super) extern "x86-interrupt" fn double_fault(
    _frame: InterruptStackFrame,
    _err: u64
) -> ! {
    x86_64::instructions::interrupts::disable();
    // We handle by panicking, which means our panic impl must never fault.
    panic!("Reached a Double Fault.");
}