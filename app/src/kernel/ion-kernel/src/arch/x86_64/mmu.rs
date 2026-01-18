//! x86_64 MMU (2 MiB huge pages only)

use x86_64::registers::control::Cr3;
use crate::arch::{VirtAddr, defs};

const ENTRY_PRESENT: u64 = 1 << 0;
const ENTRY_WRITABLE: u64 = 1 << 1;
const ENTRY_HUGE: u64 = 1 << 7;

/// Raw page-table pointer (identity-mapped)
#[repr(transparent)]
#[derive(Debug)]
pub struct PageTable(*mut u64);

/// # Safety
/// - Paging must be active
/// - Page tables must be identity-mapped
pub unsafe fn active_pml4() -> PageTable {
    let (frame, _) = Cr3::read();
    PageTable(frame.start_address().as_u64() as *mut u64)
}

#[inline]
fn pml4_index(addr: u64) -> usize {
    ((addr >> 39) & 0x1ff) as usize
}

#[inline]
fn pdpt_index(addr: u64) -> usize {
    ((addr >> 30) & 0x1ff) as usize
}

#[inline]
fn pd_index(addr: u64) -> usize {
    ((addr >> 21) & 0x1ff) as usize
}

unsafe fn next_table(entry: u64) -> *mut u64 {
    (entry & 0x000f_ffff_ffff_f000) as *mut u64
}

/// Map one 2 MiB page.
///
/// # Safety
/// - `virt` and `phys` must be 2 MiB aligned
/// - Mapping must not already exist
/// - Page tables must be identity-mapped
#[allow(unsafe_op_in_unsafe_fn)]
#[track_caller]
pub unsafe fn map_2mib(
    pml4: &mut PageTable,
    virt: VirtAddr,
    phys: usize,
    writable: bool,
) {
    let v = virt.0;

    debug_assert!(v % (2 * 1024 * 1024) == 0);
    debug_assert!(phys % (2 * 1024 * 1024) == 0);

    let l4 = pml4.0;
    let l4e = l4.add(pml4_index(v));
    assert!(*l4e & ENTRY_PRESENT != 0, "PML4 entry missing");

    let l3 = next_table(*l4e);
    let l3e = l3.add(pdpt_index(v));
    if *l3e & ENTRY_PRESENT == 0 {
        let pd = alloc_zeroed_page_table();
        *l3e = (pd as u64) | ENTRY_PRESENT | ENTRY_WRITABLE;
    }


    let l2 = next_table(*l3e);
    let pde = l2.add(pd_index(v));

    assert!(*pde & ENTRY_PRESENT == 0, "PDE already mapped");

    let mut flags = ENTRY_PRESENT | ENTRY_HUGE;
    if writable {
        flags |= ENTRY_WRITABLE;
    }

    *pde = (phys as u64) | flags;

    // Invalidate TLB for this page
    core::arch::asm!("invlpg [{}]", in(reg) v, options(nostack, preserves_flags));
}

use crate::arch::frame_allocator::alloc_2mib_frame;

unsafe fn alloc_zeroed_page_table() -> *mut u64 {
    let phys = alloc_2mib_frame();
    let ptr = phys as *mut u64;

    // Zero 4 KiB (512 entries)
    for i in 0..512 {
        unsafe { ptr.add(i).write(0) };
    }

    ptr
}


use crate::lib_alloc::{HEAP_SIZE, heap_start};

/// initializes the kernel heap
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn init_kernel_heap() {
    let mut pml4 = active_pml4();

    let mut virt = heap_start();
    let end = heap_start() + HEAP_SIZE;

    while virt < end {
        let phys = alloc_2mib_frame();

        map_2mib(
            &mut pml4,
            VirtAddr(virt as defs::PointerTy),
            phys,
            true, // writable
        );

        virt += 2 * 1024 * 1024;
    }
}
