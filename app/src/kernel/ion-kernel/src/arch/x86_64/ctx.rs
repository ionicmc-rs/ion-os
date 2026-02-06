//! Architecture-independent execution context interface.
//!
//! This module exposes ONLY portable concepts.
//! All x86_64 details are private to this file.

#![cfg(target_arch = "x86_64")]

use core::{arch::{asm, naked_asm}, mem::offset_of};

use bitflags::Flags;
use linked_list_allocator::align_up_size;
use x86_64::registers::rflags::{self, RFlags};

use crate::{address_space, log, thread};

/* ==========================================================
 * Public API (arch-independent)
 * ========================================================== */

/// Opaque execution context of the current CPU.
#[derive(Clone, Debug)]
pub struct Context {
    regs: Regs,
    ctrl: Ctrl,
    /// Unused for now
    fpu: Fpu,
}

/// Capture the current execution context.
#[inline]
pub fn current() -> Context {
    capture()
}

impl Context {
    /// creates a "zero"ed context
    /// 
    /// Note: this function actually fills the context with data - a real zeroed context would be 
    /// useless/start the system from the beginning (which would cause double-init)
    /// 
    /// on x86_64, this inherits the control registers from the current context
    pub fn initial(entry: usize, arg: usize, stack: usize) -> Self {
        let mut rsp = (align_up_size(stack, 16)) as u64;

        rsp -= size_of::<usize>() as u64;
        unsafe { (rsp as *mut usize).write_volatile(exit as *const () as usize); }

        extern "C" fn exit() -> ! {
            thread::exit_current();
        }

        log::debug!("Zeroed context being created");
        Self { 
            regs: Regs { 
                rip: entry as u64, 
                rdi: arg as u64, 
                rsp, 
                rflags: 0x202, 
                ..Default::default() 
            }, 
            ctrl: capture_ctrl(), 
            fpu: Fpu { 
                data: [0; 512] 
            } 
        }
    }

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

    pub fn set_addr_space(&mut self, address_space: address_space::AddressSpace) {
        self.ctrl.cr3 = address_space.ptr().as_ptr_ty();
    }
}

// load/save

impl Context {
    /// Load this context assuming the stack already contains a valid call frame.
    pub fn load(&self) {
        log::debug!(
            "{:#?}",
            self.regs
        );
        unsafe {
            log::debug!("restoring address space");
            // Switch address space
            asm!(
                "mov cr3, {}",
                in(reg) self.ctrl.cr3,
                options(nostack),
            );
            log::debug!("restored");

            // Restore FPU
            asm!(
                "fxrstor [{}]",
                in(reg) &self.fpu.data,
                options(nostack),
            );

            let r = &self.regs;

            log::debug!("offsets: {:#x}, {:#x}, {:#x}", offset_of!(Regs, rsp), offset_of!(Regs, rflags), offset_of!(Regs, rip));

            log::debug!("restoring regs");
            asm!(
                // Save the current RBP so we can use it as our stable base pointer
                "push rbp",
                "mov rbp, {r}", 

                // 1. Move to the new stack immediately
                "mov r14, [rbp + {rsp_off}]",
                "mov rsp, r14",
                "sub rsp, 8",

                // 2. Setup the jump target and flags on the NEW stack
                "mov r15, [rbp + {rip_off}]",
                "push r15",
                "mov r15, [rbp + {rflags_off}]",
                "push r15",

                // 3. Restore all GPRs using RBP (except RBP itself and RSP)
                "mov rax, [rbp + {rax_off}]",
                "mov rbx, [rbp + {rbx_off}]",
                "mov rcx, [rbp + {rcx_off}]",
                "mov rdx, [rbp + {rdx_off}]",
                "mov rsi, [rbp + {rsi_off}]",
                "mov rdi, [rbp + {rdi_off}]",
                "mov r8,  [rbp + {r8_off}]",
                "mov r9,  [rbp + {r9_off}]",
                "mov r10, [rbp + {r10_off}]",
                "mov r11, [rbp + {r11_off}]",
                "mov r12, [rbp + {r12_off}]",
                "mov r13, [rbp + {r13_off}]",
                "mov r14, [rbp + {r14_off}]",
                "mov r15, [rbp + {r15_off}]",

                // 4. Finally, restore RBP from the struct
                "mov rbp, [rbp + {rbp_off}]",

                // 5. Final transition
                "popfq", // Pops the flags we pushed in step 2
                "ret",   // Pops the RIP we pushed in step 2

                r = in(reg) r,
                rax_off    = const offset_of!(Regs, rax),
                rbx_off    = const offset_of!(Regs, rbx),
                rcx_off    = const offset_of!(Regs, rcx),
                rdx_off    = const offset_of!(Regs, rdx),
                rsi_off    = const offset_of!(Regs, rsi),
                rdi_off    = const offset_of!(Regs, rdi),
                rbp_off    = const offset_of!(Regs, rbp),
                rsp_off    = const offset_of!(Regs, rsp),
                r8_off     = const offset_of!(Regs, r8),
                r9_off     = const offset_of!(Regs, r9),
                r10_off    = const offset_of!(Regs, r10),
                r11_off    = const offset_of!(Regs, r11),
                r12_off    = const offset_of!(Regs, r12),
                r13_off    = const offset_of!(Regs, r13),
                r14_off    = const offset_of!(Regs, r14),
                r15_off    = const offset_of!(Regs, r15),
                rip_off    = const offset_of!(Regs, rip),
                rflags_off = const offset_of!(Regs, rflags),
                options(noreturn)
            );
        }
    }

