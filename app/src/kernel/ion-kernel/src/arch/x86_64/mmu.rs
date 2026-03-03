//! x86_64 MMU (2 MiB huge pages only)

use x86_64::registers::control::Cr3;
use crate::address_space::kernel_address_space;
use crate::arch::{PhysAddr, VirtAddr, defs};

/// represents the flag for a present entry
pub const ENTRY_PRESENT: u64 = 1 << 0;
/// represents the flag for a writable entry
pub const ENTRY_WRITABLE: u64 = 1 << 1;
/// represents the flag for a "huge" page (2 MiB)
pub const ENTRY_HUGE: u64 = 1 << 7;

/// Raw page-table pointer (identity-mapped)
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTable(*mut u64);

impl PageTable {
    /// Creates a new page table, panicking if it is not mapped
    pub fn new(ptr: VirtAddr) -> Self {
        let phys = unsafe { ptr.to_phys() };

        if phys.is_none() {
            panic!("cannot create a page table from an unmapped virtual address")
        }

        let phys = phys.unwrap();
        
        if phys.as_inner() != ptr.as_inner() {
            panic!("page table not identity mapped")
        }

        Self(ptr.as_ptr())
    }

    pub fn as_usize(self) -> usize {
        self.0.addr()
    }

    pub fn as_phys(self) -> PhysAddr {
        PhysAddr::new(self.0 as usize)
    }

    pub fn as_virt(self) -> VirtAddr {
        VirtAddr::new(self.0 as usize)
    }
}

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

#[inline]
fn pt_index(addr: u64) -> usize {
    ((addr >> 12) & 0x1ff) as usize
}

unsafe fn next_table(entry: u64) -> *mut u64 {
    (entry & 0x000f_ffff_ffff_f000) as *mut u64
}


bitflags::bitflags! {
    /// Flags for page tables
    #[derive(Debug, PartialEq, Clone, Copy)]
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
    log::debug!("attempting to map: {:?}", virt.as_ptr::<u8>());

    // temporary
    flags &= !PageTableFlags::NO_EXECUTE;

    let v = virt.as_inner() as u64;

    debug_assert!(v.is_multiple_of(2 * 1024 * 1024));
    debug_assert!(phys.as_inner().is_multiple_of(2 * 1024 * 1024));

    let l4 = pml4.0;
    let l4e = l4.add(pml4_index(v));

    if *l4e & ENTRY_PRESENT == 0 {
        let pdpt = alloc_zeroed_page_table();

        *l4e = (pdpt as u64)
            | ENTRY_PRESENT
            | ENTRY_WRITABLE
            | if flags.contains(PageTableFlags::USER_PAGE) { 1 << 2 } else { 0 };
    }

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

    assert!(*pde & ENTRY_PRESENT == 0, "PDE already mapped (virt: {:?})", virt.as_ptr::<u8>());

    let flags = PageTableFlags::PRESENT | PageTableFlags::HUGE_PAGE | flags;

    *pde = phys.as_ptr_ty() | flags.bits();

    // Invalidate TLB for this page
    core::arch::asm!("invlpg [{}]", in(reg) v, options(nostack, preserves_flags));
}

/// Unmap a 4 KiB page and return its physical address if it existed.
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn unmap_4k(
    pml4: &mut PageTable,
    virt: VirtAddr,
) -> Option<PhysAddr> {
    let v = virt.as_ptr_ty();

    let l4 = pml4.0;
    let l4e = l4.add(pml4_index(v));
    if *l4e & ENTRY_PRESENT == 0 { return None; }

    let l3 = next_table(*l4e);
    let l3e = l3.add(pdpt_index(v));
    if *l3e & ENTRY_PRESENT == 0 { return None; }

    let l2 = next_table(*l3e);
    let l2e = l2.add(pd_index(v));
    if *l2e & ENTRY_PRESENT == 0 { return None; }

    // must not be huge
    if *l2e & ENTRY_HUGE != 0 { return None; }

    let pt = next_table(*l2e);
    let pte = pt.add(pt_index(v));
    if *pte & ENTRY_PRESENT == 0 { return None; }

    let phys = (*pte & 0x000f_ffff_ffff_f000) as usize;

    core::ptr::write_volatile(pte, 0);
    core::arch::asm!("invlpg [{}]", in(reg) v, options(nostack, preserves_flags));

    Some(PhysAddr::new(phys))
}

