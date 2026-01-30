// app/src/kernel/ion-kernel/src/disk/mod.rs

use alloc::vec::Vec;
use core::{pin::Pin, sync::atomic::{AtomicUsize, Ordering}};

use crate::{io::{self}, log};

pub mod default_drivers;

/// Opaque disk identifier (kernel-owned)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DiskId(usize);

impl DiskId {
    #[allow(static_mut_refs)]
    pub fn new(id: usize) -> Option<Self> {
        if id >= unsafe { DISKS.len() } {
            return None;
        }
        Some(DiskId(id))
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }
}

use alloc::boxed::Box;
use core::future::Future;

pub trait AsyncBlockDevice: Send + Sync + 'static {
    fn block_size(&self) -> usize;
    fn block_count(&self) -> u64;

    fn read_block_async<'a>(
        &'a mut self,
        block_number: u64,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result> + 'a>>;

    fn write_block_async<'a>(
        &'a mut self,
        block_number: u64,
        buf: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result> + 'a>>;
}

/// Global disk registry
static mut DISKS: Vec<&'static mut dyn AsyncBlockDevice> = Vec::new();
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

/// Register a disk device (called by drivers)
#[allow(static_mut_refs)]
pub fn register_disk(dev: &'static mut dyn AsyncBlockDevice) -> DiskId {
    // Safety: all devices are registered before accesses - and we ensure data races don't occur
    unsafe {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        DISKS.push(dev);
        DiskId(id)
    }
}

/// Get disk by ID
pub fn get_disk(id: DiskId) -> &'static mut dyn AsyncBlockDevice {
    unsafe { DISKS[id.as_usize()] }
}

/// Initialize disk subsystem
pub fn init() {
    log::info!("disk: initializing subsystem");
    default_drivers::init();
}

// Small API

pub async fn read_at(
    dev: &mut dyn AsyncBlockDevice,
    offset: u64,
    buf: &mut [u8],
) -> io::Result {
    if buf.is_empty() {
        return Ok(());
    }

    let block_size = dev.block_size() as u64;

    let mut remaining = buf;
    let mut cur_offset = offset;

    let mut bounce = alloc::vec![0u8; block_size as usize];

    while !remaining.is_empty() {
        let block = cur_offset / block_size;
        let block_off = (cur_offset % block_size) as usize;

        let take = core::cmp::min(
            remaining.len(),
            block_size as usize - block_off,
        );

        // read full block into bounce
        dev.read_block_async(block, &mut bounce).await?;

        // copy slice out
        remaining[..take]
            .copy_from_slice(&bounce[block_off..block_off + take]);

        remaining = &mut remaining[take..];
        cur_offset += take as u64;
    }

    Ok(())
}

pub async fn write_at(
    dev: &mut dyn AsyncBlockDevice,
    offset: u64,
    buf: &[u8],
) -> io::Result {
    if buf.is_empty() {
        return Ok(());
    }

    let block_size = dev.block_size() as u64;

    let mut remaining = buf;
    let mut cur_offset = offset;

    let mut bounce = alloc::vec![0u8; block_size as usize];

    while !remaining.is_empty() {
        let block = cur_offset / block_size;
        let block_off = (cur_offset % block_size) as usize;

        let take = core::cmp::min(
            remaining.len(),
            block_size as usize - block_off,
        );

        if take != block_size as usize {
            // partial block → read-modify-write
            dev.read_block_async(block, &mut bounce).await?;
        }

        bounce[block_off..block_off + take]
            .copy_from_slice(&remaining[..take]);

        dev.write_block_async(block, &bounce).await?;

        remaining = &remaining[take..];
        cur_offset += take as u64;
    }

    Ok(())
}
