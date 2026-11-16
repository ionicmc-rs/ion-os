use core::alloc::Layout;
use core::ptr::NonNull;

use alloc::{
    alloc::{AllocError, Allocator},
    boxed::Box,
};

use crate::{
    address_space::{self, AddressSpace},
    arch::{
        PhysAddr, VirtAddr,
        frame_allocator::alloc_4k_frame,
        mmu::PageTableFlags,
    },
};

/* ============================================================
 * Free list metadata (kernel heap only)
 * ============================================================ */

#[derive(Debug)]
struct FreeBlock {
    base: usize,
    size: usize,
    next: Option<Box<FreeBlock>>,
}

#[derive(Debug)]
/// A real heap allocator that manages a pre-mapped memory region,
/// while storing all metadata on the kernel heap.
///
/// This allocator can optionally map pages into an address space when allocating.
pub struct RegionAllocator {
    head: Option<Box<FreeBlock>>,
    /// Optional address space to map pages into when allocating
    #[allow(dead_code)]
    address_space: Option<AddressSpace>,
}

impl RegionAllocator {
    /// Initialize the allocator over a memory region.
    ///
    /// # Safety
    /// - `base..base+size` MUST be mapped and writable
    /// - Must be called exactly once per region
    pub unsafe fn init(base: usize, size: usize) -> Self {
        Self {
            head: Some(Box::new(FreeBlock {
                base,
                size,
                next: None,
            })),
            address_space: None,
        }
    }

    pub const fn empty() -> Self {
        Self {
            head: None,
            address_space: None,
        }
    }

    /// Initialize with an address space for automatic page mapping
    pub unsafe fn init_with_space(base: usize, size: usize, space: AddressSpace) -> Self {
        Self {
            head: Some(Box::new(FreeBlock {
                base,
                size,
                next: None,
            })),
            address_space: Some(space),
        }
    }

    pub fn start(&self) -> usize {
        self.head.as_ref().map(|b| b.base).unwrap_or(0)
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

    #[allow(invalid_reference_casting)]
    pub fn allocate_with_flags(
        &self,
        layout: Layout,
        flags: PageTableFlags,
    ) -> Option<NonNull<[u8]>> {
        let this = unsafe { &mut *(self as *const _ as *mut RegionAllocator) };

        let base = this.alloc_inner(layout)?;

        // If we have an address space, map the pages
        if let Some(ref mut space) = this.address_space {
            let pages = (layout.size() + 0xFFF) / 0x1000;
            for i in 0..pages {
                let va = base + i * 0x1000;
                let phys = alloc_4k_frame().expect("out of physical memory");

                unsafe {
                    address_space::map_4k_into_space(
                        space,
                        VirtAddr::new(va),
                        PhysAddr::new(phys),
                        PageTableFlags::USER_PAGE
                            | PageTableFlags::WRITABLE
                            | PageTableFlags::PRESENT,
                    );
                }
            }
        }

        Some(unsafe {
            NonNull::slice_from_raw_parts(NonNull::new_unchecked(base as *mut u8), layout.size())
        })
    }
}

/* ============================================================
 * Allocator impl
 * ============================================================ */

#[allow(invalid_reference_casting)]
unsafe impl Allocator for RegionAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        // We need mutable access to map pages - use RefCell pattern
        // This is safe because we're in a single-threaded kernel
        let this = unsafe { &mut *(self as *const _ as *mut RegionAllocator) };

        let base = this.alloc_inner(layout).ok_or(AllocError)?;

        // If we have an address space, map the pages
        if let Some(ref mut space) = this.address_space {
            let pages = (layout.size() + 0xFFF) / 0x1000;
            for i in 0..pages {
                let va = base + i * 0x1000;
                let phys = alloc_4k_frame().expect("out of physical memory");

                unsafe {
                    address_space::map_4k_into_space(
                        space,
                        VirtAddr::new(va),
                        PhysAddr::new(phys),
                        PageTableFlags::USER_PAGE
                            | PageTableFlags::WRITABLE
                            | PageTableFlags::PRESENT,
                    );
                }
            }
        }

        Ok(unsafe {
            NonNull::slice_from_raw_parts(NonNull::new_unchecked(base as *mut u8), layout.size())
        })
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let this = &mut *(self as *const _ as *mut RegionAllocator);
        this.free_inner(ptr.as_ptr() as usize, layout.size().max(1));
    }
}

/* ============================================================
 * Helpers
 * ============================================================ */

#[inline]
fn align_up(x: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (x + align - 1) & !(align - 1)
}