/// Map one 4 KiB page.
///
/// # Safety
/// - `virt` and `phys` must be 4 KiB aligned
/// - Mapping must not already exist
/// - Paging must be active
/// - Page tables must be identity-mapped
#[allow(unsafe_op_in_unsafe_fn)]
#[track_caller]
pub unsafe fn map_4k(
    pml4: &mut PageTable,
    virt: VirtAddr,
    phys: PhysAddr,
    mut flags: PageTableFlags,
) {
    flags &= !PageTableFlags::NO_EXECUTE;

    let v = virt.as_inner() as u64;

    // PML4
    let l4 = pml4.0;
    let l4e = l4.add(pml4_index(v));

    if *l4e & ENTRY_PRESENT == 0 {
        let pdpt = alloc_zeroed_page_table();
        *l4e = (pdpt as u64)
            | ENTRY_PRESENT
            | ENTRY_WRITABLE
            | if flags.contains(PageTableFlags::USER_PAGE) { 1 << 2 } else { 0 };
    } else if flags.contains(PageTableFlags::USER_PAGE) {
        *l4e |= 1 << 2;
    }

    // PDPT
    let l3 = next_table(*l4e);
    let l3e = l3.add(pdpt_index(v));

    if *l3e & ENTRY_PRESENT == 0 {
        let pd = alloc_zeroed_page_table();
        *l3e = (pd as u64)
            | ENTRY_PRESENT
            | ENTRY_WRITABLE
            | if flags.contains(PageTableFlags::USER_PAGE) { 1 << 2 } else { 0 };
    } else if flags.contains(PageTableFlags::USER_PAGE) {
        *l3e |= 1 << 2;
    }

    // PD
    let l2 = next_table(*l3e);
    let l2e = l2.add(pd_index(v));

    assert!(*l2e & ENTRY_HUGE == 0, "cannot map 4KiB page: PDE is a huge page");

    if *l2e & ENTRY_PRESENT == 0 {
        let pt = alloc_zeroed_page_table();
        *l2e = (pt as u64)
            | ENTRY_PRESENT
            | ENTRY_WRITABLE
            | if flags.contains(PageTableFlags::USER_PAGE) { 1 << 2 } else { 0 };
    } else if flags.contains(PageTableFlags::USER_PAGE) {
        *l2e |= 1 << 2;
    }

    // PT
    let pt = next_table(*l2e);
    let pte = pt.add(pt_index(v));

    assert!(*pte & ENTRY_PRESENT == 0, "PTE already mapped");

    let flags = PageTableFlags::PRESENT | flags;
    *pte = phys.as_ptr_ty() | flags.bits();

    core::arch::asm!("invlpg [{}]", in(reg) v, options(nostack, preserves_flags));
}

use crate::arch::frame_allocator::{alloc_2mib_frame, dealloc_2mib_frame};
use crate::log;
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


use crate::lib_alloc::{THREAD_STACK_REGION, THREAD_STACK_SIZE};

/// initializes the kernel heap
/// # Safety
/// must only be called once
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn init_thread_heap() {
    let mut pml4 = active_pml4();

    let mut virt = THREAD_STACK_REGION;
    let end = THREAD_STACK_REGION + THREAD_STACK_SIZE;

    while virt < end {
        let phys = alloc_2mib_frame();

        map_2mib(
            &mut pml4,
            VirtAddr::new(virt),
            PhysAddr::new(phys),
            PageTableFlags::WRITABLE, // writable
        );

        virt += 2 * 1024 * 1024;
    }
}

