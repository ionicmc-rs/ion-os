//! Architecture-independent execution context interface.
//!
//! This module exposes ONLY portable concepts.
//! All x86_64 details are private to this file.

#![cfg(target_arch = "x86_64")]

use core::{arch::{asm, naked_asm}, fmt, mem::{offset_of, size_of}};

use bitflags::Flags;
use linked_list_allocator::align_up_size;
use volatile::Volatile;
use x86_64::{VirtAddr, registers::rflags::{self, RFlags}, structures::{gdt::SegmentSelector, idt::InterruptStackFrameValue}};

use crate::{address_space, arch::interrupts::InterruptRegs, log, thread};

/* ==========================================================
 * Public API (arch-independent)
 * ========================================================== */

/// Opaque execution context of the current CPU.
#[derive(Clone)]
pub struct Context {
    regs: Regs,
    ctrl: Ctrl,
    fpu: Fpu,
    segments: Segments
}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("registers", &self.regs)
            .field("control", &self.ctrl)
            .field("segmentation", &self.segments)
            .field_with("fpu", |f|
                f.debug_list().finish_non_exhaustive()
            )
            .finish()
    }
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
        unsafe { *(rsp as *mut usize) = exit as *const () as usize };

        if rsp % 16 != 0 {
            rsp -= 8;
        }

        extern "C" fn exit() -> ! {
            thread::exit_current();
        }

        Self { 
            regs: Regs { 
                rip: entry as u64, 
                rdi: arg as u64, 
                rsp, 
                rbp: 0,
                rflags: 0x202, 
                ..Default::default() 
            }, 
            ctrl: capture_ctrl(), 
            fpu: capture_fpu(),
            segments: Segments {
                cs: super::interrupts::gdt_selectors().kernel_code.0,
                ss: super::interrupts::gdt_selectors().kernel_data.0,
            }
        }
    }

    pub fn user(entry: usize, arg: usize, stack: usize) -> Self {
        let mut ctx = Self::initial(entry, arg, stack);
        let mut seg_cs = super::interrupts::gdt_selectors().user_code;
        seg_cs.set_rpl(x86_64::PrivilegeLevel::Ring3);
        ctx.segments.cs = seg_cs.0;

        let mut seg_ss = super::interrupts::gdt_selectors().user_data;
        seg_ss.set_rpl(x86_64::PrivilegeLevel::Ring3);
        ctx.segments.ss = seg_ss.0;

        ctx.regs.rsp += 8; // exit is useless because its kernel mapped

        ctx
    }

    /// Instruction pointer / program counter.
    #[inline]
    pub fn instruction_pointer(&self) -> usize {
        self.regs.ip() as usize
    }

    /// Stack pointer.
    #[inline]
    pub fn stack_pointer(&self) -> usize {
        self.regs.sp() as usize
    }

    /// Address-space identifier (CR3 / TTBR / SATP).
    #[inline]
    pub fn address_space(&self) -> usize {
        self.ctrl.address_space() as usize
    }
    
    pub fn set_addr_space(&mut self, address_space: address_space::AddressSpace) {
        self.ctrl.cr3 = address_space.ptr().as_ptr_ty();
    }
}
const KERNEL_RFLAGS_MASK: u64 =
    RFlags::INTERRUPT_FLAG.bits()
| RFlags::ZERO_FLAG.bits()
| RFlags::SIGN_FLAG.bits()
| RFlags::PARITY_FLAG.bits()
| 0x2; // reserved bit

// load/save

impl Context {
    pub(in crate::arch::x86_64) fn load_with_interrupt_frame(&self, regs_on_stack: &mut super::interrupts::InterruptRegs, mut stack_frame: Volatile<&mut InterruptStackFrameValue>) {
        unsafe {
            // Switch address space
            asm!(
                "mov cr3, {}",
                in(reg) self.ctrl.cr3,
                options(nostack),
            );

            // Restore FPU
            asm!(
                "clts",
                // "fxrstor64 [{}]",
                // in(reg) &self.fpu.data,
                options(nostack),
            );
        }
    
        // Overwrite the GPRs on the stack
        regs_on_stack.r15 = self.regs.r15;
        regs_on_stack.r14 = self.regs.r14;
        regs_on_stack.r13 = self.regs.r13;
        regs_on_stack.r12 = self.regs.r12;
        regs_on_stack.r11 = self.regs.r11;
        regs_on_stack.r10 = self.regs.r10;
        regs_on_stack.r9 = self.regs.r9;
        regs_on_stack.r8 = self.regs.r8;
        regs_on_stack.rdi = self.regs.rdi;
        regs_on_stack.rsi = self.regs.rsi;
        regs_on_stack.rbp = self.regs.rbp;
        regs_on_stack.rdx = self.regs.rdx;
        regs_on_stack.rcx = self.regs.rcx;
        regs_on_stack.rbx = self.regs.rbx;
        regs_on_stack.rax = self.regs.rax;
    
        // Overwrite the iretq frame
        stack_frame.update(|frame_value| {
            frame_value.instruction_pointer = VirtAddr::new(self.regs.rip);
            frame_value.stack_pointer = VirtAddr::new(self.regs.rsp);
            frame_value.cpu_flags = RFlags::from_bits_retain(self.regs.rflags);
            frame_value.code_segment = SegmentSelector(self.segments.cs);
            frame_value.stack_segment = SegmentSelector(self.segments.ss);
        });
    }

    pub(in crate::arch::x86_64) fn save_from_interrupt(&mut self, regs: &InterruptRegs, stack_frame: Volatile<&mut InterruptStackFrameValue, volatile::access::ReadOnly>) {
        self.ctrl = capture_ctrl();
        self.fpu = capture_fpu();
        self.regs.r8 = regs.r8;
        self.regs.r9 = regs.r9;
        self.regs.r10 = regs.r10;
        self.regs.r11 = regs.r11;
        self.regs.r12 = regs.r12;
        self.regs.r13 = regs.r13;
        self.regs.r14 = regs.r14;
        self.regs.r15 = regs.r15;
        self.regs.rax = regs.rax;
        self.regs.rbx = regs.rbx;
        self.regs.rbp = regs.rbp;
        self.regs.rcx = regs.rcx;
        self.regs.rdi = regs.rdi;
        self.regs.rdx = regs.rdx;
        self.regs.rsi = regs.rsi;

        let frame = stack_frame.read();

        self.regs.rip = frame.instruction_pointer.as_u64();
        self.regs.rsp = frame.stack_pointer.as_u64();
        self.regs.rflags = frame.cpu_flags.bits();
        self.segments.cs = frame.code_segment.0;
        self.segments.ss = frame.stack_segment.0;
    }
}

/* ==========================================================
 * Private x86_64 implementation
 * ========================================================== */

#[derive(Debug, Clone, Copy)]
struct Segments {
    cs: u16,
    ss: u16
}

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
        segments: capture_segments(),
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

fn capture_segments() -> Segments {
    let mut s = Segments { cs: 0, ss: 0 };

    unsafe {
        // use :x for 16-bit values
        asm!("mov {:x}, cs", out(reg) s.cs);
        asm!("mov {:x}, ss", out(reg) s.ss);
    }

    s
}

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

    // unsafe {
    //     asm!(
    //         "clts",
    //         "fxsave64 [{}]", 
    //         in(reg) f.data.as_mut_ptr()
    //     );
    // }

    f
}
