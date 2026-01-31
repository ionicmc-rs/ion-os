//! x86_64 MMU (2 MiB huge pages only)

use x86_64::registers::control::Cr3;
use crate::arch::{PhysAddr, VirtAddr, defs};

/// represents the flag for a present entry
pub const ENTRY_PRESENT: u64 = 1 << 0;
/// represents the flag for a writable entry
pub const ENTRY_WRITABLE: u64 = 1 << 1;
/// represents the flag for a "huge" page (2 MiB)
pub const ENTRY_HUGE: u64 = 1 << 7;

/// Raw page-table pointer (identity-mapped)
#[repr(transparent)]
#[derive(Debug)]
pub struct PageTable(pub *mut u64);

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


bitflags::bitflags! {
    /// Flags for page tables
    #[derive(PartialEq, Clone, Copy)]
    pub struct PageTableFlags: u64 {
        /// Page is 2MiB
        const HUGE_PAGE = ENTRY_HUGE;
        /// Page accessible by users
        /// 
        /// never apply on kernel-private data
        const USER_PAGE = 1 << 2;
        /// Page can be written to
        const WRITABLE = ENTRY_WRITABLE;
        /// Page is present
        const PRESENT = ENTRY_PRESENT;
        const NO_CACHE = 1 << 4;
        const NO_EXECUTE = 1 << 63;
    }
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
    phys: PhysAddr,
    mut flags: PageTableFlags,
) {
    // temporary
    flags &= !PageTableFlags::NO_EXECUTE;

    let v = virt.0;

    debug_assert!(v.is_multiple_of(2 * 1024 * 1024));
    debug_assert!(phys.0.is_multiple_of(2 * 1024 * 1024));

    let l4 = pml4.0;
    let l4e = l4.add(pml4_index(v));
    assert!(*l4e & ENTRY_PRESENT != 0, "PML4 entry missing");

    let l3 = next_table(*l4e);
    let l3e = l3.add(pdpt_index(v));
    if *l3e & ENTRY_PRESENT == 0 {
        let pd = alloc_zeroed_page_table();
        if flags & PageTableFlags::USER_PAGE == PageTableFlags::from_bits_truncate(4) {
            *l3e = (pd as u64) | ENTRY_PRESENT | ENTRY_WRITABLE | 1 << 2;
        } else {
            *l3e = (pd as u64) | ENTRY_PRESENT | ENTRY_WRITABLE;
        }
    }
    let l2 = next_table(*l3e);
    let pde = l2.add(pd_index(v));

    assert!(*pde & ENTRY_PRESENT == 0, "PDE already mapped");

    let flags = PageTableFlags::PRESENT | PageTableFlags::HUGE_PAGE | flags;

    *pde = phys.0 | flags.bits();

    // Invalidate TLB for this page
    core::arch::asm!("invlpg [{}]", in(reg) v, options(nostack, preserves_flags));
}

use crate::arch::frame_allocator::{alloc_2mib_frame, dealloc_2mib_frame};
use spin::Mutex;

/// A simple page table allocator that subdivides 2 MiB frames into 4 KiB page tables
/// One frame can hold 512 page tables (2 MiB / 4 KiB = 512)
struct PageTableAllocator {
    /// Current frame's base address (physical)
    current_frame: usize,
    /// Number of page tables allocated from current frame
    count_in_frame: usize,
}

// SAFETY: PageTableAllocator only contains usize, which is Send+Sync
unsafe impl Send for PageTableAllocator {}
unsafe impl Sync for PageTableAllocator {}

impl PageTableAllocator {
    const TABLES_PER_FRAME: usize = 512; // 2 MiB / 4 KiB
    const TABLE_SIZE: usize = 4 * 1024; // 4 KiB

    const fn new() -> Self {
        Self {
            current_frame: 0,
            count_in_frame: Self::TABLES_PER_FRAME, // Force allocation on first call
        }
    }

    /// Allocate a 4 KiB page table (512 u64 entries), zero-initialized
    /// Returns a *virtual* pointer (identity-mapped)
    unsafe fn allocate(&mut self) -> *mut u64 {
        // Need a new frame?
        if self.count_in_frame >= Self::TABLES_PER_FRAME {
            let phys = alloc_2mib_frame();
            self.current_frame = phys;
            self.count_in_frame = 0;
        }

        // Get pointer to this table
        // SAFETY: frame allocator returns identity-mapped physical addresses
        let ptr = (self.current_frame + self.count_in_frame * Self::TABLE_SIZE) as *mut u64;

        // Zero it (4 KiB = 512 u64 entries)
        for i in 0..512 {
            unsafe { ptr.add(i).write(0) };
        }

        self.count_in_frame += 1;
        ptr
    }
}

