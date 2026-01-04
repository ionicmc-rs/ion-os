use core::sync::atomic::Ordering;

use crate::{interrupts::pic8259::handlers::notify, serial::dbg, thread::{INIT, NEED_RESCHED, SCHED, ThreadState, switch_interrupt_context_raw}};

/// Naked assembly entry. Saves volatile registers, calls `timer_isr_handler`,
/// restores registers and iretq if the handler returns (no switch).
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timer_isr_entry() {
    core::arch::naked_asm!(
        // Save volatile caller-saved registers the Rust handler may clobber.
        // Keep the set minimal but safe for x86_64 SysV: rax, rcx, rdx, rsi, rdi, r8-r11.
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "call {b_log}",
        // Align stack if needed (we assume it is already aligned by hardware).
        // Call the Rust handler (C ABI)
        "call {handler}",
        // If handler returns, restore volatile regs and iretq (normal interrupt return).
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rax",
        // Return from interrupt normally (handler returned and did not switch).
        "call {a_log}",
        "iretq",
        handler = sym timer_isr_handler,
        b_log = sym b_log,
        a_log = sym a_log
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn b_log() {
    dbg::str("pushed registers");
}

pub extern "C" fn a_log() {
    dbg::str("returning from timer interrupt.");
}

/// Rust-side handler called from the naked ISR entry.
/// This runs with interrupts disabled (hardware entry). It may call
/// `switch_interrupt_context_raw` which performs `iretq` and never returns.
#[unsafe(no_mangle)]
#[allow(static_mut_refs, unused_unsafe)]
pub extern "C" fn timer_isr_handler() {
    // Mark need-resched
    NEED_RESCHED.store(true, Ordering::Release);
    
    // If we need to reschedule, pick next and switch.
    // This mirrors your previous logic but runs from a non-compiler-managed frame.
    unsafe {
        if NEED_RESCHED.swap(false, Ordering::Acquire) && INIT.load(Ordering::Relaxed) {
            dbg::str("switching stack now.");
            let me = match SCHED.current_id {
                Some(id) => id,
                None => {
                    dbg::str("Nothing to switch to.");
                    notify!(unsafe Timer);
                    return;
                }, // nothing to preempt
            };

            let next = SCHED.pick_next_ready(me);
            if next != me {
                dbg::str("Found next");
                // mark current ready and re-enqueue
                {
                    let me_tcb = SCHED.tcb.get_mut(&me).unwrap();
                    if matches!(me_tcb.state, ThreadState::Running) {
                        me_tcb.state = ThreadState::Ready;
                        SCHED.runq.push_back(me);
                    }
                }

                SCHED.current_id = Some(next);

                // Sanity checks before switching: alignment and frame contents
                let new_sp_ptr: *const *mut u8 = &SCHED.tcb.get(&next).unwrap().stack_ptr;
                let rsp = unsafe { *new_sp_ptr } as *const u64;
                debug_assert_eq!((rsp as usize) & 0xF, 0, "iretq entry must be 16-byte aligned");

                // Optional debug dump (uncomment while debugging)
                let rip = unsafe { *rsp.add(0) };
                let cs  = unsafe { *rsp.add(1) };
                let rf  = unsafe { *rsp.add(2) };
                dbg::fmt(format_args!("next frame: RIP={:#x} CS={:#x} RFLAGS={:#x}", rip, cs as u16, rf));

                let old_sp_ptr: *mut *mut u8 = &mut SCHED.tcb.get_mut(&me).unwrap().stack_ptr;
                // Call the naked iretq switcher. This will not return.
                switch_interrupt_context_raw(old_sp_ptr, new_sp_ptr);
                core::hint::unreachable_unchecked();
            }
        }
    }
    notify!(unsafe Timer);
    // If we get here, we did not switch; return to the naked entry which will iretq.
}