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

use alloc::{boxed::Box, vec};
use x86_64::align_up;

/// Represents a stack.
///
/// note: stacks are not required to track their state (this is done by the [`Tracked`] wrapper)
pub trait Stack {
    /// base of the stack
    fn base(&self) -> *mut u8;
    /// stack size
    fn size(&self) -> usize;

    /// stack top
    ///
    /// base + size by default
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
    size: usize,
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
#[derive(Debug)]
pub enum PointerError {
    /// pointer was null
    Null,
    /// pointer was not aligned to 16
    Misaligned,
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

    /// creates a dynamic stack from a slice.
    ///
    /// # Safety
    /// it must outlive this stack
    pub const fn from_slice(slice: &[u8]) -> Self {
        Self {
            base: slice.as_ptr().cast_mut(),
            size: slice.len(),
        }
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
            return Err(PointerError::Misaligned);
        } else if ptr.is_null() {
            return Err(PointerError::Null);
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
        self.base as usize <= addr && addr < self.top() as usize
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
            panic!(
                "cannot write with size mismatch: {} != {}",
                self.size, other.size
            )
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

// static stack

// yes i wrote it with AI, theres nothing you can do to stop me >:)

/// A stack of a fixed, statically known size.
///
/// # Guarantees
/// - The stack memory is reserved at compile time.
/// - The size is exactly `SIZE` bytes for the lifetime of the stack.
/// - The stack cannot be resized or rebased.
/// - The stack grows downward.
/// - `base()` is the lowest address.
/// - `top()` is the initial stack pointer.
///
/// This type is intended for:
/// - kernel stacks
/// - interrupt stacks
/// - bootstrap / early-init stacks
///
/// It deliberately does **not** track the stack pointer.
/// Use a `Tracked<StaticStack<_>>` wrapper for that.
///
/// # Layout
/// ```text
/// base ─────────────────────── top
/// ^ lowest address             ^ initial RSP
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct StaticStack<const SIZE: usize> {
    // Invariant: `dynamic.size == SIZE`
    dynamic: DynStack,
}

impl<const SIZE: usize> StaticStack<SIZE> {
    // -------------------------------------------------------------------------
    // Creation
    // -------------------------------------------------------------------------

    /// Creates a static stack backed by a `'static` byte array.
    ///
    /// # Panics
    /// Panics if:
    /// - `SIZE == 0`
    /// - the array is not 16-byte aligned
    ///
    /// # Notes
    /// - The backing array must live for the entire lifetime of the kernel.
    /// - The stack pointer should be initialized to `top()`.
    pub fn from_array(array: &'static [u8; SIZE]) -> Self {
        if SIZE == 0 {
            panic!("StaticStack size must be non-zero");
        }

        let ptr = array.as_ptr() as usize;
        if (ptr & 0xF) != 0 {
            panic!("StaticStack base must be 16-byte aligned");
        }

        // Safety:
        // - pointer is non-null
        // - pointer is aligned
        // - memory is valid for reads/writes for entire program lifetime
        let dynamic = unsafe { DynStack::new_unchecked(array.as_ptr() as *mut u8, SIZE) };

        Self { dynamic }
    }

    /// Creates a static stack from a non-`'static` array **without lifetime checks**.
    ///
    /// # Safety
    /// - The backing array must outlive all uses of the stack.
    /// - The array must be properly aligned.
    /// - The memory must remain valid for reads and writes.
    pub const unsafe fn from_array_unchecked(array: &[u8; SIZE]) -> Self {
        let dynamic = unsafe { DynStack::new_unchecked(array.as_ptr() as *mut u8, SIZE) };

        Self { dynamic }
    }

    // -------------------------------------------------------------------------
    // Containment
    // -------------------------------------------------------------------------

    /// Returns whether the given address lies within this stack.
    ///
    /// The upper bound is exclusive.
    #[inline]
    pub fn contains(&self, addr: usize) -> bool {
        self.dynamic.contains(addr)
    }

    /// Returns whether the given pointer lies within this stack.
    #[inline]
    pub fn contains_ptr(&self, ptr: *const u8) -> bool {
        self.contains(ptr as usize)
    }

    // -------------------------------------------------------------------------
    // Views
    // -------------------------------------------------------------------------

    /// Returns the entire stack memory as a slice.
    ///
    /// # Safety
    /// Mutating stack memory while it is actively in use by the CPU
    /// may cause undefined behavior.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        self.dynamic.as_slice()
    }

    /// Returns the entire stack memory as a mutable slice.
    ///
    /// # Safety
    /// Mutating stack memory while it is actively in use by the CPU
    /// may cause undefined behavior.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.dynamic.as_mut_slice()
    }

