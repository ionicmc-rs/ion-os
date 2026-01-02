//! Contains tools for Allocation
//! 
//! Typically, you dont use this module, you use [`alloc`]
use buddy_system_allocator::LockedHeap;

// Heap Defs.

/// The Beginning of the Heap.
pub const HEAP_START: usize = 0x_4444_4444_0000;

/// The Heap's Size
pub const HEAP_SIZE: usize = 100 * 1024; // 100 KiB

/// The Heap's End.
/// 
/// Equals [`HEAP_START`] + [`HEAP_SIZE`].
pub const HEAP_END: usize = HEAP_START + HEAP_SIZE;

/// Returns wether addr is inside of the heap.
pub const fn is_heap_addr(addr: usize) -> bool {
    (HEAP_START..=HEAP_END).contains(&addr)
}

use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

/// Initialize the Heap.
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush()
        };
    }

    unsafe {
        GLOBAL_ALLOC.lock().init(HEAP_START, HEAP_SIZE);
    }

    Ok(())
}

// Global Allocator

#[global_allocator]
/// This static the global allocator.
/// 
/// This should be used through [`Box`](alloc::boxed::Box), and other alloc types.
pub static GLOBAL_ALLOC: LockedHeap<32> = LockedHeap::empty();

#[cfg(feature = "test")]
/// Tests
pub mod tests;