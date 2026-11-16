//! Analytics for IonOS
//! 
//! Cannot be disabled for now

use core::sync::atomic::Ordering;

use crate::{analytics::mem_display::MemDisplay, arch::frame_allocator::{FRAME_SIZE, frame_counts}, lib_alloc::TOTAL_ALLOC, process, scheduler::global};

pub mod mem_display;

/// Physical memory currently tracked by the frame allocator.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct MemoryUsage {
    pub total_bytes: usize,
    pub used_bytes: usize,
    pub used_percentage: f64,
}

/// Returns allocator memory statistics, or `None` before allocator initialization.
pub fn memory_usage() -> Option<MemoryUsage> {
    let (total_frames, free_frames) = frame_counts()?;
    let total_bytes = total_frames.saturating_mul(FRAME_SIZE);
    let used_frames = total_frames.saturating_sub(free_frames);
    let used_bytes = used_frames.saturating_mul(FRAME_SIZE);
    let used_percentage = if total_bytes == 0 {
        0.
    } else {
        ((used_bytes as f64 * 100.0) / total_bytes as f64)
    };

    Some(MemoryUsage {
        total_bytes,
        used_bytes,
        used_percentage,
    })
}

/// Returns the amount of RAM held by allocations in bytes.
///
/// Returns zero before the frame allocator has initialized.
pub fn used_memory() -> usize {
    memory_usage().map_or(0, |usage| usage.used_bytes)
}

/// returns the total amount of usable memory, does not include hardware-reserved
/// or unusable memory
pub fn usable_memory() -> usize {
    memory_usage().map_or(0, |usage| usage.total_bytes)
}

pub fn used_memory_percentage() -> f64 {
    memory_usage().map_or(0., |usage| usage.used_percentage)
}

pub fn total_kernel_allocated() -> usize {
    TOTAL_ALLOC.load(Ordering::SeqCst)
}

pub fn total_active_threads() -> usize {
    global::total_active_threads()
}

pub fn total_processes() -> usize {
    process::PROCESSES.lock().proc_count()
}

pub fn display_memory(bytes: usize) -> MemDisplay {
    MemDisplay::from_bytes(bytes)
}