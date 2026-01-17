//! Tools for managing address spaces.

use alloc::{alloc::AllocError, vec::Vec};

use crate::arch::{self, VirtAddr};



pub struct AddressSpace {
    /// Architecture-defined page table root
    root: arch::mmu::PageTableRoot,

    /// Bookkeeping only
    regions: Vec<VmRegion>,
}


pub struct VmRegion {
    pub start: VirtAddr,
    pub size: usize,
    pub flags: VmFlags,
    pub kind: VmRegionKind,
}

bitflags::bitflags! {
    pub struct VmFlags: u32 {
        const READ    = 1 << 0;
        const WRITE   = 1 << 1;
        const EXEC    = 1 << 2;
        const USER    = 1 << 3; // future use
    }
}

pub enum VmRegionKind {
    Anonymous,
    Kernel,
    ElfSegment,
    Stack,
    Device,
}

impl AddressSpace {
    pub fn new() -> Result<Self, AllocError> {
        Ok(Self {
            root: arch::mmu::create_root()?,
            regions: Vec::new(),
        })
    }

    pub unsafe fn activate(&self) {
        arch::mmu::activate_root(&self.root);
    }

    pub fn map(&mut self, region: VmRegion) {
        for page in region.pages() {
            arch::mmu::map(
                &self.root,
                page.virt,
                page.phys,
                region.flags.into(),
            );
        }
        self.regions.push(region);
    }
}
