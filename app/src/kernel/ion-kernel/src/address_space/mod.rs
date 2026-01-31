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
    pml4: *mut u64,
    user_alloc: Option<Arc<RescheduleAlloc>>
}

use core::{cell::OnceCell};

use alloc::sync::Arc;

use crate::{arch::{self, PhysAddr, VirtAddr, frame_allocator::alloc_2mib_frame, mmu::{ENTRY_PRESENT, PageTable, PageTableFlags, active_pml4}}, lib_alloc::{RescheduleAlloc, user::reserve_space_for_addr}};

impl AddressSpace {
    /// returns the raw PML4 pointer
    pub fn ptr(&self) -> *mut u64 {
        self.pml4
    }

    /// returns a clone of the user allocator, if any
    pub fn allocator(&self) -> Option<Arc<RescheduleAlloc>> {
        self.user_alloc.clone()
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
    KERNEL_SPACE.0.get_or_init(|| {
        AddressSpace {
            pml4: unsafe { active_pml4().0 },
            user_alloc: None
        }
    }).clone() // clone is cheap - pointers are Copy and Arc increases ref count
}

/// Creates a new address space, empty and unpaged.
/// 
/// this inherits from the active pml4. (assuming it is the kernel)
pub unsafe fn create_address_space() -> AddressSpace {
    // Allocate a new PML4
    let pml4_phys = alloc_2mib_frame();
    let pml4 = pml4_phys as *mut u64;

    // Zero it
    for i in 0..512 {
        unsafe { pml4.add(i).write(0) };
    }

    // Copy kernel mappings
    let kernel_pml4 = unsafe { active_pml4().0 };
    for i in 0..512 {
        let entry = unsafe { kernel_pml4.add(i).read() };
        if entry & ENTRY_PRESENT != 0 {
            unsafe { pml4.add(i).write(entry) };
        }
    }

    let mut space = AddressSpace { pml4, user_alloc: None };
    let alloc = reserve_space_for_addr(&space);
    space.user_alloc = Some(alloc.into());
    space
}

/// maps inside this space
/// # Safety
/// see [`arch::mmu::map_2mib`]
pub unsafe fn map_into_space(
    space: &AddressSpace,
    virt: VirtAddr,
    phys: PhysAddr,
    flags: PageTableFlags,
) {
    unsafe { arch::mmu::map_2mib(
        &mut PageTable(space.pml4),
        virt,
        phys,
        flags,
    ) };
}
