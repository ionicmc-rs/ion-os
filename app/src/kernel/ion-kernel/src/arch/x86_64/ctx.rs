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
#[derive(Clone, Debug)]
pub struct Context {
    regs: Regs,
    ctrl: Ctrl,
    /// Unused for now
    _fpu: Fpu,
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

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
struct Ctrl {
    cr0: u64,
    cr2: u64,
    cr3: u64,
    cr4: u64,
}

#[derive(Clone, Debug)]
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
        _fpu: capture_fpu(),
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
            "mov [{0} + 0x00], rax",
            "mov [{0} + 0x08], rbx",
            "mov [{0} + 0x10], rcx",
            "mov [{0} + 0x18], rdx",
            "mov [{0} + 0x20], rsi",
            "mov [{0} + 0x28], rdi",
            "mov [{0} + 0x30], rbp",
            "mov [{0} + 0x38], rsp",
            "mov [{0} + 0x40], r8",
            "mov [{0} + 0x48], r9",
            "mov [{0} + 0x50], r10",
            "mov [{0} + 0x58], r11",
            "mov [{0} + 0x60], r12",
            "mov [{0} + 0x68], r13",
            "mov [{0} + 0x70], r14",
            "mov [{0} + 0x78], r15",
            "pushfq",
            "pop qword ptr [{0} + 0x88]",
            in(reg) &mut r,
            options(nostack, preserves_flags),
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
