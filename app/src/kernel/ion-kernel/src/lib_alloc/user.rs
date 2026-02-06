use core::{alloc::{ Layout}, ptr};

use crate::{address_space::{self, AddressSpace}, arch::{PhysAddr, VirtAddr, defs, frame_allocator::alloc_2mib_frame, mmu::PageTableFlags}, lib_alloc::RescheduleAlloc, serial_println};

use core::ptr::NonNull;
use alloc::{alloc::{AllocError, Allocator}, boxed::Box};



/// A free block descriptor.
/// This metadata lives on the *kernel heap*.
#[derive(Debug)]
struct FreeBlock {
    base: usize,
    size: usize,
    next: Option<Box<FreeBlock>>,
}

/// A real heap allocator that manages a pre-mapped memory region,
/// while storing all metadata on the kernel heap.
pub struct RegionAllocator {
    head: Option<Box<FreeBlock>>,
}

impl RegionAllocator {
    /// Initialize the allocator over a memory region.
    ///
    /// # Safety
    /// - `base .. base+size` must be mapped and writable
    /// - Must be called exactly once
    pub unsafe fn init(base: usize, size: usize) -> Self {
        Self {
            head: Some(Box::new(FreeBlock {
                base,
                size,
                next: None,
            })),
        }
    }

    pub const fn empty() -> Self {
        Self { head: None }
    }

    fn alloc_inner(&mut self, layout: Layout) -> Option<usize> {
        let size = layout.size().max(1);
        let align = layout.align();

        let mut cur = &mut self.head;

        while let Some(mut block) = cur.take() {
            let aligned = align_up(block.base, align);
            let padding = aligned - block.base;

            if padding + size <= block.size {
                let alloc_base = aligned;
                let before = padding;
                let after = block.size - (padding + size);

                if before > 0 {
                    block.size = before;

                    if after > 0 {
                        block.next = Some(Box::new(FreeBlock {
                            base: alloc_base + size,
                            size: after,
                            next: block.next.take(),
                        }));
                    }

                    *cur = Some(block);
                } else if after > 0 {
                    *cur = Some(Box::new(FreeBlock {
                        base: alloc_base + size,
                        size: after,
                        next: block.next.take(),
                    }));
                } else {
                    *cur = block.next.take();
                }

                return Some(alloc_base);
            }

            *cur = Some(block);
            cur = &mut cur.as_mut().unwrap().next;
        }

        None
    }

    fn free_inner(&mut self, base: usize, size: usize) {
        let mut node = Box::new(FreeBlock {
            base,
            size,
            next: None,
        });

        let mut cur = &mut self.head;

        while let Some(blk) = cur {
            if base < blk.base {
                break;
            }
            cur = &mut cur.as_mut().unwrap().next;
        }

        node.next = cur.take();
        *cur = Some(node);

        self.merge_adjacent();
    }

    fn merge_adjacent(&mut self) {
        let mut cur = &mut self.head;

        while let Some(a) = cur {
            // While the next block exists and is adjacent, merge it
            while let Some(ref b) = a.next {
                if a.base + a.size == b.base {
                    let mut next = a.next.take().unwrap();
                    a.size += next.size;
                    a.next = next.next.take();
                } else {
                    break;
                }
            }

            cur = &mut a.next;
        }
    }

}

#[allow(invalid_reference_casting)]
unsafe impl Allocator for RegionAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let this = unsafe { &mut *(self as *const _ as *mut RegionAllocator) };

        let base = this.alloc_inner(layout).ok_or(AllocError)?;

        Ok(unsafe {
            NonNull::slice_from_raw_parts(
                NonNull::new_unchecked(base as *mut u8),
                layout.size(),
            )
        })
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let this = unsafe { &mut *(self as *const _ as *mut RegionAllocator) };

        this.free_inner(
            ptr.as_ptr() as usize,
            layout.size().max(1),
        );
    }
}

#[inline]
fn align_up(x: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (x + align - 1) & !(align - 1)
}

/// Allocator that stores user data.
static mut USER_DATA: RegionAllocator = RegionAllocator::empty();

// const defs:

#[allow(static_mut_refs)]
/// returns the allocator used to store user data
pub fn user_data_allocator() -> &'static RegionAllocator {
    unsafe { &USER_DATA }
}

#[allow(static_mut_refs)]
pub fn reserve_space_for_addr(
    space: &mut AddressSpace,
) -> RescheduleAlloc {
    const SIZE: usize = 2 * 1024 * 1024;

    // Allocate virtual address space (no mapping yet)
    let alloc =
        unsafe { USER_DATA
            .allocate(
                Layout::from_size_align(SIZE, SIZE)
                .unwrap()
            )
            .unwrap()
            .as_ptr()
            .cast() };

    let base = alloc as usize;

    unsafe {
        serial_println!("mapping pages");
        let phys = alloc_2mib_frame();

        address_space::map_into_space(
            &mut address_space::kernel_address_space(),  
            VirtAddr::new(base),
            PhysAddr::new(phys), 
            PageTableFlags::WRITABLE
        );

        serial_println!("mapping pages for user space");

        address_space::map_into_space(
            space,
            VirtAddr::new(base),
            PhysAddr::new(phys),
            PageTableFlags::USER_PAGE | PageTableFlags::WRITABLE,
        );
    }

    unsafe {
        ptr::write_bytes(base as *mut u8, 0, SIZE);
    }

    // 3️⃣ Initialize per-address allocator metadata
    let resc_alloc = RescheduleAlloc::empty();

    unsafe {
        resc_alloc
            .0
            .lock()
            .init(alloc, SIZE);
    }

    resc_alloc
}


/// Size for user data.
pub const USER_DATA_SIZE: usize = 4 * 1024 * 1024; // size for user data.
/// base for user data.
pub const USER_DATA_BASE: usize = 0x0000_0000_7000_0000;

/// # Safety
/// must only be called once.
pub unsafe fn init_user_data() {
    unsafe { *(&raw const USER_DATA as *mut _) = RegionAllocator::init(USER_DATA_BASE, USER_DATA_SIZE); }
}