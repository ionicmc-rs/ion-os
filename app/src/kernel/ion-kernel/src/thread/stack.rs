use core::{alloc::Layout, ptr::NonNull};

use alloc::alloc::{Allocator, handle_alloc_error};

use crate::{c_lib::libc, lib_alloc::is_heap_addr};

/// A stack meant for a thread.
///
/// Currently, these threads are unguarded, this is not planned for user threads.
///
/// however, this is not a stable decision - and is subject to change (in the near future!).
/// # [`Clone`]ing.
/// This struct implements [`Clone`]; and you may think cloning a stack is inefficient - and you're right.
///
/// However, this struct only holds a pointer to the stack allocated on the heap, making it safe to [`Clone`]
///
/// # Examples
/// no examples are provided - use the [`thread api`](super) for threads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadStack<A: Allocator> {
    base: *mut u8,
    alloc: A,
    size: usize,
    #[cfg(debug_assertions)]
    should_drop: bool,
}

impl<A: Allocator> ThreadStack<A> {
    pub fn of_size(size: usize, alloc: A) -> ThreadStack<A> {
        let layout = Layout::from_size_align(size, 16)
            .expect("Invalid thread stack size - must be usable with an alignment of 16");

        let allocated = alloc.allocate_zeroed(layout).unwrap_or_else(|_| handle_alloc_error(layout)).as_ptr();

        ThreadStack {
            base: allocated as *mut u8,
            size,
            alloc,
            #[cfg(debug_assertions)]
            should_drop: true,
        }
    }
    #[cfg(debug_assertions)]
    pub fn should_not_drop(&mut self) {
        self.should_drop = false;
    }

    pub const fn size(&self) -> usize {
        self.size
    }

    pub const fn ptr(&self) -> *mut u8 {
        self.base
    }

    pub const ALIGNMENT: usize = 16;
}

impl<A: Allocator> Drop for ThreadStack<A> {
    fn drop(&mut self) {
        unsafe {
            // Safety: if the layout fails here, that means this `ThreadStack` was created illegally.
            let layout = Layout::from_size_align(self.size, 16).unwrap_unchecked();
            #[cfg(debug_assertions)]
            if !self.should_drop {
                panic!(
                    "Thread stack that was not supposed to drop reached drop implementation\n\nThis indicates a bug with the scheduler - these checks are optional and should not be relied on."
                );
            }
            // Safety: if this fails here, that means this `ThreadStack` was created with an allocation error, which should not happen
            self.alloc.deallocate(NonNull::new(self.base).unwrap(), layout);
        };
    }
}
