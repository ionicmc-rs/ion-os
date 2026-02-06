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

use crate::arch::{defs, mmu};

/// A Physical Address
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysAddr(crate::arch::defs::PointerTy);

/// A Virtual Address
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtAddr(crate::arch::defs::PointerTy);

impl VirtAddr {
    /// Creates a new virtual address.
    /// 
    /// # Panics
    /// On some archs, this given address may not be correct, so this function panic
    pub fn new(addr: usize) -> Self {
        if !defs::is_valid_virt_addr(addr) {
            panic!("attempted to create an invalid virtual address. check the arch you are using and make sure you meet the requirements\n\nCANONICAL_BITS for this platform is: {}", defs::CANONICAL_BITS)
        }
        Self(addr as crate::arch::defs::PointerTy)
    }

    /// Attempts to create a new virt addr
    pub fn try_new(addr: usize) -> Option<Self> {
        defs::is_valid_virt_addr(addr)
            .then(|| Self::new(addr))
    }

    /// Converts this type to a usize
    pub fn as_inner(self) -> usize {
        self.0 as usize
    }

    /// Converts this virtual address to [`defs::PointerTy`]
    /// 
    /// This type is not constant across arch, this is mostly a helper for the arch module itself
    /// 
    /// be careful when calling this.
    pub fn as_ptr_ty(self) -> defs::PointerTy {
        self.0
    }
    
    /// Converts this type to a ptr
    pub fn as_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    /// attempts to convert this virtual address to a physical one.
    /// 
    /// Note: this is a one way operation as physical addresses may be mapped to many virtual ones,
    /// so if you need the virtual address again you must store it.
    /// # Safety
    /// see [`mmu::virt_to_phys`]
    pub unsafe fn to_phys(self) -> Option<PhysAddr> {
        unsafe { mmu::virt_to_phys(self) }
    }
}

impl PhysAddr {
    /// Creates a new physical address, panicking if its invalid
    pub fn new(addr: usize) -> Self {
        if !defs::is_valid_phys_addr(addr) {
            panic!("attempted to create an invalid physical address. check the arch you are using and make sure you meet the requirements")
        }
        Self(addr as defs::PointerTy)
    }

    /// Attempts to create a new physical address
    pub fn try_new(addr: usize) -> Option<Self> {
        defs::is_valid_phys_addr(addr)
            .then(|| Self::new(addr))
    }

    /// Converts this physical address to [`defs::PointerTy`]
    /// 
    /// This type is not constant across arch, this is mostly a helper for the arch module itself
    /// 
    /// be careful when calling this.
    pub fn as_ptr_ty(self) -> defs::PointerTy {
        self.0
    }

    /// Converts this physical address to a usize
    pub fn as_inner(self) -> usize {
        self.0 as usize
    }

}