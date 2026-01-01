use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

/// Dummy Allocator
/// 
/// Temp.
pub struct Dummy;

// Global Allocator

unsafe impl GlobalAlloc for Dummy {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        panic!("dealloc should be never called")
    }
}

#[global_allocator]
/// This static the global allocator.
/// 
/// This should be used through [`Box`](alloc::boxed::Box), and other alloc types.
static GLOBAL_ALLOC: Dummy = Dummy;