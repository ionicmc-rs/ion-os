//! Contains tools for Allocation
//! 
//! Typically, you dont use this module, you use [`alloc`]
use core::{alloc::GlobalAlloc, fmt};

use linked_list_allocator::LockedHeap;
// Heap Defs.

/// The Beginning of the Heap.
pub const HEAP_START: usize = 0x_4444_4444_0000;

/// The Heap's Size
pub const HEAP_SIZE: usize = 1500 * 1024; // 1 MiB by default.

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

use crate::thread;

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
        GLOBAL_ALLOC.0.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }

    Ok(())
}

/// An allocator that reschedules if needed
pub struct RescheduleAlloc(LockedHeap);

impl fmt::Debug for RescheduleAlloc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f
            .debug_struct("RescheduleAlloc")
            .finish_non_exhaustive()
    }
}

impl RescheduleAlloc {
    /// Creates an empty heap
    pub const fn empty() -> Self {
        RescheduleAlloc(LockedHeap::empty())
    }
}

unsafe impl GlobalAlloc for RescheduleAlloc {
    #[inline]
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        thread::critical_moment(
            || { 
                unsafe { self.0.alloc(layout) } 
            }
        ) // reschedules after
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        thread::critical_moment(|| {
            unsafe { self.0.dealloc(ptr, layout); }
        })
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: core::alloc::Layout) -> *mut u8 {
        thread::critical_moment(|| {
            unsafe { self.0.alloc_zeroed(layout) }
        })
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: core::alloc::Layout, new_size: usize) -> *mut u8 {
        thread::critical_moment(|| {
            unsafe {
                self.0.realloc(ptr, layout, new_size)    
            }
        })
    }
}

// Global Allocator

#[global_allocator]
/// This static the global allocator.
/// 
/// This should be used through [`Box`](alloc::boxed::Box), and other alloc types.
pub static GLOBAL_ALLOC: RescheduleAlloc = RescheduleAlloc::empty();

#[cfg(feature = "test")]
/// Tests
pub mod tests;