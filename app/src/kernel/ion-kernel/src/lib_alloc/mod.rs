//! Contains tools for allocation
//!
//! Typically, you don't use this module directly;
//! instead, use [`alloc`] types like `Box`, `Vec`, etc.

use core::{alloc::GlobalAlloc, fmt, ptr::NonNull};

use alloc::alloc::{AllocError, Allocator};
use linked_list_allocator::LockedHeap;

use crate::{c_lib::libc, thread};

unsafe extern "C" {
    static __kernel_end: u8;
}

const HEAP_ALIGN: usize = 2 * 1024 * 1024;

/// Size of the heap in bytes.
pub const HEAP_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

/// gets heap's start
#[inline]
pub fn heap_start() -> usize {
    const TEMP_HEAP_START: usize = 1024 * 1024 * 1024; // 1 GiB
    linked_list_allocator::align_up(TEMP_HEAP_START as *mut u8, HEAP_ALIGN) as usize
}

#[inline]
pub fn heap_end() -> usize {
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
        let res = thread::critical_moment(|| unsafe { self.0.alloc(layout) });

        if res.is_null() {
            libc::set_os_error(libc::OsError::AllocFail);
        }

        res
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        thread::critical_moment(|| {
            unsafe { self.0.dealloc(ptr, layout) };
        })
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: core::alloc::Layout) -> *mut u8 {
        let res = thread::critical_moment(|| unsafe { self.0.alloc_zeroed(layout) });

        if res.is_null() {
            libc::set_os_error(libc::OsError::AllocFail);
        }

        res
    }

    #[inline]
    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        layout: core::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        let res = thread::critical_moment(|| unsafe { self.0.realloc(ptr, layout, new_size) });

        if res.is_null() {
            libc::set_os_error(libc::OsError::AllocFail);
        }

        res
    }
}

unsafe impl Allocator for RescheduleAlloc {
    fn allocate(&self, layout: core::alloc::Layout) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        let alloc = unsafe { self.alloc(layout) };
        
        let ptr = NonNull::new(alloc).ok_or(AllocError)?;
        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: core::alloc::Layout) {
        unsafe { self.dealloc(ptr.as_ptr(), layout) };
    }
}

/// Global kernel allocator.
///
/// Used implicitly by `Box`, `Vec`, etc.
#[global_allocator]
pub static GLOBAL_ALLOC: RescheduleAlloc = RescheduleAlloc::empty();

#[cfg(feature = "test")]
pub mod tests;
pub mod user;