use x86_64::structures::idt::InterruptStackFrame;

pub extern "x86-interrupt" fn timer_isr_entry(_: InterruptStackFrame) {
    // NOTE: for now, this simply requests rescheduling, but this may change in the future to actually reschedule here.
}