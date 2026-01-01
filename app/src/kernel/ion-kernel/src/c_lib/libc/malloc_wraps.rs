//! Type wrappers for the [`malloc`] allocator

use core::{ptr::NonNull};

use alloc::{alloc::{AllocError, Allocator}, boxed::Box, collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque}, rc::Rc, sync::Arc, vec::Vec};

use crate::c_lib::libc::mem::{free, malloc};


/// Allocator which uses malloc.
/// 
/// This implementation is unsafe and unrecommended, and eventually maps to the global allocator, so
/// it is recommended you use that instead.
#[derive(Debug, Clone, Copy)]
pub struct MallocAllocator;

unsafe impl Allocator for MallocAllocator {
    fn allocate(&self, layout: core::alloc::Layout) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        let ptr = unsafe { malloc(layout.size()) };
        let nn = NonNull::new(ptr).ok_or(AllocError)?;
        // Construct slice pointer with correct length
        Ok(NonNull::slice_from_raw_parts(nn, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: core::alloc::Layout) {
        let size: usize = ptr.sub(size_of::<usize>()).cast().read();
        if size != layout.size() {
            super::error::set_errno(5);
            panic!("Invalid Layout: {layout:?} - does not match stored metadata.");
        }
        // Safety: Caller ensures pointer comes from this allocator, which always comes from
        // malloc
        free(ptr.as_ptr());
    }
}


macro malloc_tool(
    $( $T:ident < $( $Generic:ident$(: $Bound:ident)? ),* > = $Eq:ident ),*
) {
    $(
        #[doc = concat!("A `", stringify!($Eq), "` Which uses [`malloc`]")]
        pub type $T<$( $Generic ),*>
        = $Eq<$( $Generic ),*, MallocAllocator>
        where
            $( $Generic: ?Sized $(+ $Bound)? ),*
        ;
    )*
}

malloc_tool!(
    MallocBox<T> = Box,
    MallocRc<T> = Rc,
    MallocLinkedList<T: Sized> = LinkedList,
    MallocVecDeque<T: Sized> = VecDeque,
    MallocBtreeMap<K: Sized, V: Sized> = BTreeMap,
    MallocBtreeSet<T: Sized> = BTreeSet,
    MallocBinaryHeap<T: Sized> = BinaryHeap,
    MallocArc<T> = Arc,
    MallocVec<T: Sized> = Vec
);
