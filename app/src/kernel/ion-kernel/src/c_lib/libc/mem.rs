//! C-style Memory Operations
//! 
//! everything provided by this module can be done elsewhere with Rust APIs, but this module provides
//! the basic form
//! 
//! note: we do not use exactly the same API as C, we use a safer api to prevent memory leaks, and incorrect allocations

use core::{alloc::Layout, mem::size_of_val_raw, ptr};

use alloc::alloc::{alloc, dealloc};

#[cfg(feature = "test")]
use crate::test::{TestInfo, TestResult};

// SAFE API

/// Allocates memory suitable for holding `T`
/// 
/// despite the function's name, this function is marked unsafe
/// # Safety
/// The caller must ensure `T` is not zero sized, and the the call follows the Safety requirements of
/// [`buddy_system_allocator::LockedHeap`]
/// 
/// Additionally, the allocation must be deallocated, before the halt loop at the end of our kernel,
/// using [`free_safe`]
/// # `malloc_safe` for unsized values.
/// for unsized values, use [`malloc_unsized_safe_val`], as not having value is unsafe (use [`malloc`] or [`Box`](alloc::boxed) instead)
pub unsafe fn malloc_safe<T>() -> *mut T {
    unsafe { alloc(Layout::new::<T>()).cast() }
}

/// Allocates using malloc, and writes the argument into it.
/// 
/// despite the function's name, this function is marked unsafe
/// # Safety
/// See [`malloc_safe`]
pub unsafe fn malloc_safe_val<T>(val: T) -> *mut T {
    let ptr: *mut T = unsafe { alloc(Layout::new::<T>()).cast() };
    unsafe { ptr.write(val) };
    ptr
}

/// Allocates memory suitable for holding `val`
/// 
/// **THIS DOES NOT PLACE `val` INTO THE ALLOCATION.**
/// 
/// despite the function's name, this function is marked unsafe
/// # Safety
/// See [`malloc_safe`]
/// # Return
/// This returns an allocation suitable ONLY to hold the argument (or identical values)
pub unsafe fn malloc_unsized_safe_val<T: ?Sized>(val: &T) -> *mut u8 {
    // Compute layout for the value
    let layout = Layout::for_value(val);

    // Allocate raw memory
    let raw = unsafe { malloc(layout.size()) };
    if raw.is_null() {
        super::error::set_errno(5);
        alloc::alloc::handle_alloc_error(layout);
    }

    raw
}



/// Deallocates Memory pointing to `T`, with the assumption it was allocated using [`malloc_safe`] 
/// (or at least the GlobalAllocator, safely).
/// 
/// despite the function's name, this function is marked unsafe.
/// # Safety
/// The caller must ensure `ptr` was allocated properly, and the the call follows the Safety requirements of
/// [`buddy_system_allocator::LockedHeap`]
pub unsafe fn free_safe<T: ?Sized>(ptr: *mut T) {
    if ptr.is_null() {
        super::error::set_errno(5);
        return;
    }
    unsafe { dealloc(ptr.cast(), Layout::for_value_raw(ptr)) };
}

/// Reallocates Memory in `ptr` suitable for holding values of type `T`
/// 
/// # Safety
/// The caller must ensure:
/// - The pointer was already a validly allocated pointer to `T`, using the Global Allocator.
/// - T is not too big of a value.
/// # Why this returns `*mut u8`
/// If your type unsized, this function cant cast that ptr, so you get a `*mut u8`
pub unsafe fn realloc_safe<T: ?Sized>(ptr: *mut T) -> *mut u8 {
    let layout = unsafe { Layout::for_value_raw(ptr) };
    let res = unsafe { alloc::alloc::realloc(ptr.cast(), layout, size_of_val_raw(ptr)) };
    if res.is_null() {
        super::error::set_errno(5);
        alloc::alloc::handle_alloc_error(layout);
    }
    res
}

/// Allocs a slice of size `nmemb`, suitable to hold `[T]`
/// 
/// # Safety
/// The caller must ensure:
/// - `T` is not zero-sized.
/// - The call follows the Safety requirements of [`buddy_system_allocator::LockedHeap`].
/// - The allocation must be deallocated using [`free_safe`] before the halt loop at the end of the kernel.
pub unsafe fn calloc_safe<T>(nmemb: usize) -> *mut T {
    if nmemb == 0 {
        return ptr::null_mut();
    }

    // Compute total size
    let total_size = nmemb.checked_mul(size_of::<T>())
        .unwrap_or_else(|| {
            super::error::set_errno(5);
            panic!("calloc_safe: size too big")
        });

    // Create layout for the array
    let layout = Layout::from_size_align(total_size, core::mem::align_of::<T>())
        .unwrap_or_else(|e| {
            super::error::set_errno(5);
            panic!("calloc_safe: invalid layout: {e}");
        });

    // Allocate
    let raw = unsafe { alloc(layout) };
    if raw.is_null() {
        super::error::set_errno(5);
        return ptr::null_mut();
    }

    // Zero-initialize the memory
    unsafe { ptr::write_bytes(raw, 0, total_size) };

    raw.cast()
}

// RAW API

