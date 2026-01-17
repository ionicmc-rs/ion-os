//! Contains tools for allocation
//!
//! Typically, you don't use this module directly;
//! instead, use [`alloc`] types like `Box`, `Vec`, etc.

use core::{alloc::GlobalAlloc, fmt};

use linked_list_allocator::LockedHeap;

use crate::thread;

/// Virtual start of the heap.
///
/// This address must be mapped by the architecture MMU code
/// before calling [`init_heap`].
pub const HEAP_START: usize = 0x_4444_4444_0000;

/// Size of the heap in bytes.
pub const HEAP_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

#[inline]
fn heap_start() -> usize {
    linked_list_allocator::align_up(HEAP_START as *mut u8, 16) as usize
}

#[inline]
fn heap_end() -> usize {
    heap_start() + HEAP_SIZE
}

/// Returns whether `addr` lies inside the heap.
#[inline]
pub fn is_heap_addr(addr: usize) -> bool {
    (heap_start()..heap_end()).contains(&addr)
}

/// Initialize the heap allocator.
///
/// # Safety
/// - The virtual memory range `[heap_start, heap_start + HEAP_SIZE)`
///   must be mapped, writable, and exclusive to the heap.
/// - Must be called exactly once.
pub unsafe fn init_heap() {
    unsafe { GLOBAL_ALLOC
        .0
        .lock()
        .init(heap_start() as *mut u8, HEAP_SIZE) };
}

/// A global allocator that cooperates with the scheduler.
pub struct RescheduleAlloc(LockedHeap);

impl fmt::Debug for RescheduleAlloc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RescheduleAlloc")
            .finish_non_exhaustive()
    }
}

impl RescheduleAlloc {
    /// Create an empty allocator.
    pub const fn empty() -> Self {
        Self(LockedHeap::empty())
    }
}

unsafe impl GlobalAlloc for RescheduleAlloc {
    #[inline]
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        thread::critical_moment(|| unsafe { self.0.alloc(layout) })
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        thread::critical_moment(|| {
            unsafe { self.0.dealloc(ptr, layout) };
        })
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: core::alloc::Layout) -> *mut u8 {
        thread::critical_moment(|| unsafe { self.0.alloc_zeroed(layout) })
    }

    #[inline]
    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        layout: core::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        thread::critical_moment(|| unsafe { self.0.realloc(ptr, layout, new_size) })
    }
}

/// Global kernel allocator.
///
/// Used implicitly by `Box`, `Vec`, etc.
#[global_allocator]
pub static GLOBAL_ALLOC: RescheduleAlloc = RescheduleAlloc::empty();

#[cfg(feature = "test")]
pub mod tests;
