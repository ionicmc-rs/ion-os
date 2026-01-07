#![allow(missing_docs)]

use crate::{hlt_loop, serial_println};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Context {
    /// Stack pointer
    pub rsp: usize,

    /// Callee-saved registers (System V ABI)
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
}

impl Context {
    /// Create a fresh context that will start executing `entry(arg)`
    pub fn new(
        entry: extern "C" fn(usize) -> !,
        arg: usize,
        stack_top: usize,
    ) -> Self {
        serial_println!("creating and aligning stack");

        // SysV ABI: stack must be 16-byte aligned *before call*
        let mut rsp = stack_top & !0xF;

        unsafe {
            // Fake return address → thread_exit
            rsp -= core::mem::size_of::<usize>();
            *(rsp as *mut usize) = thread_exit as *const () as usize;

            // Initial return target → thread_trampoline
            rsp -= core::mem::size_of::<usize>();
            *(rsp as *mut usize) = thread_trampoline as *const () as usize;
        }

        serial_println!("constructed initial context");

        Self {
            rsp,
            r15: entry as *const () as u64, // entry function
            r12: arg as u64,   // argument
            r14: 0,
            r13: 0,
            rbx: 0,
            rbp: 0,
        }
    }

    pub fn current() -> Self {
        let rsp: usize;
        let r15: u64;
        let r14: u64;
        let r13: u64;
        let r12: u64;
        let rbx: u64;
        let rbp: u64;

        unsafe {
            core::arch::asm!(
                "mov {0}, rsp",
                "mov {1}, r15",
                "mov {2}, r14",
                "mov {3}, r13",
                "mov {4}, r12",
                "mov {5}, rbx",
                "mov {6}, rbp",
                out(reg) rsp,
                out(reg) r15,
                out(reg) r14,
                out(reg) r13,
                out(reg) r12,
                out(reg) rbx,
                out(reg) rbp,
                options(nomem, nostack)
            )
        }

        Self { rsp, r15, r14, r13, r12, rbx, rbp }
    }

    /// Switch from this context to `next`
    ///
    /// # Safety
    /// Must be called only by the scheduler at a safe point.
    pub unsafe fn switch_to(&mut self, next: &Context) {
        serial_println!("context switch {:#x} -> {:#x}", self.rsp, next.rsp);
        unsafe { context_switch(self, next) };
    }
}



/// Called if a thread function ever returns
extern "C" fn thread_exit() -> ! {
    serial_println!("thread exited");
    crate::scheduler::exit_current();
    hlt_loop()
}

unsafe extern "C" {
    /// Save current context into `old`, restore `new`
    ///
    /// Implemented in assembly.
    pub unsafe fn context_switch(old: *mut Context, new: *const Context);
    pub unsafe fn thread_trampoline() -> !;
}