/// Allocates memory with a size of `size`
/// 
/// This function is extremely unsafe, and if you know the type of the allocation, [`malloc_safe`] is
/// preferred. 
/// 
/// Even better, use [`Box`](alloc::boxed) or [`Vec`](alloc::vec) to ensure the value is always dropped
/// # Safety
/// The following must be ensured:
/// - `size`, when rounded up to the nearest multiple of align, must not overflow isize (i.e., the 
///   rounded value must be less than or equal to isize::MAX).
/// - The allocation must be freed eventually.
/// - the the call follows the Safety requirements of
///   [`buddy_system_allocator::LockedHeap`]
/// # Allocation
/// This will allocate `size + size_of::<usize> (8)` bytes, due to size metadata at the begining of the allocation
/// for [`free`] to actually free the allocation.
#[unsafe(no_mangle)] // This allows it to be used elsewhere in implicit compiler calls
pub unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    if size == 0 {
        super::error::set_errno(5);
        return core::ptr::null_mut();
    }

    // Reserve extra space to store size
    let layout = Layout::from_size_align(size + core::mem::size_of::<usize>(), core::mem::align_of::<usize>())
        .unwrap();

    let raw = unsafe { alloc(layout) };
    if raw.is_null() {
        super::error::set_errno(5);
        return core::ptr::null_mut();
    }

    // Store size at the beginning
    unsafe { *(raw as *mut usize) = size };

    // Return pointer after metadata
    unsafe { raw.add(core::mem::size_of::<usize>()) }
}

/// Frees the Allocation.
/// 
/// # Safety
/// The caller must ensure this pointer was allocated using [`malloc`], and the the call follows the Safety requirements of
/// [`buddy_system_allocator::LockedHeap`]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut u8) {
    if ptr.is_null() {
        super::error::set_errno(5);
        return;
    }

    // Step back to metadata
    let meta_ptr = unsafe { ptr.sub(core::mem::size_of::<usize>()) };
    let size = unsafe { *(meta_ptr as *mut usize) };

    let layout = Layout::from_size_align(size + core::mem::size_of::<usize>(), core::mem::align_of::<usize>())
        .unwrap();

    unsafe { dealloc(meta_ptr, layout) };
}

/// Allocates `nmemb` elements, of size `size`, all zeroed.
/// 
/// This is useful for slices.
/// # Safety
/// The caller must ensure:
/// - nmemb * size does not overflow isize
/// - the pointer is freed before the kernel's end.
/// - the the call follows the Safety requirements of
///   [`buddy_system_allocator::LockedHeap`]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut u8 {
    let total = nmemb.checked_mul(size).unwrap_or(0);
    if total == 0 {
        super::error::set_errno(5);
        return ptr::null_mut();
    }

    // Reserve space for metadata
    let layout = Layout::from_size_align(total + core::mem::size_of::<usize>(), core::mem::align_of::<usize>())
        .unwrap();

    let raw = unsafe { alloc(layout) };
    if raw.is_null() {
        super::error::set_errno(5);
        return ptr::null_mut();
    }

    // Store size
    unsafe { *(raw as *mut usize) = total };

    // Zero initialize
    let user_ptr = unsafe { raw.add(core::mem::size_of::<usize>()) };
    unsafe { ptr::write_bytes(user_ptr, 0, total) };

    user_ptr
}

/// Reallocates the pointer with `new_size`
/// # Safety
/// - [`malloc`]\(`new_size`) must be a valid call
///
/// the pointer must point to valid data if it is not null
/// # Alloc Sizes
/// see [`malloc`] for more info.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut u8, new_size: usize) -> *mut u8 {
    if ptr.is_null() {
        return unsafe { malloc(new_size) };
    }
    if new_size == 0 {
        super::error::set_errno(5);
        unsafe { free(ptr) };
        return ptr::null_mut();
    }

    // Recover old size
    let meta_ptr = unsafe { ptr.sub(core::mem::size_of::<usize>()) };
    let old_size = unsafe { *(meta_ptr as *mut usize) };

    // Allocate new block
    let new_ptr = unsafe { malloc(new_size) };
    if new_ptr.is_null() {
        super::error::set_errno(5);
        return ptr::null_mut();
    }

    // Copy min(old_size, new_size)
    unsafe { ptr::copy_nonoverlapping(ptr, new_ptr, core::cmp::min(old_size, new_size)) };

    // Free old block
    unsafe { free(ptr) };

    new_ptr
}

/// Test malloc and related functions
#[cfg(feature = "test")]
pub fn test_malloc(inf: TestInfo) -> TestResult {
    // safe api.

    // variable data for better testing.

    use crate::{c_lib::libc::malloc_wraps::{MallocAllocator, MallocBox}, test::test_assert_eq};
    unsafe { 
        let safe_ptr = malloc_safe_val(inf.ord);
        test_assert_eq!(*safe_ptr, inf.ord)?;
        free_safe(safe_ptr); 
    };

    // raw api

    unsafe {
        let alloc = malloc(1);
        alloc.write(42);
        test_assert_eq!(*alloc, 42)?;
        // this fails on invalid allocation.
        free(alloc);
    }

    // malloc wrapper

    let boxed = MallocBox::new_in(42, MallocAllocator);
    test_assert_eq!(*boxed, 42)?;

    // realloc test

    unsafe {
        let alloc = malloc(1);
        alloc.write(1);
        let re: *mut u16 = realloc(alloc, 2).cast();
        re.write(2);
        test_assert_eq!(*re, 2)?;
        free(re.cast());
    }

    // uninit safe

    unsafe {
        let alloc = malloc_safe::<u16>();
        let new: *mut u16 = realloc_safe(alloc).cast();
        new.write(42);
        test_assert_eq!(*new, 42)?;
        free_safe(new);
    }

    // calloc, final 2

    unsafe {
        let slice_alloc = calloc(3, 1);
        let slice = core::slice::from_raw_parts(slice_alloc, 3);
        test_assert_eq!(slice, &[0, 0, 0])?;
        free(slice_alloc);
    }

    unsafe {
        let slice_safe = calloc_safe::<u16>(3);
        let slice = core::slice::from_raw_parts(slice_safe, 3);
        test_assert_eq!(slice, &[0, 0, 0]);
        free_safe(slice_safe);
    }

    TestResult::Ok
} 