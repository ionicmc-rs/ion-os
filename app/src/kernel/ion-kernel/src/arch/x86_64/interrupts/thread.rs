use core::{
    arch::naked_asm,
    sync::atomic::{AtomicU64, Ordering},
};

use spin::Mutex;
use x86_64::structures::idt::InterruptStackFrame;

use crate::{
    arch::interrupts::{self, apic::handlers::notify, without_interrupts},
    scheduler::{self, ScheduleReason, current_scheduler},
    thread,
};

// pub extern "x86-interrupt" fn timer_isr_entry(_: InterruptStackFrame) {
//     // NOTE: for now, this simply requests rescheduling, but this may change in the future to actually reschedule here.
//     TICKS.fetch_add(1, Ordering::Relaxed);

//     scheduler::wake_done();

//     notify!(unsafe Timer)
// }

static TICKS: AtomicU64 = AtomicU64::new(0);
const MAX_CPUS: usize = 64;

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


        // IST top is aligned; hardware's 40-byte frame and 15 register
        // pushes leave RSP aligned before the SysV call.
        "sub rsp, 512",
        "clts",
        "fxsave64 [rsp]",
        "lea rdi, [rsp + 512]", // &mut InterruptRegs
        "lea rsi, [rsp + 632]", // InterruptStackFrame
        "mov rdx, rsp",          // pre-handler FPU image
        "call {handler}",
        "add rsp, 512",


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
pub extern "C" fn timer_handler(
    regs: &mut InterruptRegs,
    frame: &mut InterruptStackFrame,
    fpu_state: *const u8,
) {
    let t = TICKS.fetch_add(1, Ordering::Relaxed);

    if thread::current().is_none() {
        // The timer is enabled before the first scheduler thread starts.
        // Acknowledge that early interrupt too, or the LAPIC leaves it in
        // service and will not deliver subsequent timer ticks.
        notify!(unsafe Timer);
        unsafe { restore_fpu(fpu_state) };
        return;
    }

    // Finish interrupt-side work before restoring another thread's FPU state.
    scheduler::request_reschedule();
    scheduler::wake_done();
    notify!(unsafe Timer);

    let switched =
        current_scheduler().switch_from_interrupt(ScheduleReason::Preempt, regs, frame, fpu_state);
    if !switched {
        unsafe { restore_fpu(fpu_state) };
    }
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

        // IST top is aligned; hardware's 40-byte frame and 15 register
        // pushes leave RSP aligned before the SysV call.
        "sub rsp, 512",
        "clts",
        "fxsave64 [rsp]",
        "lea rdi, [rsp + 512]", // &mut InterruptRegs
        "lea rsi, [rsp + 632]", // InterruptStackFrame
        "mov rdx, rsp",          // pre-handler FPU image
        "call {handler}",
        "add rsp, 512",

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

extern "C" fn yield_handler(
    regs: &mut InterruptRegs,
    frame: &mut InterruptStackFrame,
    fpu_state: *const u8,
) {
    let switched = without_interrupts(|| {
        if let Some(mut lock) = current_yield_reason().try_lock() {
            if let Some(reason) = lock.take() {
                return current_scheduler().switch_from_interrupt(reason.0, regs, frame, fpu_state);
            }
        }
        false
    });
    if !switched {
        unsafe { restore_fpu(fpu_state) };
    }
}

unsafe fn restore_fpu(state: *const u8) {
    unsafe {
        core::arch::asm!(
            "clts",
            "fxrstor64 [{}]",
            in(reg) state,
            clobber_abi("C"),
            options(nostack)
        );
    }
}

// yield arguments

pub struct YieldReason(pub ScheduleReason);

unsafe impl Sync for YieldReason {}
unsafe impl Send for YieldReason {}

static YIELD_REASON: [Mutex<Option<YieldReason>>; MAX_CPUS] =
    [const { Mutex::new(None) }; MAX_CPUS];

fn current_yield_reason() -> &'static Mutex<Option<YieldReason>> {
    let apic_id = unsafe { core::arch::x86_64::__cpuid(1).ebx >> 24 };
    let index = crate::arch::smp::cpu_index_from_apic_id(apic_id)
        .expect("yield interrupt on an undiscovered CPU");
    &YIELD_REASON[index]
}

#[inline]
pub fn set_yield_reason(reason: ScheduleReason) {
    *current_yield_reason().lock() = Some(YieldReason(reason));
}

pub unsafe fn trigger_yield() {
    unsafe { interrupts::trigger::<YIELD_INTERRUPT_NUMBER>() };
}

pub(super) const YIELD_INTERRUPT_NUMBER: u8 = 0xF8;
