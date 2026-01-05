use core::alloc::Layout;

use alloc::alloc::{alloc_zeroed, dealloc, handle_alloc_error};

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
pub struct ThreadStack {
    base: *mut u8,
    size: usize
}

impl ThreadStack {
    pub fn of_size(size: usize) -> ThreadStack {
        let layout = Layout::from_size_align(size, 16).expect("Invalid thread stack size - must be usable with an alignment of 16");

        let alloc = unsafe { 
            alloc_zeroed(layout)
        };

        if alloc.is_null() || !is_heap_addr(alloc as usize) {
            libc::set_errno(5); // alloc failure
            handle_alloc_error(layout);
        }

        ThreadStack {
            base: alloc,
            size
        }
    }

    pub const fn size(&self) -> usize {
        self.size
    }

    pub const fn ptr(&self) -> *mut u8 {
        self.base
    }

    pub const ALIGNMENT: usize = 16;
}

impl Drop for ThreadStack {
    fn drop(&mut self) {
        unsafe { 
            // Safety: if the layout fails here, that means this `ThreadStack` was created illegally.
            let layout = Layout::from_size_align(self.size, 16).unwrap_unchecked();
            
            // Safety: if this fails here, that means this `ThreadStack` was created with an allocation error, which should not happen
            dealloc(self.base, layout);
        };
    }
}