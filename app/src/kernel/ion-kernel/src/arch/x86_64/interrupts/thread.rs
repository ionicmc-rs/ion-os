use core::{arch::naked_asm, sync::atomic::{AtomicU8, AtomicU64, Ordering, compiler_fence, fence}};

use spin::Mutex;
use x86_64::structures::idt::InterruptStackFrame;

use crate::{
    arch::{cpu, interrupts::{self, InterruptGuard, pic8259::handlers::notify, without_interrupts}}, log, scheduler::{self, SCHEDULER, ScheduleReason}, serial_println
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
    pub r9:  u64,
    pub r8:  u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}

#[unsafe(naked)]
pub extern "x86-interrupt" fn timer_trampoline(_: InterruptStackFrame /* unused param here for Rust to not complain */) {
        naked_asm!(
            "mov rsi, rsp", // &mut InterruptStackFrame
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
    let next = SCHEDULER.pick_preempt();

    if let Some(v) = next {
        if let Some(old) = v.0 {
            old.save_from_interrupt(regs, unsafe { frame.as_mut().read_only() }); // does not need the actual memory
        }
        log::debug!("timer switch to ctx {:?}", v.1);
        v.1.load_with_interrupt_frame(regs, unsafe { frame.as_mut() });
    }

    let t = TICKS.fetch_add(1, Ordering::Relaxed);
    if t.is_multiple_of(50) {
        serial_println!("reached notable tick: {t}");
    }

    scheduler::request_reschedule();
    scheduler::wake_done();

    notify!(unsafe Timer)
}

pub(super) const TIMER_IST: u16 = 4;
pub(super) const YIELD_IST: u16 = 4;

#[unsafe(naked)]
pub extern "x86-interrupt" fn yield_trampoline(_: InterruptStackFrame /* unused param here for Rust to not complain */) {
        naked_asm!(
            "mov rsi, rsp", // &mut InterruptStackFrame
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
    let _guard = InterruptGuard::new();

    log::debug!("{regs:#?}");
    if let Some(mut lock) = YIELD_REASON.try_lock() 
        && let Some(reason) = lock.take() {

        match reason.0 {
            ScheduleReason::Yield => log::debug!("yielding"),
            ScheduleReason::Preempt => log::debug!("preempting"),
            ScheduleReason::Exit => log::debug!("exiting"),
            ScheduleReason::Block(_) => log::debug!("blocking"),
            ScheduleReason::Sleep(_) => log::debug!("sleeping"),
            ScheduleReason::Panicked => log::debug!("panicked"),
            ScheduleReason::Start => log::debug!("starting"),
        }

        if let Some(v) = SCHEDULER.pick(reason.0) {
            fence(Ordering::SeqCst);
            if let Some(old) = v.0 {
                log::debug!("saving old thread context");
                old.save_from_interrupt(regs, unsafe { frame.as_mut().read_only() });
                log::debug!("saved old thread context");
            }
            log::debug!("loading new thread context: {:?}", v.1);
            v.1.load_with_interrupt_frame(regs, unsafe { frame.as_mut() });
            log::debug!("loaded new thread context");
        }
    }
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