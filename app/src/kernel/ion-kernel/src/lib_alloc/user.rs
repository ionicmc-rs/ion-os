use core::alloc::Layout;
use core::ptr::NonNull;

use alloc::{
    alloc::{AllocError, Allocator},
    boxed::Box,
};

use crate::{
    address_space::{self, AddressSpace},
    arch::{
        PhysAddr,
        VirtAddr,
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
pub struct RegionAllocator {
    head: Option<Box<FreeBlock>>,
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
        }
    }

    pub const fn empty() -> Self {
        Self { head: None }
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
}

/* ============================================================
 * Allocator impl
 * ============================================================ */

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

/* ============================================================
 * Global user data allocator
 * ============================================================ */

static mut USER_DATA: RegionAllocator = RegionAllocator::empty();

pub const USER_DATA_SIZE: usize = 4 * 1024 * 1024;
pub const USER_DATA_BASE: usize = 0x0000_0000_7000_0000;

const USER_DATA_PAGES: usize = USER_DATA_SIZE / 0x1000;

/// Physical backing for USER_DATA (allocated once)
static mut USER_DATA_FRAMES: [usize; USER_DATA_PAGES] = [0; USER_DATA_PAGES];

#[allow(static_mut_refs)]
pub fn user_data_allocator() -> &'static RegionAllocator {
    unsafe { &USER_DATA }
}

/* ============================================================
 * Initialization
 * ============================================================ */

/// Initialize USER_DATA backing and allocator.
///
/// # Safety
/// - Must be called exactly once at boot
pub unsafe fn init_user_data() {
    // allocate physical backing ONCE
    for i in 0..USER_DATA_PAGES {
        USER_DATA_FRAMES[i] = alloc_4k_frame().expect("out of physical memory");
    }

    USER_DATA = RegionAllocator::init(USER_DATA_BASE, USER_DATA_SIZE);
}

/// Map USER_DATA into an address space (process or kernel).
///
/// # Safety
/// - `space` must be a valid address space
pub unsafe fn map_user_data(space: &mut AddressSpace) {
    for (i, &phys) in USER_DATA_FRAMES.iter().enumerate() {
        let va = USER_DATA_BASE + i * 0x1000;

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

/* ============================================================
 * Sub-region reservation
 * ============================================================ */

pub fn reserve_space_for_addr(
    _space: &mut AddressSpace,
    size: usize,
    align: usize,
) -> RegionAllocator {
    let alloc = unsafe {
        USER_DATA
            .allocate(Layout::from_size_align(size, align).unwrap())
            .unwrap()
            .as_ptr()
            .cast::<u8>()
    };

    unsafe { RegionAllocator::init(alloc as usize, size) }
}