static PAGE_TABLE_ALLOC: Mutex<PageTableAllocator> = Mutex::new(PageTableAllocator::new());

unsafe fn alloc_zeroed_page_table() -> *mut u64 {
    let mut alloc = PAGE_TABLE_ALLOC.lock();
    unsafe { alloc.allocate() }
}


use crate::lib_alloc::{HEAP_SIZE, heap_start};

/// initializes the kernel heap
/// # Safety
/// must only be called once
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
            PhysAddr(phys as defs::PointerTy),
            PageTableFlags::WRITABLE, // writable
        );

        virt += 2 * 1024 * 1024;
    }
}

/// # Safety
/// the virtual address must be correct, and the current paging config must be correct (i.e., allow for reading)
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn virt_to_phys(virt: VirtAddr) -> Option<PhysAddr> {
    let addr = virt.0 as u64;
    
    // Get the physical root from CR3 (just like active_pml4 does)
    let (frame, _) = x86_64::registers::control::Cr3::read();
    let pml4_ptr = frame.start_address().as_u64() as *const u64;
    
    // Level 4: PML4
    let pml4e = *pml4_ptr.add(pml4_index(addr));
    if pml4e & ENTRY_PRESENT == 0 { return None; }

    // Level 3: PDPT
    let pdpt_ptr = next_table(pml4e) as *const u64;
    let pdpte = *pdpt_ptr.add(pdpt_index(addr));
    if pdpte & ENTRY_PRESENT == 0 { return None; }

    // Level 2: Page Directory
    let pd_ptr = next_table(pdpte) as *const u64;
    let pde = *pd_ptr.add(pd_index(addr));
    if pde & ENTRY_PRESENT == 0 { return None; }

    // Check if this is a 2MiB Huge Page (Your driver uses these!)
    if pde & ENTRY_HUGE != 0 {
        let phys_base = pde & 0x000f_ffff_fffe_0000; // Mask for 2MiB alignment
        let offset = addr & 0x1F_FFFF;
        return Some(PhysAddr(phys_base | offset));
    }

    // Level 1: Page Table (Standard 4KiB page)
    // Note: Based on your map_2mib, you likely don't have level 1 tables,
    // but we include this for completeness.
    let pt_ptr = next_table(pde) as *const u64;
    let pt_idx = ((addr >> 12) & 0x1ff) as usize;
    let pte = *pt_ptr.add(pt_idx);
    if pte & ENTRY_PRESENT == 0 { return None; }

    let phys_base = pte & 0x000f_ffff_ffff_f000;
    let offset = addr & 0xFFF;
    Some(PhysAddr(phys_base | offset))
}

/// Unmap a 2MiB mapping and return the physical base if it existed.
/// Safety: paging must be active and page tables identity-mapped.
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn unmap_2mib(pml4: &mut PageTable, virt: VirtAddr) -> Option<PhysAddr> {
    let v = virt.0 as u64;
    let l4 = pml4.0;
    let l4e = l4.add(pml4_index(v));
    if *l4e & ENTRY_PRESENT == 0 { return None; }

    let l3 = next_table(*l4e);
    let l3e = l3.add(pdpt_index(v));
    if *l3e & ENTRY_PRESENT == 0 { return None; }

    let l2 = next_table(*l3e);
    let pde = l2.add(pd_index(v));
    if *pde & ENTRY_PRESENT == 0 { return None; }

    let pde_val = *pde;
    if pde_val & ENTRY_HUGE == 0 { return None; }

    let phys_base = (pde_val & 0x000f_ffff_fffe_0000) as usize;

    // clear PDE
    core::ptr::write_volatile(pde, 0u64);

    // Invalidate TLB
    core::arch::asm!("invlpg [{}]", in(reg) v, options(nostack, preserves_flags));

    Some(PhysAddr(phys_base as defs::PointerTy))
}

/// Unmap a 2MiB mapping and free its backing physical frame (if it existed)
pub unsafe fn unmap_and_free_2mib(pml4: &mut PageTable, virt: VirtAddr) -> Option<()> {
    if let Some(phys) = unsafe { unmap_2mib(pml4, virt) } {
        dealloc_2mib_frame(phys.as_inner() as usize);
        Some(())
    } else {
        None
    }
}
