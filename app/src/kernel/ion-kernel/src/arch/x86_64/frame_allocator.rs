//! x86_64 physical frame allocator

use core::ptr::NonNull;

use crate::c_lib::{MultibootMemory, USABLE_ENTRY};

/// Frame allocator using boot info
#[derive(Debug)]
pub struct BootInfoFrameAllocator {
    memory_map: NonNull<MultibootMemory>,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// # Safety
    /// Memory map must be valid and frames unused.
    pub unsafe fn init(memory_map: NonNull<MultibootMemory>) -> Self {
        Self { memory_map, next: 0 }
    }

    /// Allocate one 4KiB physical frame.
    pub fn alloc_frame(&mut self) -> Option<usize> {
        let mem = unsafe { self.memory_map.as_ref() };

        let addr = mem.entries.iter()
            .filter(|e| e.entry_type == USABLE_ENTRY)
            .flat_map(|e| e.start_addr()..e.end_addr())
            .step_by(4096)
            .nth(self.next)?;

        self.next += 1;
        Some(addr)
    }
}