    /// Load this context by mutating an interrupt frame.
    ///
    /// The interrupt frame must have been entered *after* a call frame was pushed.
    /// The call frame is consumed and execution resumes as if returning normally.
    pub(in crate::arch::x86_64) fn load_with_interrupt_frame(&self, frame: &mut x86_64::structures::idt::InterruptStackFrameValue) {
        unsafe {
            // Switch address space
            asm!(
                "mov cr3, {}",
                in(reg) self.ctrl.cr3,
                options(nostack, preserves_flags),
            );

            // Restore FPU
            // asm!(
            //     "fxrstor [{}]",
            //     in(reg) &self.fpu.data,
            //     options(nostack),
            // );

            let r = &self.regs;

            // Restore GP registers (except rsp)
            asm!(
                "mov rax, [{r} + 0x00]",
                "mov rbx, [{r} + 0x08]",
                "mov rcx, [{r} + 0x10]",
                "mov rdx, [{r} + 0x18]",
                "mov rsi, [{r} + 0x20]",
                "mov rdi, [{r} + 0x28]",
                "mov rbp, [{r} + 0x30]",
                "mov r8,  [{r} + 0x40]",
                "mov r9,  [{r} + 0x48]",
                "mov r10, [{r} + 0x50]",
                "mov r11, [{r} + 0x58]",
                "mov r12, [{r} + 0x60]",
                "mov r13, [{r} + 0x68]",
                "mov r14, [{r} + 0x70]",
                "mov r15, [{r} + 0x78]",
                r = in(reg) r,
                options(nostack),
            );

            // Extract return RIP from call frame (pushed before interrupt)
            let call_rip = *(frame.instruction_pointer.as_ptr());

            // Rewrite interrupt frame
            frame.instruction_pointer = x86_64::VirtAddr::new(call_rip);
            frame.cpu_flags = RFlags::from_bits_truncate(self.regs.rflags);
            frame.stack_pointer += 8; // consume call frame

            // CS / SS intentionally unchanged
            // Final control transfer happens via iretq by caller
        }
    }

    /// Save the current execution context into this struct.
    #[unsafe(naked)]
    pub extern "C" fn save(&mut self) {
        naked_asm!(
            "mov rsi, [rip + 0f]",
            "call {f}",
            "0:",
            "ret",
            f = sym Self::internal_save
        )
    }

    extern "C" fn internal_save(&mut self, addr: u64) {
        log::debug!("saving context");
        self.ctrl = capture_ctrl();
        self.fpu = capture_fpu();
        self.regs = capture_regs();
        log::debug!("context rip: {addr}");
        self.regs.rip = addr;
        self.regs.rflags = RFlags::from_bits_truncate(self.regs.rflags).bits();
    }
}

/* ==========================================================
 * Private x86_64 implementation
 * ========================================================== */

#[derive(Clone, Debug, Default)]
#[repr(C)]
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

#[repr(C, align(16))]
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
