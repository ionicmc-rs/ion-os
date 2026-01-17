/// Represents the width of a pointer
/// 
/// is 16, 32 or 64
#[derive(Debug, Clone, Copy)]
pub enum PointerWidth {
    /// 16
    Sixteen,
    /// 32
    ThirtyTwo,
    /// 64
    SixtyFour
}

// Paging

use core::fmt::Debug;

/// A Physical Address
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysAddr(pub crate::arch::defs::PointerTy);

/// A Virtual Address
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtAddr(pub crate::arch::defs::PointerTy);

/// Trait for paging
pub trait Paging: Debug {
    /// translates the given address to a physical one
    fn translate(&self, addr: VirtAddr) -> Option<PhysAddr>;
}

/// Trait for frame allocator
/// 
/// maps to x86_64's impl
pub trait FrameAllocator: Debug {
    /// allocates a frame
    fn alloc_frame(&mut self) -> Option<PhysAddr>;
}
