use core::{arch::naked_asm, sync::atomic::{AtomicU64, Ordering}};

use x86_64::structures::idt::InterruptStackFrame;

use crate::{
    arch::interrupts::pic8259::handlers::notify, log, scheduler::{self, SCHEDULER}, serial_println
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

#[repr(C)]
pub struct InterruptRegs {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9:  u64,
    pub r8:  u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub stack_frame: InterruptStackFrame,
}

#[unsafe(naked)]
pub extern "x86-interrupt" fn timer_trampoline(_: InterruptStackFrame /* unused param here for Rust to not complain */) {
        naked_asm!(
            "push rax",
            "push rbx",
            "push rcx",
            "push rdx",
            "push rbp",
            "push rsi",
            "push rdi",
            "push r8",
            "push r9",
            "push r10",
            "push r11",
            "push r12",
            "push r13",
            "push r14",
            "push r15",

            "mov rdi, rsp",   // &InterruptRegs
            "call {handler}",

            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rdi",
            "pop rsi",
            "pop rbp",
            "pop rdx",
            "pop rcx",
            "pop rbx",
            "pop rax",

            "iretq",
            handler = sym timer_handler,
        );
}

#[allow(unused)]
#[unsafe(no_mangle)]
pub extern "C" fn timer_handler(regs: &mut InterruptRegs) {
    let next = SCHEDULER.pick_preempt();

    if let Some(v) = next {
        if let Some(old) = v.0 {
            old.save_from_interrupt(regs); // does not need the actual memory
        }
        v.1.load_with_interrupt_frame(unsafe { regs.stack_frame.as_mut() });
    }

    let t = TICKS.fetch_add(1, Ordering::Relaxed);
    if t.is_multiple_of(50) {
        serial_println!("reached notable tick: {t}");
    }

    scheduler::request_reschedule();
    scheduler::wake_done();

    notify!(unsafe Timer)
}
