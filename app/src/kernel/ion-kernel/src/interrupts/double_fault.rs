use x86_64::structures::idt::InterruptStackFrame;


/// Index of a Double Fault in the IST.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

pub(super) extern "x86-interrupt" fn double_fault(
    frame: InterruptStackFrame,
    err: u64
) -> ! {
    panic!("Reached a Double Fault: {err}\n{frame:#?}");
}