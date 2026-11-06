//! Global Descriptor Table (GDT) and Task State Segment (TSS) setup.
//!
//! This module defines all segment descriptors required by Ion OS:
//! - Kernel code / data
//! - User code / data
//! - Task State Segment (TSS)
//!
//! It is fully compatible with:
//! - long mode
//! - interrupts & IST
//! - syscall/sysret
//!
//! Segmentation is used only for privilege and CPU transitions;
//! paging remains the primary isolation mechanism.

use lazy_static::lazy_static;
use x86_64::{
    VirtAddr,
    registers::segmentation::Segment,
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
};

use crate::arch::interrupts::page_fault::PAGE_FAULT_IST_INDEX;
use crate::arch::interrupts::{
    double_fault::DOUBLE_FAULT_IST_INDEX, gp::GP_IST_INDEX, thread::TIMER_IST,
};

const IST_STACK_SIZE: usize = 32 * 1024;

lazy_static! {
    /// Global Task State Segment.
    ///
    /// Holds stack pointers for fault handlers.
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        // Double fault IST
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            static mut STACK: [u8; IST_STACK_SIZE] = [0; IST_STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + IST_STACK_SIZE as u64
        };

        // Page fault IST
        tss.interrupt_stack_table[PAGE_FAULT_IST_INDEX as usize] = {
            static mut STACK: [u8; IST_STACK_SIZE] = [0; IST_STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + IST_STACK_SIZE as u64
        };

        // General Protection fault IST
        tss.interrupt_stack_table[GP_IST_INDEX as usize] = {
            static mut STACK: [u8; IST_STACK_SIZE] = [0; IST_STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + IST_STACK_SIZE as u64
        };

        tss.interrupt_stack_table[TIMER_IST as usize] = {
            static mut STACK: [u8; IST_STACK_SIZE] = [0; IST_STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + IST_STACK_SIZE as u64
        };

        tss
    };
}

/// GDT segment selectors used throughout the kernel.
#[derive(Debug, Clone, Copy)]
pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
    pub tss: SegmentSelector,
}

lazy_static! {
    /// Global Descriptor Table and its selectors.
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        let kernel_code = gdt.append(Descriptor::kernel_code_segment());
        let kernel_data = gdt.append(Descriptor::kernel_data_segment());

        let user_data = gdt.append(Descriptor::user_data_segment());
        let user_code = gdt.append(Descriptor::user_code_segment());

        let tss = gdt.append(Descriptor::tss_segment(&TSS));

        (
            gdt,
            Selectors {
                kernel_code,
                kernel_data,
                user_code,
                user_data,
                tss,
            },
        )
    };
}

/// Initialize the GDT and load the TSS.
///
/// Must be called once during early kernel initialization,
/// before enabling syscalls or interrupts.
pub fn init() {
    use x86_64::instructions::{
        segmentation::{CS, DS, ES, SS},
        tables::load_tss,
    };

    GDT.0.load();

    unsafe {
        CS::set_reg(GDT.1.kernel_code);
        DS::set_reg(GDT.1.kernel_data);
        ES::set_reg(GDT.1.kernel_data);
        SS::set_reg(GDT.1.kernel_data);

        load_tss(GDT.1.tss);
    }
}

/// Get the active GDT selectors.
///
/// Intended for syscall initialization and low-level CPU setup.
#[inline]
pub fn selectors() -> Selectors {
    GDT.1
}
