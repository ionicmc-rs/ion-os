//! Tools for separating user and kernel address spaces
//!
//! an [`AddressSpace`] represents a handle to a paging configuration.
//!
//! You can get the current address space from [`crate::thread::current_address_space`]

/// Represents a handle paging configuration
// Note: this is handle, and thus can be copied/cloned
#[derive(Debug, Clone)]
#[repr(C)]
pub struct AddressSpace {
    page_base: PageTable,
    user_alloc: Option<Arc<RegionAllocator>>,
}

use core::cell::OnceCell;

use alloc::sync::Arc;

use crate::{
    arch::{
        self, PhysAddr, VirtAddr,
        frame_allocator::alloc_2mib_frame,
        mmu::{
            ENTRY_PRESENT, PageTable, PageTableFlags, active_pml4, copy_kernel_mappings_to,
            zero_page_table,
        },
    },
    lib_alloc::{
        RescheduleAlloc,
        user::{RegionAllocator, reserve_space_for_addr},
    },
    log,
};

impl AddressSpace {
    /// returns the raw PML4 pointer
    pub fn ptr(&self) -> PhysAddr {
        self.page_base.as_phys()
    }

    /// returns a clone of the user allocator, if any
    pub fn allocator(&self) -> Option<Arc<RegionAllocator>> {
        self.user_alloc.clone()
    }

    pub fn page_table(&self) -> &PageTable {
        &self.page_base
    }

    pub fn page_table_mut(&mut self) -> &mut PageTable {
        &mut self.page_base
    }
}

struct KernelSpace(OnceCell<AddressSpace>);

unsafe impl Sync for KernelSpace {}

static KERNEL_SPACE: KernelSpace = KernelSpace(OnceCell::new());

/// Gets the current kernel address space
///
/// Note: this is cached after the first call.
/// # Safety
/// See [`active_pml4`]
///
/// Additionally, this must be called early on, before any other address spaces are used.
pub unsafe fn kernel_address_space() -> AddressSpace {
    KERNEL_SPACE
        .0
        .get_or_init(|| AddressSpace {
            page_base: unsafe { active_pml4() },
            user_alloc: None,
        })
        .clone() // clone is cheap - pointers are Copy and Arc increases ref count
}

/// Creates a new address space, empty and unpaged.
///
/// this inherits from the active pml4. (assuming it is the kernel)
pub unsafe fn create_address_space(size: usize, align: usize) -> AddressSpace {
    let pml4_phys = alloc_2mib_frame();
    let mut pml4 = PageTable::new(VirtAddr::new(pml4_phys));

    // Zero it
    zero_page_table(&mut pml4);

    copy_kernel_mappings_to(&mut pml4);

    let mut space = AddressSpace {
        page_base: pml4,
        user_alloc: None,
    };
    let alloc = reserve_space_for_addr(&mut space, size, align);
    space.user_alloc = Some(alloc.into());

    log::debug!(
        "created new address space with pml4 at {:#x}",
        space.page_base.as_phys().as_inner()
    );

    space
}

/// maps inside this space
/// # Safety
/// see [`arch::mmu::map_2mib`]
#[track_caller]
pub unsafe fn map_2mib_into_space(
    space: &mut AddressSpace,
    virt: VirtAddr,
    phys: PhysAddr,
    flags: PageTableFlags,
) {
    unsafe { arch::mmu::map_2mib(&mut space.page_base, virt, phys, flags) };
}

/// maps inside this space
/// # Safety
/// see [`arch::mmu::map_4k`]
#[track_caller]
pub unsafe fn map_4k_into_space(
    space: &mut AddressSpace,
    virt: VirtAddr,
    phys: PhysAddr,
    flags: PageTableFlags,
) {
    unsafe { arch::mmu::map_4k(&mut space.page_base, virt, phys, flags) };
}
