//! Architecture-independent execution context interface.
//!
//! This module exposes ONLY portable concepts.
//! All x86_64 details are private to this file.

#![cfg(target_arch = "x86_64")]

use core::arch::asm;

/* ==========================================================
 * Public API (arch-independent)
 * ========================================================== */

/// Opaque execution context of the current CPU.
#[derive(Clone)]
pub struct Context {
    regs: Regs,
    ctrl: Ctrl,
    fpu: Fpu,
}

/// Capture the current execution context.
#[inline]
pub fn current() -> Context {
    capture()
}

impl Context {
    /// Instruction pointer / program counter.
    #[inline]
    pub fn instruction_pointer(&self) -> u64 {
        self.regs.ip()
    }

    /// Stack pointer.
    #[inline]
    pub fn stack_pointer(&self) -> u64 {
        self.regs.sp()
    }

    /// Address-space identifier (CR3 / TTBR / SATP).
    #[inline]
    pub fn address_space(&self) -> u64 {
        self.ctrl.address_space()
    }
}

/* ==========================================================
 * Private x86_64 implementation
 * ========================================================== */

#[derive(Clone)]
struct Regs {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rsp: u64,
    r8:  u64,
    r9:  u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rip: u64,
    rflags: u64,
}

#[derive(Clone)]
struct Ctrl {
    cr0: u64,
    cr2: u64,
    cr3: u64,
    cr4: u64,
}

#[derive(Clone)]
struct Fpu {
    data: [u8; 512],
}

/* ==========================================================
 * Capture glue
 * ========================================================== */

fn capture() -> Context {
    Context {
        regs: capture_regs(),
        ctrl: capture_ctrl(),
        fpu: capture_fpu(),
    }
}

/* ==========================================================
 * Semantic accessors (still private)
 * ========================================================== */

impl Regs {
    #[inline]
    fn ip(&self) -> u64 {
        self.rip
    }

    #[inline]
    fn sp(&self) -> u64 {
        self.rsp
    }
}

impl Ctrl {
    #[inline]
    fn address_space(&self) -> u64 {
        self.cr3
    }
}

/* ==========================================================
 * Low-level x86_64 capture
 * ========================================================== */

fn capture_regs() -> Regs {
    let mut r = Regs {
        rax: 0, rbx: 0, rcx: 0, rdx: 0,
        rsi: 0, rdi: 0, rbp: 0, rsp: 0,
        r8:  0, r9:  0, r10: 0, r11: 0,
        r12: 0, r13: 0, r14: 0, r15: 0,
        rip: 0, rflags: 0,
    };

    unsafe {
        asm!(
            "mov {0}, rax",
            "mov {1}, rbx",
            "mov {2}, rcx",
            "mov {3}, rdx",
            "mov {4}, rsi",
            "mov {5}, rdi",
            "mov {6}, rbp",
            "mov {7}, rsp",
            "mov {8}, r8",
            "mov {9}, r9",
            "mov {10}, r10",
            "mov {11}, r11",
            "mov {12}, r12",
            "mov {13}, r13",
            "mov {14}, r14",
            "mov {15}, r15",
            "pushfq; pop {16}",
            out(reg) r.rax,
            out(reg) r.rbx,
            out(reg) r.rcx,
            out(reg) r.rdx,
            out(reg) r.rsi,
            out(reg) r.rdi,
            out(reg) r.rbp,
            out(reg) r.rsp,
            out(reg) r.r8,
            out(reg) r.r9,
            out(reg) r.r10,
            out(reg) r.r11,
            out(reg) r.r12,
            out(reg) r.r13,
            out(reg) r.r14,
            out(reg) r.r15,
            out(reg) r.rflags,
        );
    }

    r.rip = read_rip();
    r
}

#[inline]
fn read_rip() -> u64 {
    let rip: u64;
    
    unsafe {
        asm!("lea {}, [rip]", out(reg) rip);
    }
    rip
}

fn capture_ctrl() -> Ctrl {
    let mut c = Ctrl { cr0: 0, cr2: 0, cr3: 0, cr4: 0 };

    unsafe {
        asm!(
            "mov {}, cr0",
            "mov {}, cr2",
            "mov {}, cr3",
            "mov {}, cr4",
            out(reg) c.cr0,
            out(reg) c.cr2,
            out(reg) c.cr3,
            out(reg) c.cr4,
        );
    }

    c
}

fn capture_fpu() -> Fpu {
    let mut f = Fpu { data: [0u8; 512] };

    unsafe {
        asm!("fxsave [{}]", in(reg) &mut f.data);
    }

    f
}
