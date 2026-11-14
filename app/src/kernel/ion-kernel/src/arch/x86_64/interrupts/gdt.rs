//! Global Descriptor Table (GDT) and per-CPU Task State Segment setup.

use core::mem::MaybeUninit;

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

const MAX_CPUS: usize = 64;
const IST_STACK_SIZE: usize = 32 * 1024;
const IST_STACKS_PER_CPU: usize = 4;

/// Each CPU must have its own TSS: the descriptor becomes busy after `ltr`,
/// and each CPU needs private IST stacks for nested/fault interrupts.
static mut TSS: [MaybeUninit<TaskStateSegment>; MAX_CPUS] =
    [const { MaybeUninit::uninit() }; MAX_CPUS];
static mut GDT: [MaybeUninit<(GlobalDescriptorTable, Selectors)>; MAX_CPUS] =
    [const { MaybeUninit::uninit() }; MAX_CPUS];
static mut IST_STACKS: [[[u8; IST_STACK_SIZE]; IST_STACKS_PER_CPU]; MAX_CPUS] =
    [[[0; IST_STACK_SIZE]; IST_STACKS_PER_CPU]; MAX_CPUS];

/// GDT segment selectors used throughout the kernel.
#[derive(Debug, Clone, Copy)]
pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
    pub tss: SegmentSelector,
}

/// Initialize this CPU's GDT and load its TSS.
///
/// Must be called once on each CPU before enabling interrupts on that CPU.
pub fn init_cpu(cpu_index: usize) {
    use x86_64::instructions::{
        segmentation::{CS, DS, ES, SS},
        tables::load_tss,
    };

    unsafe {
        let mut tss = TaskStateSegment::new();
        let stacks_base = core::ptr::addr_of_mut!(IST_STACKS).cast::<u8>();

        for (stack_number, ist_index) in [
            DOUBLE_FAULT_IST_INDEX,
            PAGE_FAULT_IST_INDEX,
            GP_IST_INDEX,
            TIMER_IST,
        ]
        .into_iter()
        .enumerate()
        {
            let stack = stacks_base.add(
                (cpu_index * IST_STACKS_PER_CPU + stack_number) * IST_STACK_SIZE,
            );
            let stack_start = VirtAddr::from_ptr(stack);
            tss.interrupt_stack_table[ist_index as usize] = stack_start + IST_STACK_SIZE as u64;
        }

        TSS[cpu_index].write(tss);
        let tss = TSS[cpu_index].assume_init_ref();

        let mut gdt = GlobalDescriptorTable::new();
        let kernel_code = gdt.append(Descriptor::kernel_code_segment());
        let kernel_data = gdt.append(Descriptor::kernel_data_segment());
        let user_data = gdt.append(Descriptor::user_data_segment());
        let user_code = gdt.append(Descriptor::user_code_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(tss));

        GDT[cpu_index].write((
            gdt,
            Selectors {
                kernel_code,
                kernel_data,
                user_code,
                user_data,
                tss: tss_selector,
            },
        ));

        let (gdt, selectors) = GDT[cpu_index].assume_init_ref();
        gdt.load();

        CS::set_reg(selectors.kernel_code);
        DS::set_reg(selectors.kernel_data);
        ES::set_reg(selectors.kernel_data);
        SS::set_reg(selectors.kernel_data);
        load_tss(selectors.tss);
    }
}

/// Get the active CPU's segment selectors.
#[inline]
pub fn selectors() -> Selectors {
    // Selectors have identical values in each per-CPU GDT.
    unsafe { GDT[0].assume_init_ref().1 }
}