    // -------------------------------------------------------------------------
    // Copying
    // -------------------------------------------------------------------------

    /// Copies this stack into another static stack of the **same size**.
    ///
    /// # Panics
    /// Panics if the sizes do not match.
    ///
    /// # Notes
    /// - The stacks must not overlap.
    /// - This does **not** adjust any stack pointers.
    pub fn write_to_exact<const OTHER: usize>(&self, other: &mut StaticStack<OTHER>) {
        if SIZE != OTHER {
            panic!("StaticStack size mismatch");
        }

        // Safety:
        // - sizes are equal
        // - both stacks are valid
        unsafe {
            self.dynamic
                .base()
                .copy_to_nonoverlapping(other.dynamic.base(), SIZE);
        }
    }

    /// Copies this stack into a raw pointer.
    ///
    /// # Safety
    /// - `dst` must be valid for writes of `SIZE` bytes.
    /// - The destination must not overlap this stack.
    pub unsafe fn write_to_ptr(&self, dst: *mut u8) {
        unsafe { self.dynamic.base().copy_to_nonoverlapping(dst, SIZE) };
    }

    // -------------------------------------------------------------------------
    // Internal access
    // -------------------------------------------------------------------------

    /// Exposes the underlying `DynStack`.
    #[inline]
    pub fn as_dyn(&self) -> &DynStack {
        &self.dynamic
    }
}

impl<const SIZE: usize> Stack for StaticStack<SIZE> {
    fn base(&self) -> *mut u8 {
        self.dynamic.base
    }
    fn size(&self) -> usize {
        SIZE
    }
}

/// creates static stacks
pub macro create_static_stacks($($vis:vis $name:ident: $count:expr),* $(,)?) {
    $(
        $vis static mut $name: $crate::stack::StaticStack<$count> = {
            static __RESERVATION: [u8; $count] = [0; $count];
            $crate::stack::StaticStack::from_array(&__RESERVATION)
        };
    )*
}

// allocated stack

/// A dynamically allocated stack backed by a heap allocation.
///
/// # Guarantees
/// - Memory is owned by this stack.
/// - Memory is contiguous and writable.
/// - The stack size is fixed for its lifetime.
/// - The stack grows downward.
/// - `base()` is the lowest usable address.
/// - `top()` is the initial stack pointer.
///
/// This type does **not** track the stack pointer.
pub struct AllocatedStack {
    backing: Box<[u8]>,
}

impl AllocatedStack {
    // -------------------------------------------------------------------------
    // Creation
    // -------------------------------------------------------------------------

    /// Allocates a new stack of the given size.
    ///
    /// # Panics
    /// Panics if:
    /// - `size == 0`
    /// - `size` is not 16-byte aligned
    ///
    /// # Notes
    /// - The stack pointer should be initialized to `top()`.
    pub fn new(size: usize) -> Self {
        if size == 0 {
            panic!("AllocatedStack size must be non-zero");
        }

        if (size & 0xF) != 0 {
            panic!("AllocatedStack size must be 16-byte aligned");
        }

        // Allocate zeroed memory to avoid leaking old data
        let backing = vec![0u8; size].into_boxed_slice();

        Self { backing }
    }

    /// Creates an allocated stack from a boxed slice without validation.
    ///
    /// # Safety
    /// - `backing` must not be resized or aliased.
    /// - The memory must remain writable.
    pub unsafe fn from_box_unchecked(backing: Box<[u8]>) -> Self {
        Self { backing }
    }

    // -------------------------------------------------------------------------
    // Containment
    // -------------------------------------------------------------------------

    /// Returns whether the given address lies within this stack.
    ///
    /// The upper bound is exclusive.
    #[inline]
    pub fn contains(&self, addr: usize) -> bool {
        let base = self.base() as usize;
        addr >= base && addr < base + self.size()
    }

    /// Returns whether the given pointer lies within this stack.
    #[inline]
    pub fn contains_ptr(&self, ptr: *const u8) -> bool {
        self.contains(ptr as usize)
    }

    // -------------------------------------------------------------------------
    // Views
    // -------------------------------------------------------------------------

    /// Returns the entire stack memory as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.backing
    }

