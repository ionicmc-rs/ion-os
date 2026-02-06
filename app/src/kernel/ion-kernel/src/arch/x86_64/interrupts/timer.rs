use core::sync::atomic::{AtomicU64, Ordering};

use x86_64::structures::idt::InterruptStackFrame;

use crate::{
    arch::interrupts::pic8259::handlers::notify, log, scheduler, serial_println
};

// pub extern "x86-interrupt" fn timer_isr_entry(_: InterruptStackFrame) {
//     // NOTE: for now, this simply requests rescheduling, but this may change in the future to actually reschedule here.
//     TICKS.fetch_add(1, Ordering::Relaxed);

//     scheduler::wake_done();

//     notify!(unsafe Timer)
// }

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Returns the number of ticks since startup
///
/// **note**: this wraps when overflowing.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

#[allow(unused)]
#[unsafe(no_mangle)]
pub extern "x86-interrupt" fn timer_isr(_stack_frame: InterruptStackFrame) {
    let t = TICKS.fetch_add(1, Ordering::Relaxed);
    if t.is_multiple_of(50) {
        log::debug!("{_stack_frame:#?}");
        serial_println!("reached notable tick: {t}");
    }

    scheduler::request_reschedule();
    scheduler::wake_done();

    notify!(unsafe Timer)
}
