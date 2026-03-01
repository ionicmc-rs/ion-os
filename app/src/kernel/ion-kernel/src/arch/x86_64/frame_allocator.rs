//! x86_64 physical frame allocator (2 MiB frames + 4 KiB subframes)

use core::ptr::NonNull;
use spin::Mutex;

use crate::{
    arch::defs::PAGE_2MIB,
    c_lib::{
        MultibootMemory,
        USABLE_ENTRY,
        libc::{self, OsError},
    },
};

/* ============================================================
 * Constants
 * ============================================================ */

/// 2 MiB frame size
pub const FRAME_SIZE: usize = PAGE_2MIB;

/// 4 KiB sub-frame size
pub const SMALL_FRAME_SIZE: usize = 4096;

/// Number of 4 KiB frames per 2 MiB frame
const SMALL_FRAMES_PER_LARGE: usize = FRAME_SIZE / SMALL_FRAME_SIZE; // 512

/// Max number of tracked 2 MiB frames subdivided into 4 KiB frames
const MAX_SUBDIVIDED_FRAMES: usize = 128;

/* ============================================================
 * Boot-time 2 MiB frame allocator
 * ============================================================ */

#[derive(Debug)]
pub struct BootInfoFrameAllocator {
    _memory_map: NonNull<MultibootMemory>,
    frames: *mut usize,
    count: usize,
}

impl BootInfoFrameAllocator {
    /// # Safety
    /// - memory_map must be valid
    /// - must be called once
    pub unsafe fn init(memory_map: NonNull<MultibootMemory>) -> Self {
        let mem = memory_map.as_ref();

        const MAX_FRAMES: usize = 4096;
        static mut FRAME_BACKING: [usize; MAX_FRAMES] = [0; MAX_FRAMES];

        let mut idx = 0;
        for e in mem.entries.iter().filter(|e| e.entry_type == USABLE_ENTRY) {
            let start = align_up(e.start_addr(), FRAME_SIZE);
            let end   = align_down(e.end_addr(), FRAME_SIZE);

            let mut a = start;
            while a < end {
                if idx < MAX_FRAMES {
                    FRAME_BACKING[idx] = a;
                    idx += 1;
                }
                a += FRAME_SIZE;
            }
        }

        Self {
            _memory_map: memory_map,
            frames: core::ptr::addr_of_mut!(FRAME_BACKING) as *mut usize,
            count: idx,
        }
    }

    pub fn alloc_frame(&mut self) -> Option<usize> {
        if self.count == 0 {
            libc::set_os_error(OsError::MemoryError);
            return None;
        }
        self.count -= 1;
        Some(unsafe { *self.frames.add(self.count) })
    }
}

/* ============================================================
 * Global 2 MiB allocator wrapper
 * ============================================================ */

struct GlobalAlloc(BootInfoFrameAllocator);
unsafe impl Send for GlobalAlloc {}
unsafe impl Sync for GlobalAlloc {}

static FRAME_ALLOCATOR: Mutex<Option<GlobalAlloc>> = Mutex::new(None);

pub unsafe fn init_global_frame_allocator(
    memory_map: NonNull<MultibootMemory>,
) {
    let mut guard = FRAME_ALLOCATOR.lock();
    assert!(guard.is_none(), "frame allocator initialized twice");
    *guard = Some(GlobalAlloc(BootInfoFrameAllocator::init(memory_map)));
}

pub fn alloc_2mib_frame() -> usize {
    let mut guard = FRAME_ALLOCATOR.lock();
    guard
        .as_mut()
        .expect("frame allocator not initialized")
        .0
        .alloc_frame()
        .expect("out of physical memory")
}

pub fn dealloc_2mib_frame(addr: usize) {
    let mut guard = FRAME_ALLOCATOR.lock();
    let alloc = guard.as_mut().expect("frame allocator not initialized");

    for i in 0..alloc.0.count {
        if unsafe { *alloc.0.frames.add(i) } == addr {
            return;
        }
    }

    const MAX_FRAMES: usize = 4096;
    if alloc.0.count < MAX_FRAMES {
        unsafe { *alloc.0.frames.add(alloc.0.count) = addr; }
        alloc.0.count += 1;
    } else {
        panic!("frame backing full on dealloc");
    }
}

/* ============================================================
 * 4 KiB allocator (with bitmap + reclaim)
 * ============================================================ */

#[derive(Copy, Clone)]
struct SubFrameBlock {
    base: usize,      // 2 MiB frame base
    bitmap: [u64; 8], // 512 bits
    used: usize,
}

struct SmallFrameAllocator {
    blocks: [Option<SubFrameBlock>; MAX_SUBDIVIDED_FRAMES],
}

static SMALL_FRAME_ALLOCATOR: Mutex<SmallFrameAllocator> =
    Mutex::new(SmallFrameAllocator {
        blocks: [None; MAX_SUBDIVIDED_FRAMES],
    });

pub fn alloc_4k_frame() -> Option<usize> {
    let mut alloc = SMALL_FRAME_ALLOCATOR.lock();

    // Try existing blocks first
    for slot in alloc.blocks.iter_mut() {
        if let Some(block) = slot {
            if block.used < SMALL_FRAMES_PER_LARGE {
                for (word_idx, word) in block.bitmap.iter_mut().enumerate() {
                    if *word != u64::MAX {
                        let bit = (!*word).trailing_zeros() as usize;
                        *word |= 1 << bit;
                        block.used += 1;
                        return Some(
                            block.base + (word_idx * 64 + bit) * SMALL_FRAME_SIZE
                        );
                    }
                }
            }
        }
    }

    // Need a new 2 MiB block
    let base = alloc_2mib_frame();

    let mut block = SubFrameBlock {
        base,
        bitmap: [0; 8],
        used: 1,
    };

    block.bitmap[0] = 1;

    for slot in alloc.blocks.iter_mut() {
        if slot.is_none() {
            *slot = Some(block);
            return Some(base);
        }
    }

    // No tracking slots left
    dealloc_2mib_frame(base);
    None
}

pub fn dealloc_4k_frame(addr: usize) {
    let mut alloc = SMALL_FRAME_ALLOCATOR.lock();

    for slot in alloc.blocks.iter_mut() {
        if let Some(block) = slot {
            if addr >= block.base && addr < block.base + FRAME_SIZE {
                let offset = addr - block.base;
                let index = offset / SMALL_FRAME_SIZE;
                let word = index / 64;
                let bit = index % 64;

                let mask = 1u64 << bit;
                assert!(
                    block.bitmap[word] & mask != 0,
                    "double free of 4 KiB frame"
                );

                block.bitmap[word] &= !mask;
                block.used -= 1;

                if block.used == 0 {
                    dealloc_2mib_frame(block.base);
                    *slot = None;
                }
                return;
            }
        }
    }

    panic!("dealloc_4k_frame: address not allocated");
}

/* ============================================================
 * Helpers
 * ============================================================ */

#[inline]
const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

#[inline]
const fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}