//! x86_64 physical frame allocator (2 MiB frames only)

use core::ptr::NonNull;
use crate::{arch::defs::PAGE_SIZE, c_lib::{MultibootMemory, USABLE_ENTRY}};

/// 2 MiB (Size of one Frame, subject to change)
pub const FRAME_SIZE: usize = PAGE_SIZE;

/// Frame allocator which uses boot info
#[derive(Debug)]
pub struct BootInfoFrameAllocator {
    memory_map: NonNull<MultibootMemory>,
    // Pre-filled free-list of frame physical addresses (pointer to backing array)
    frames: *mut usize,
    count: usize,
}

impl BootInfoFrameAllocator {
    /// # Safety
    /// - memory_map must be valid
    /// - frames must not be in use
    pub unsafe fn init(memory_map: NonNull<MultibootMemory>) -> Self {
        let mem = unsafe { memory_map.as_ref() };

        // Static backing for frame list. MAX_FRAMES covers up to 8 GiB (4096 * 2MiB).
        const MAX_FRAMES: usize = 4096;
        static mut FRAME_BACKING: [usize; MAX_FRAMES] = [0; MAX_FRAMES];

        let mut idx: usize = 0;
        for e in mem.entries.iter().filter(|e| e.entry_type == USABLE_ENTRY) {
            let start = align_up(e.start_addr(), FRAME_SIZE);
            let end = align_down(e.end_addr(), FRAME_SIZE);
            let mut a = start;
            while a < end {
                if idx < MAX_FRAMES {
                    unsafe { FRAME_BACKING[idx] = a; }
                    idx += 1;
                }
                a += FRAME_SIZE;
            }
        }

        // Provide the backing pointer (use raw pointer API to avoid creating
        // a mutable reference to a mutable static under the rust_2024 rules)
        let frames = unsafe { core::ptr::addr_of_mut!(FRAME_BACKING) as *mut usize };

        Self { memory_map, frames, count: idx }
    }

    /// Allocate one 2 MiB physical frame.
    pub fn alloc_frame(&mut self) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        // Pop from end for LIFO allocation
        self.count -= 1;
        let addr = unsafe { *self.frames.add(self.count) };
        Some(addr)
    }

        /// Deallocate a previously allocated 2 MiB physical frame.
        ///
        /// If the frame is already freed or the backing array is full, this will
        /// either return silently (if already freed) or panic when out of space.
        pub fn dealloc_2mib_frame(addr: usize) {
            let mut guard = FRAME_ALLOCATOR.lock();
            let alloc = guard
                .as_mut()
                .expect("frame allocator not initialized");

            // Check for duplicates
            for i in 0..alloc.0.count {
                let existing = unsafe { *alloc.0.frames.add(i) };
                if existing == addr {
                    // already freed
                    return;
                }
            }

            // push back if space
            const MAX_FRAMES: usize = 4096;
            if alloc.0.count < MAX_FRAMES {
                unsafe { *alloc.0.frames.add(alloc.0.count) = addr; }
                alloc.0.count += 1;
            } else {
                panic!("frame backing full on dealloc");
            }
        }
}

#[inline]
const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

#[inline]
const fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

use spin::Mutex;

struct GlobalAlloc(BootInfoFrameAllocator);

unsafe impl Send for GlobalAlloc {}
unsafe impl Sync for GlobalAlloc {}

static FRAME_ALLOCATOR: Mutex<Option<GlobalAlloc>> =
    Mutex::new(None);

/// Initialize the global frame allocator.
///
/// # Safety
/// - Must be called exactly once
/// - `memory_map` must be valid
pub unsafe fn init_global_frame_allocator(
    memory_map: NonNull<MultibootMemory>,
) {
    let mut guard = FRAME_ALLOCATOR.lock();

    assert!(guard.is_none(), "frame allocator initialized twice");

    let alloc = unsafe { BootInfoFrameAllocator::init(memory_map) };
    *guard = Some(GlobalAlloc(alloc));
}

/// Allocate one 2 MiB physical frame.
///
/// Panics if allocator is not initialized or out of memory.
pub fn alloc_2mib_frame() -> usize {
    let mut guard = FRAME_ALLOCATOR.lock();
    let alloc = guard
        .as_mut()
        .expect("frame allocator not initialized");
    alloc.0
        .alloc_frame()
        .expect("out of physical memory")
}

/// Public wrapper to deallocate a previously allocated 2 MiB physical frame.
pub fn dealloc_2mib_frame(addr: usize) {
    let mut guard = FRAME_ALLOCATOR.lock();
    let alloc = guard
        .as_mut()
        .expect("frame allocator not initialized");

    // Check for duplicates
    for i in 0..alloc.0.count {
        let existing = unsafe { *alloc.0.frames.add(i) };
        if existing == addr {
            // already freed
            return;
        }
    }

    // push back if space
    const MAX_FRAMES: usize = 4096;
    if alloc.0.count < MAX_FRAMES {
        unsafe { *alloc.0.frames.add(alloc.0.count) = addr; }
        alloc.0.count += 1;
    } else {
        panic!("frame backing full on dealloc");
    }
}
