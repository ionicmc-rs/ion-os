//! Tools for creating, reading, and writing to stacks
//! 
//! This module contains different types of stacks, for different types of use-cases:
//! - [`DynStack`]: a stack (stored on the stack) with a dynamic size.
//!   - this has very niche use-cases, and most-likely does not refer to an actual stack.
//! - [`StaticStack`]: a statically reserved stack that has known size.
//! - [`AllocatedStack`]: a stack allocated on the heap.
//! 
//! None of these store the current stack pointer - that can be done via the [`Tracked`] wrapper.
//! 
//! Additionally, the [`Stack`] trait provides an abstraction to use in generics

use core::{fmt::Debug, slice};

use x86_64::align_up;

pub trait Stack {
    fn base(&self) -> *mut u8;
    fn size(&self) -> usize;

    fn top(&self) -> *mut u8 {
        self.base().wrapping_add(self.size())
    }
}

// Dyn Stack

/// A stack of a dynamic size - stored on the stack.
/// 
/// this is likely a virtual stack, as larger stacks would require more space, and likely allocation
/// 
/// if you do want allocation, see [`AllocatedStack`]
// No Clone - stacks should not be cloned.
#[derive(PartialEq, Eq)]
pub struct DynStack {
    base: *mut u8,
    size: usize
}

// trait impls

impl Stack for DynStack {
    fn base(&self) -> *mut u8 {
        self.base
    }

    fn size(&self) -> usize {
        self.size
    }
}

impl Debug for DynStack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "<DynStack {:p} -> {:p}>", self.base, self.top())
    }
}

/// Error for invalid pointers
pub enum PointerError {
    Null,
    Misaligned
}

impl DynStack {
    // creations
    
    /// Creates a new stack at `base`, of the given size.
    /// 
    /// # Safety
    /// base must be valid for reads and writes
    pub unsafe fn new(base: *mut u8, size: usize) -> Result<Self, PointerError> {
        if (base as usize) & !0xF != 0 {
            Err(PointerError::Misaligned)
        } else if base.is_null() {
            Err(PointerError::Null)
        } else {
            Ok(Self { base, size })
        }
    }

    /// creates a new stack at `base` of the given size, not checking for validity.
    /// 
    /// # Safety
    /// pointer must be non-null, aligned, and valid for reads and writes.
    pub const unsafe fn new_unchecked(base: *mut u8, size: usize) -> Self {
        Self { base, size }
    }

    pub const fn from_slice(slice: &[u8]) -> Self {
        Self { base: slice.as_ptr().cast_mut(), size: slice.len() }
    }

    // size changes

    /// shrinks the stack by the given count.
    /// 
    /// note: the number is rounded up to the nearest multiple of 16 for alignment.
    /// # Panics
    /// panics if `by` is greater than the stack size.
    #[track_caller]
    pub fn shrink(&mut self, by: usize) {
        if by > self.size {
            panic!("attempted to shrink stack size beyond limits.")
        }
        self.size -= by;
        self.size = align_up(self.size as u64, 16) as usize;
    }

    /// grows the stack by the given count.
    /// 
    /// note: the number is rounded up to the nearest multiple of 16 for alignment.
    /// # Panics
    /// panics if `by` overflows usize when added to the current size.
    pub fn grow(&mut self, by: usize) {
        self.size += by;
        self.size = align_up(self.size as u64, 16) as usize;
    }

    /// sets the stack to the given size.
    /// 
    /// note: the number is rounded up to the nearest multiple of 16 for alignment.
    pub fn size_to(&mut self, to: usize) {
        self.size = to;
        self.size = align_up(self.size as u64, 16) as usize;
    }

    /// rebases the stack to the given pointer.
    /// 
    /// # Safety
    /// the pointer is checked to be non-null and aligned well for pointers.
    /// but the pointer must still be valid for both reads, and writes.
    pub unsafe fn rebase(&mut self, ptr: *mut u8) -> Result<(), PointerError> {
        if !ptr.cast::<usize>().is_aligned() {
            return Err(PointerError::Misaligned)
        } else if ptr.is_null() {
            return Err(PointerError::Null)
        }

        self.base = ptr;

        Ok(())
    }

