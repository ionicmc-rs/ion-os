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

use crate::arch::defs;

/// A Physical Address
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysAddr(pub crate::arch::defs::PointerTy);

/// A Virtual Address
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtAddr(pub crate::arch::defs::PointerTy);

impl VirtAddr {
    pub fn new(addr: defs::PointerTy) -> Self {
        Self(addr)
    }

    pub fn as_inner(self) -> defs::PointerTy {
        self.0
    }
}

impl PhysAddr {
    pub fn new(addr: defs::PointerTy) -> Self {
        Self(addr)
    }

    pub fn as_inner(self) -> defs::PointerTy {
        self.0
    }
}

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