/// # Safety
/// the virtual address must be correct, and the current paging config must be correct (i.e., allow for reading)
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn virt_to_phys(virt: VirtAddr) -> Option<PhysAddr> {
    let addr = virt.as_ptr_ty();
    
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

    // Check if this is a 2MiB Huge Page (disk driver uses these!)
    if pde & ENTRY_HUGE != 0 {
        let phys_base = pde & 0x000f_ffff_fffe_0000; // Mask for 2MiB alignment
        let offset = addr & 0x1F_FFFF;
        return Some(PhysAddr::new((phys_base | offset) as usize));
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
    Some(PhysAddr::new((phys_base | offset) as usize))
}

pub unsafe fn virt_to_phys_in(virt: VirtAddr, table: &mut PageTable) -> Option<PhysAddr> {
    let addr = virt.as_ptr_ty();
    
    // Level 4: PML4
    let pml4e = unsafe { (table.as_usize() as *const u64).add(pml4_index(addr)).read() };
    if pml4e & ENTRY_PRESENT == 0 { return None; }

    // Level 3: PDPT
    let pdpt_ptr = next_table(pml4e) as *const u64;
    let pdpte = unsafe { pdpt_ptr.add(pdpt_index(addr)).read() };
    if pdpte & ENTRY_PRESENT == 0 { return None; }

    // Level 2: Page Directory
    let pd_ptr = next_table(pdpte) as *const u64;
    let pde = unsafe { pd_ptr.add(pd_index(addr)).read() };
    if pde & ENTRY_PRESENT == 0 { return None; }

    // Check if this is a 2MiB Huge Page
    if pde & ENTRY_HUGE != 0 {
        let phys_base = pde & 0x000f_ffff_fffe_0000; // Mask for 2MiB alignment
        let offset = addr & 0x1F_FFFF;
        return Some(PhysAddr::new((phys_base | offset) as usize));
    }

    // Level 1: Page Table
    let pt_ptr = next_table(pde) as *const u64;
    let pt_idx = ((addr >> 12) & 0x1ff) as usize;
    let pte = unsafe { pt_ptr.add(pt_idx).read() };
    if pte & ENTRY_PRESENT == 0 { return None; }

    let phys_base = pte & 0x000f_ffff_ffff_f000;
    let offset = addr & 0xFFF;
    Some(PhysAddr::new((phys_base | offset) as usize))
}

/// Unmap a 2MiB mapping and return the physical base if it existed.
/// Safety: paging must be active and page tables identity-mapped.
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn unmap_2mib(pml4: &mut PageTable, virt: VirtAddr) -> Option<PhysAddr> {
    let v = virt.as_ptr_ty();
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

    Some(PhysAddr::new(phys_base))
}

/// Unmap a 2MiB mapping and free its backing physical frame (if it existed)
pub unsafe fn unmap_and_free_2mib(pml4: &mut PageTable, virt: VirtAddr) -> Option<()> {
    if let Some(phys) = unsafe { unmap_2mib(pml4, virt) } {
        dealloc_2mib_frame(phys.as_inner());
        Some(())
    } else {
        None
    }
}

pub fn copy_kernel_mappings_to(table: &mut PageTable) {
    let user_pml4 = table.as_usize() as *mut u64;
    let kernel_pml4 =
        unsafe { kernel_address_space().ptr().as_inner() as *mut u64 };

    // Copy ONLY the kernel half
    for i in 0..512 {
        let entry = unsafe { kernel_pml4.add(i).read() };
        if entry & ENTRY_PRESENT != 0 {
            unsafe { user_pml4.add(i).write(entry) };
        }
    }
}

pub fn zero_page_table(table: &mut PageTable) {
    let pml4 = table.as_usize() as *mut u64;

    for i in 0..512 {
        unsafe { pml4.add(i).write(0) };
    }
}

impl PageTable {
    pub fn query_flags(&self, addr: VirtAddr) -> Option<PageTableFlags> {
        let v = addr.as_ptr_ty();
    
        let pml4 = self.0.cast_const(); // identity-mapped *const u64
        let pml4e = unsafe { *pml4.add(pml4_index(v)) };
        if pml4e & ENTRY_PRESENT == 0 {
            return None;
        }
    
        let pdpt = unsafe { next_table(pml4e) };
        let pdpte = unsafe { *pdpt.add(pdpt_index(v)) };
        if pdpte & ENTRY_PRESENT == 0 {
            return None;
        }
    
        // 1 GiB pages (optional, but handle if you ever add them)
        if pdpte & ENTRY_HUGE != 0 {
            return Some(PageTableFlags::from_bits_truncate(pdpte));
        }
    
        let pd = unsafe { next_table(pdpte) };
        let pde = unsafe { *pd.add(pd_index(v)) };
        if pde & ENTRY_PRESENT == 0 {
            return None;
        }
    
        // 2 MiB page
        if pde & ENTRY_HUGE != 0 {
            return Some(PageTableFlags::from_bits_truncate(pde));
        }
    
        let pt = unsafe { next_table(pde) };
        let pte = unsafe { *pt.add(pt_index(v)) };
        if pte & ENTRY_PRESENT == 0 {
            return None;
        }
    
        Some(PageTableFlags::from_bits_truncate(pte))
    }
}

pub fn flags_of_in(page: &PageTable, virt: VirtAddr) -> Option<PageTableFlags> {
    page.query_flags(virt)
}

pub fn flags_of(virt: VirtAddr) -> Option<PageTableFlags> {
    let pml4 = unsafe { active_pml4() };
    pml4.query_flags(virt)
}

pub fn is_user_accessible_in(page: &PageTable, virt: VirtAddr) -> bool {
    flags_of_in(page, virt).is_some_and(|flags| flags.contains(PageTableFlags::USER_PAGE))
}

pub fn is_user_accessible(virt: VirtAddr) -> bool {
    let pml4 = unsafe { active_pml4() };
    is_user_accessible_in(&pml4, virt)
}