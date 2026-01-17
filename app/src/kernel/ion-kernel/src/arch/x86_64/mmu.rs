//! x86_64 MMU / paging

use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable as XOffsetPageTable, Page, PageTable as XPTable, PageTableFlags, PhysFrame, Size4KiB, Translate
};

use x86_64::VirtAddr as XVirtAddr;

use super::super::VirtAddr;
use crate::arch::frame_allocator::BootInfoFrameAllocator;
use crate::c_lib::PHYSICAL_MEMORY_OFFSET;

/// Opaque x86_64 page table
#[repr(transparent)]
#[derive(Debug)]
pub struct PageTable(XPTable);

/// Opaque x86_64 offset page table
#[derive(Debug)]
pub struct OffsetPageTable<'a>(XOffsetPageTable<'a>);

impl PageTable {
    /// Get a mutable reference to the inner x86_64 page table.
    ///
    /// # Safety
    /// Caller must uphold paging invariants.
    #[inline]
    #[allow(unused)]
    pub(crate) unsafe fn inner_mut(&mut self) -> &mut XPTable {
        &mut self.0
    }
}

impl<'a> OffsetPageTable<'a> {
    /// Translate a virtual address to a physical address.
    #[inline]
    pub fn translate_addr(&self, addr: VirtAddr) -> Option<u64> {
        self.0
            .translate_addr(XVirtAddr::new(addr.0))
            .map(|p| p.as_u64())
    }

    /// Get the physical memory offset used by this page table.
    #[inline]
    pub fn phys_offset(&self) -> VirtAddr {
        VirtAddr(PHYSICAL_MEMORY_OFFSET as u64)
    }
}

/// Returns a mutable reference to the active level 4 page table.
///
/// # Safety
/// - Must be called only once
/// - Caller must ensure no aliasing `&mut` references
pub unsafe fn active_level_4_table() -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let phys_offset = VirtAddr(PHYSICAL_MEMORY_OFFSET as u64);
    let (frame, _) = Cr3::read();

    let virt = phys_offset.0 + frame.start_address().as_u64();
    unsafe { &mut *(virt as *mut PageTable) }
}

/// Initialize an offset page table using the active level 4 table.
///
/// # Safety
/// - Must be called only once
/// - Paging structures must already be mapped
pub unsafe fn init() -> OffsetPageTable<'static> {
    let l4 = unsafe { active_level_4_table() };

    OffsetPageTable(
        unsafe { XOffsetPageTable::new(
            &mut l4.0,
            XVirtAddr::new(PHYSICAL_MEMORY_OFFSET as u64),
        ) }
    )
}

// heap init

use x86_64::PhysAddr as XPhysAddr;

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let addr = self.alloc_frame()?;
        Some(PhysFrame::containing_address(XPhysAddr::new(addr as u64)))
    }
}

/// Map the kernel heap virtual memory.
///
/// # Safety
/// - Must be called once
/// - Paging must be active
/// - Heap region must be unmapped
pub unsafe fn map_kernel_heap(
    mapper: &mut OffsetPageTable<'static>,
    frame_alloc: &mut BootInfoFrameAllocator,
    heap_start: usize,
    heap_size: usize,
) {
    let start = XVirtAddr::new(heap_start as u64);
    let end = XVirtAddr::new((heap_start + heap_size - 1) as u64);

    let start_page = Page::<Size4KiB>::containing_address(start);
    let end_page = Page::<Size4KiB>::containing_address(end);

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    for page in Page::range_inclusive(start_page, end_page) {
        let frame = frame_alloc
            .alloc_frame()
            .expect("out of physical memory");

        let phys = x86_64::PhysAddr::new(frame as u64);

        unsafe { mapper
            .0
            .map_to(
                page,
                PhysFrame::containing_address(phys),
                flags,
                frame_alloc,
            )
            .expect("map_to failed")
            .flush() };
    }
}