    /// Returns the entire stack memory as a mutable slice.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.backing
    }

    // -------------------------------------------------------------------------
    // Copying
    // -------------------------------------------------------------------------

    /// Copies this stack into another allocated stack of equal size.
    ///
    /// # Panics
    /// Panics if the stack sizes do not match.
    pub fn write_to_exact(&self, other: &mut AllocatedStack) {
        if self.size() != other.size() {
            panic!("AllocatedStack size mismatch");
        }

        unsafe {
            self.base()
                .copy_to_nonoverlapping(other.base(), self.size());
        }
    }

    /// Copies this stack into a raw pointer.
    ///
    /// # Safety
    /// - `dst` must be valid for writes of `size()` bytes.
    pub unsafe fn write_to_ptr(&self, dst: *mut u8) {
        unsafe { self.base().copy_to_nonoverlapping(dst, self.size()) };
    }
}

impl Stack for AllocatedStack {
    fn base(&self) -> *mut u8 {
        self.backing.as_ptr() as *mut u8
    }
    fn size(&self) -> usize {
        self.backing.len()
    }
}

impl Debug for AllocatedStack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "<AllocatedStack {:p} -> {:p} ({} bytes)>",
            self.base(),
            self.top(),
            self.size()
        )
    }
}

// finally, tracking stacks

/// A stack paired with a tracked stack pointer.
///
/// # Guarantees
/// - `rsp` always lies within the stack.
/// - `rsp` is 16-byte aligned.
/// - The stack grows downward.
/// - The stack memory itself is not modified by this type.
///
/// This type does **not** perform stack switching by itself.
/// It is intended to be used by schedulers and context switch code.
pub struct Tracked<S: Stack> {
    stack: S,
    rsp: usize,
}

impl<S: Stack> Tracked<S> {
    // -------------------------------------------------------------------------
    // Creation
    // -------------------------------------------------------------------------

    /// Creates a new tracked stack with the stack pointer initialized to the top.
    ///
    /// # Panics
    /// Panics if:
    /// - the stack size is zero
    /// - the stack top is not 16-byte aligned
    pub fn new(stack: S) -> Self {
        let top = stack.top() as usize;

        if stack.size() == 0 {
            panic!("Tracked stack size must be non-zero");
        }

        if (top & 0xF) != 0 {
            panic!("Initial stack pointer must be 16-byte aligned");
        }

        Self { stack, rsp: top }
    }

    /// Creates a tracked stack with an explicit initial stack pointer.
    ///
    /// # Panics
    /// Panics if:
    /// - `rsp` is not within the stack
    /// - `rsp` is not 16-byte aligned
    pub fn with_rsp(stack: S, rsp: usize) -> Self {
        let base = stack.base() as usize;
        let top = stack.top() as usize;

        if rsp < base || rsp > top {
            panic!("Initial rsp is outside of stack bounds");
        }

        if (rsp & 0xF) != 0 {
            panic!("rsp must be 16-byte aligned");
        }

        Self { stack, rsp }
    }

    /// Creates a tracked stack without validation.
    ///
    /// # Safety
    /// - `rsp` must lie within `stack`
    /// - `rsp` must be 16-byte aligned
    pub unsafe fn new_unchecked(stack: S, rsp: usize) -> Self {
        Self { stack, rsp }
    }

    // -------------------------------------------------------------------------
    // Accessors
    // -------------------------------------------------------------------------

    /// Returns a reference to the underlying stack.
    #[inline]
    pub fn stack(&self) -> &S {
        &self.stack
    }

    /// Returns a mutable reference to the underlying stack.
    ///
    /// # Safety
    /// Mutating stack memory while it is active may cause undefined behavior.
    #[inline]
    pub fn stack_mut(&mut self) -> &mut S {
        &mut self.stack
    }

    /// Returns the current stack pointer.
    #[inline]
    pub fn rsp(&self) -> usize {
        self.rsp
    }

    /// Sets the stack pointer.
    ///
    /// # Panics
    /// Panics if:
    /// - `rsp` is outside the stack
    /// - `rsp` is not 16-byte aligned
    pub fn set_rsp(&mut self, rsp: usize) {
        let base = self.stack.base() as usize;
        let top = self.stack.top() as usize;

        if rsp < base || rsp > top {
            panic!("rsp outside of stack bounds");
        }

        if (rsp & 0xF) != 0 {
            panic!("rsp must be 16-byte aligned");
        }

        self.rsp = rsp;
    }