    /// rebases the stack to the given pointer, without checking for validity
    /// 
    /// # Safety
    /// the pointer is not checked to be non-null and aligned well for pointers.
    /// the pointer must be valid for both reads, and writes.
    pub unsafe fn rebase_unchecked(&mut self, ptr: *mut u8) {
        self.base = ptr;
    }

    // boolean ops

    /// returns wether the given address is a part of this stack.
    pub fn contains(&self, addr: usize) -> bool {
        self.base as usize <= addr 
        && addr < self.top() as usize
    }

    /// convenience function to avoid redundant conversion from `*<const/mut> T` to
    /// usize.
    /// 
    /// calls [`DynStack::contains`]
    pub fn contains_ptr(&self, ptr: *mut u8) -> bool {
        self.contains(ptr as usize)
    }

    // conversions

    /// returns this stack as a slice
    pub fn as_slice(&self) -> &[u8] {
        // Safety: this guaranteed valid by invariants
        unsafe { slice::from_raw_parts(self.base, self.size) }
    }

    /// returns this stack as a mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // Safety: this guaranteed valid by invariants
        unsafe { slice::from_raw_parts_mut(self.base, self.size) }
    }

    // writing

    /// Copies this stack to `other`
    /// 
    /// # Panics
    /// panics if the other stack is smaller than self.
    /// # See also
    /// [`DynStack::write_to_truncate`] -> truncates excess data
    /// [`DynStack::write_to_exact`] -> writes only if both stacks match
    pub fn write_to(&self, other: &mut DynStack) {
        if other.size < self.size {
            panic!("cannot write to a smaller stack.");
        }

        // Safety: stacks are valid by invariant.
        unsafe { self.base.copy_to(other.base, self.size) };
    }

    /// Copies this stack to `other`, panicking if the sizes are not the same
    /// 
    /// # Panics
    /// panics if the other stack is smaller than self.
    /// # See also
    /// [`DynStack::write_to`] -> panics if writing is not possible
    /// [`DynStack::write_to_truncate`] -> writes, removing excess bytes.
    pub fn write_to_exact(&self, other: &mut DynStack) {
        if self.size != other.size {
            panic!("cannot write with size mismatch: {} != {}", self.size, other.size)
        }
        // Safety: stacks are valid by invariant.
        unsafe { self.base.copy_to(other.base, other.size) };
    }

    /// Copies this stack to `other`, panicking if the sizes are not the same
    /// 
    /// # Panics
    /// panics if the other stack is smaller than self.
    /// # See also
    /// [`DynStack::write_to`] -> panics if writing is not possible
    /// [`DynStack::write_to_exact`] -> writes only if both stacks match
    pub fn write_to_truncate(&self, other: &mut DynStack) {
        // Safety: invariants ensure this is safe
        unsafe { self.base.copy_to(other.base, other.size) };
    }

    /// Copies this stack to `other`
    /// 
    /// # Safety
    /// the pointer must valid for writing.
    pub unsafe fn write_to_ptr(&self, other: *mut u8) {
        unsafe { self.base.copy_to(other, self.size) };
    }

    /// Copies from `other` to self.
    pub fn copy_from(&mut self, other: &DynStack) {
        other.write_to(self);
    }

    /// Copies from `other` to self, truncating excess bytes.
    pub fn copy_from_truncate(&mut self, other: &DynStack) {
        other.write_to_truncate(self);
    }

    /// Copies from `other` to self, panicking if their sizes don't match
    pub fn copy_from_exact(&mut self, other: &DynStack) {
        other.write_to_exact(self);
    }

    /// Copies from `other` to self
    /// 
    /// # Safety
    /// the pointer must valid for reading.
    pub unsafe fn copy_from_ptr(&mut self, other: *const u8) {
        unsafe { other.copy_to(self.base, self.size) };
    }
}