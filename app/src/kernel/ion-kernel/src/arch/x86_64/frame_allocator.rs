//! x86_64 physical frame allocator (2 MiB frames only)

use core::ptr::NonNull;
use crate::{arch::defs::PAGE_SIZE, c_lib::{MultibootMemory, USABLE_ENTRY}, serial_println};

/// 2 MiB (Size of one Frame, subject to change)
pub const FRAME_SIZE: usize = PAGE_SIZE;

/// Frame allocator which uses boot info
#[derive(Debug)]
pub struct BootInfoFrameAllocator {
    memory_map: NonNull<MultibootMemory>,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// # Safety
    /// - memory_map must be valid
    /// - frames must not be in use
    pub unsafe fn init(memory_map: NonNull<MultibootMemory>) -> Self {
        let count = unsafe { memory_map.as_ref() }
            .entries
            .iter()
            .filter(|e| e.entry_type == USABLE_ENTRY)
            .flat_map(|e| {
                let start = align_up(e.start_addr(), FRAME_SIZE);
                let end = align_down(e.end_addr(), FRAME_SIZE);
                (start..end).step_by(FRAME_SIZE)
            })
            .count();
        serial_println!("max frame count: {count}");
        Self { memory_map, next: 0 }
    }

    /// Allocate one 2 MiB physical frame.
    pub fn alloc_frame(&mut self) -> Option<usize> {
        let mem = unsafe { self.memory_map.as_ref() };

        let addr = mem.entries.iter()
            .filter(|e| e.entry_type == USABLE_ENTRY)
            .flat_map(|e| {
                let start = align_up(e.start_addr(), FRAME_SIZE);
                let end = align_down(e.end_addr(), FRAME_SIZE);
                (start..end).step_by(FRAME_SIZE)
            })
            .nth(self.next)?;

        self.next += 1;
        Some(addr)
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