    /// Sets the stack pointer without validation.
    ///
    /// # Safety
    /// - `rsp` must lie within the stack
    /// - `rsp` must be 16-byte aligned
    pub unsafe fn set_rsp_unchecked(&mut self, rsp: usize) {
        self.rsp = rsp;
    }

    // -------------------------------------------------------------------------
    // Stack-relative helpers
    // -------------------------------------------------------------------------

    /// Returns how many bytes are currently used on the stack.
    ///
    /// This assumes a downward-growing stack.
    #[inline]
    pub fn used(&self) -> usize {
        self.stack.top() as usize - self.rsp
    }

    /// Returns how many bytes remain available on the stack.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.rsp - self.stack.base() as usize
    }

    /// Returns whether the stack pointer is within bounds.
    #[inline]
    pub fn rsp_in_bounds(&self) -> bool {
        let rsp = self.rsp;
        let base = self.stack.base() as usize;
        let top = self.stack.top() as usize;

        rsp >= base && rsp <= top
    }

    // -------------------------------------------------------------------------
    // Ownership utilities
    // -------------------------------------------------------------------------

    /// Consumes the tracked stack and returns the underlying stack.
    ///
    /// The stack pointer is discarded.
    pub fn into_stack(self) -> S {
        self.stack
    }

    /// Maps the underlying stack to another stack type.
    ///
    /// Useful for wrapping, e.g. `AllocatedStack -> DynStack`.
    pub fn map<T: Stack>(self, f: impl FnOnce(S) -> T) -> Tracked<T> {
        let rsp = self.rsp;
        let stack = f(self.stack);
        unsafe { Tracked::new_unchecked(stack, rsp) }
    }
}

// pushing and popping

use core::{mem, ptr};

impl<S: Stack> Tracked<S> {
    // ---------------------------------------------------------------------
    // Raw byte operations
    // ---------------------------------------------------------------------

    /// Pushes `size` bytes onto the stack.
    ///
    /// The stack pointer is moved downward by `size`.
    ///
    /// # Panics
    /// Panics if:
    /// - the stack would overflow
    /// - the resulting `rsp` is not aligned to 16 bytes
    pub fn push_bytes(&mut self, size: usize) -> *mut u8 {
        let new_rsp = self.rsp.wrapping_sub(size);

        let base = self.stack.base() as usize;

        if new_rsp < base {
            panic!("stack overflow");
        }

        if (new_rsp & 0xF) != 0 {
            panic!("rsp misaligned after push");
        }

        self.rsp = new_rsp;
        new_rsp as *mut u8
    }

    /// Pops `size` bytes from the stack.
    ///
    /// # Panics
    /// Panics if:
    /// - the stack would underflow
    pub fn pop_bytes(&mut self, size: usize) {
        let new_rsp = self.rsp.wrapping_add(size);
        let top = self.stack.top() as usize;

        if new_rsp > top {
            panic!("stack underflow");
        }

        self.rsp = new_rsp;
    }

    /// Pushes bytes without checks.
    ///
    /// # Safety
    /// - caller must ensure no overflow
    /// - resulting `rsp` must be valid
    pub unsafe fn push_bytes_unchecked(&mut self, size: usize) -> *mut u8 {
        self.rsp -= size;
        self.rsp as *mut u8
    }

    /// Pops bytes without checks.
    ///
    /// # Safety
    /// - caller must ensure no underflow
    pub unsafe fn pop_bytes_unchecked(&mut self, size: usize) {
        self.rsp += size;
    }
}

impl<S: Stack> Tracked<S> {
    // ---------------------------------------------------------------------
    // Typed operations
    // ---------------------------------------------------------------------

    /// Pushes a value onto the stack.
    ///
    /// # Panics
    /// Panics if:
    /// - the stack overflows
    /// - alignment requirements are violated
    pub fn push<T>(&mut self, value: T) {
        let size = mem::size_of::<T>();
        let align = mem::align_of::<T>();

        let new_rsp = (self.rsp - size) & !(align - 1);

        let base = self.stack.base() as usize;
        if new_rsp < base {
            panic!("stack overflow");
        }

        self.rsp = new_rsp;

        unsafe {
            ptr::write(self.rsp as *mut T, value);
        }
    }

