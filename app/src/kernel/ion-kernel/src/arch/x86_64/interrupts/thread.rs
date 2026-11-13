use core::{
    arch::naked_asm,
    sync::atomic::{AtomicU8, AtomicU64, Ordering, compiler_fence, fence},
};

use spin::Mutex;
use x86_64::structures::idt::InterruptStackFrame;

use crate::{
    arch::{
        cpu,
        interrupts::{self, InterruptGuard, apic::handlers::notify, without_interrupts},
    },
    event_logger::{self, Event, push_event},
    log,
    scheduler::{self, ScheduleReason, current_scheduler},
    serial_println, thread,
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
#[derive(Debug)]
pub struct InterruptRegs {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}

#[unsafe(naked)]
pub extern "x86-interrupt" fn timer_trampoline(
    _: InterruptStackFrame, /* unused param here for Rust to not complain */
) {
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


        "mov rdi, rsp",   // &mut InterruptRegs
        "lea rsi, [rsp + 120]", // above pushed registers
        "sub rsp, 8",     // align the stack for the call
        "call {handler}",
        "add rsp, 8",     // restore the stack pointer


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
pub extern "C" fn timer_handler(regs: &mut InterruptRegs, frame: &mut InterruptStackFrame) {
    let t = TICKS.fetch_add(1, Ordering::Relaxed);

    if thread::current().is_none() {
        // The timer is enabled before the first scheduler thread starts.
        // Acknowledge that early interrupt too, or the LAPIC leaves it in
        // service and will not deliver subsequent timer ticks.
        notify!(unsafe Timer);
        return;
    }

    let next = current_scheduler().pick_preempt();

    if let Some(v) = next {
        if let Some(old) = v.0 {
            old.save_from_interrupt(regs, unsafe { frame.as_mut().read_only() }); // does not need the actual memory
        }
        v.1.load_with_interrupt_frame(regs, unsafe { frame.as_mut() });
    }

    scheduler::request_reschedule();
    scheduler::wake_done();

    notify!(unsafe Timer);
}

pub(super) const TIMER_IST: u16 = 4;
pub(super) const YIELD_IST: u16 = 4;

#[unsafe(naked)]
pub extern "x86-interrupt" fn yield_trampoline(
    _: InterruptStackFrame, /* unused param here for Rust to not complain */
) {
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

        "mov rdi, rsp",   // &mut InterruptRegs
        "lea rsi, [rsp + 120]", // above pushed registers
        "sub rsp, 8",     // align the stack for the call
        "call {handler}",
        "add rsp, 8",     // restore the stack pointer

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
        handler = sym yield_handler,
    );
}

extern "C" fn yield_handler(regs: &mut InterruptRegs, frame: &mut InterruptStackFrame) {
    without_interrupts(|| {
        if let Some(mut lock) = YIELD_REASON.try_lock() {
            if let Some(reason) = lock.take() {
                if let Some(v) = current_scheduler().pick(reason.0) {
                    fence(Ordering::SeqCst);
                    if let Some(old) = v.0 {
                        old.save_from_interrupt(regs, unsafe { frame.as_mut().read_only() });
                    }

                    v.1.load_with_interrupt_frame(regs, unsafe { frame.as_mut() });
                }
            }
        }
    });
}

// yield arguments

pub struct YieldReason(pub ScheduleReason);

unsafe impl Sync for YieldReason {}
unsafe impl Send for YieldReason {}

static YIELD_REASON: Mutex<Option<YieldReason>> = Mutex::new(None);

#[inline]
pub fn set_yield_reason(reason: ScheduleReason) {
    *YIELD_REASON.lock() = Some(YieldReason(reason));
}

pub unsafe fn trigger_yield() {
    unsafe { interrupts::trigger::<YIELD_INTERRUPT_NUMBER>() };
}

pub(super) const YIELD_INTERRUPT_NUMBER: u8 = 0xF8;
