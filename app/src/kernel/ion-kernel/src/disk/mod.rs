// app/src/kernel/ion-kernel/src/disk/mod.rs

use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{io::{Read, Seek, Write}, log};

pub mod default_drivers;

/// Opaque disk identifier (kernel-owned)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DiskId(usize);

/// Block-level disk interface
pub trait BlockDevice: Send + Sync + Read + Write + Seek {
    fn block_size(&self) -> usize;
    fn block_count(&self) -> u64;
}

/// Global disk registry
static mut DISKS: Vec<&'static mut dyn BlockDevice> = Vec::new();
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

/// Register a disk device (called by drivers)
#[allow(static_mut_refs)]
pub fn register_disk(dev: &'static mut dyn BlockDevice) -> DiskId {
    // Safety: all devices are registered before accesses - and we ensure data races don't occur
    unsafe {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        DISKS.push(dev);
        DiskId(id)
    }
}

/// Get disk by ID
pub fn get_disk(id: usize) -> &'static mut dyn BlockDevice {
    unsafe { DISKS[id] }
}

/// Initialize disk subsystem
pub fn init() {
    log::info!("disk: initializing subsystem");
    default_drivers::init();
}