    /// Pops a value from the stack.
    ///
    /// # Panics
    /// Panics if:
    /// - the stack underflows
    pub fn pop<T>(&mut self) -> T {
        let align = mem::align_of::<T>();
        let rsp = self.rsp;

        if (rsp & (align - 1)) != 0 {
            panic!("rsp misaligned for pop");
        }

        let value = unsafe { ptr::read(rsp as *const T) };

        let new_rsp = rsp + mem::size_of::<T>();
        let top = self.stack.top() as usize;

        if new_rsp > top {
            panic!("stack underflow");
        }

        self.rsp = new_rsp;
        value
    }

    /// Pushes a value without checks.
    ///
    /// # Safety
    /// - caller must ensure alignment
    /// - caller must ensure no overflow
    pub unsafe fn push_unchecked<T>(&mut self, value: T) {
        self.rsp -= mem::size_of::<T>();
        unsafe { ptr::write(self.rsp as *mut T, value) };
    }

    /// Pops a value without checks.
    ///
    /// # Safety
    /// - caller must ensure alignment
    /// - caller must ensure no underflow
    pub unsafe fn pop_unchecked<T>(&mut self) -> T {
        let value = unsafe { ptr::read(self.rsp as *const T) };
        self.rsp += mem::size_of::<T>();
        value
    }
}

impl<S: Stack + Debug> Debug for Tracked<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Tracked")
            .field("stack", &self.stack)
            .field("rsp", &format_args!("{:#x}", self.rsp))
            .finish()
    }
}

#[cfg(feature = "test")]
#[allow(missing_docs)]
pub mod tests {
    use crate::{
        stack::{DynStack, Stack, StaticStack, Tracked},
        test::TestInfo,
    };

    pub fn tracked_initial_rsp_is_top(_info: TestInfo) {
        #[repr(align(16))]
        struct S([u8; 4096]);

        static STACK: S = S([0; 4096]);

        let stack = StaticStack::<4096>::from_array(&STACK.0);
        let tracked = Tracked::new(stack);

        assert_eq!(tracked.rsp(), tracked.stack().top() as usize);
    }

    pub fn tracked_rejects_misaligned_rsp(mut info: TestInfo) {
        info.should_panic();

        static STACK: [u8; 4096] = [0; 4096];
        let stack = StaticStack::<4096>::from_array(&STACK);

        let bad_rsp = stack.top() as usize - 8;
        let _ = Tracked::with_rsp(stack, bad_rsp);
    }

    pub fn tracked_rejects_out_of_bounds_rsp(mut info: TestInfo) {
        info.should_panic();

        static STACK: [u8; 4096] = [0; 4096];
        let stack = StaticStack::<4096>::from_array(&STACK);

        let bad_rsp = stack.top() as usize + 16;
        let _ = Tracked::with_rsp(stack, bad_rsp);
    }

    pub fn tracked_used_remaining_correct(_info: TestInfo) {
        #[repr(align(16))]
        struct S([u8; 1024]);

        static STACK: S = S([0; 1024]);

        let stack = StaticStack::<1024>::from_array(&STACK.0);
        let mut tracked = Tracked::new(stack);

        let new_rsp = tracked.rsp() - 128;
        tracked.set_rsp(new_rsp);

        assert_eq!(tracked.used(), 128);
        assert_eq!(tracked.remaining(), 1024 - 128);
    }

    pub fn tracked_set_rsp_rejects_invalid(mut info: TestInfo) {
        info.should_panic();

        static STACK: [u8; 512] = [0; 512];
        let stack = StaticStack::<512>::from_array(&STACK);
        let mut tracked = Tracked::new(stack);

        let bad_rsp = tracked.stack().base() as usize - 16;
        tracked.set_rsp(bad_rsp);
    }

    pub fn tracked_unchecked_preserves_rsp(_info: TestInfo) {
        #[repr(align(16))]
        struct S([u8; 256]);

        static STACK: S = S([0; 256]);

        let stack = StaticStack::<256>::from_array(&STACK.0);

        let rsp = stack.base() as usize + 3; // intentionally invalid

        let tracked = unsafe { Tracked::new_unchecked(stack, rsp) };

        assert_eq!(tracked.rsp(), rsp);
    }

    pub fn tracked_map_preserves_rsp(_info: TestInfo) {
        #[repr(align(16))]
        struct S([u8; 1024]);

        static STACK: S = S([0; 1024]);

        let stack = StaticStack::<1024>::from_array(&STACK.0);
        let tracked = Tracked::new(stack);

        let rsp = tracked.rsp();

        let mapped = tracked.map(|s| DynStack::from_slice(s.as_slice()));

        assert_eq!(mapped.rsp(), rsp);
    }
}
