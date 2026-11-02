// app/src/kernel/ion-kernel/src/disk/mod.rs

use alloc::{boxed::Box, vec::Vec};
use core::{pin::Pin, sync::atomic::{AtomicUsize, Ordering}};

use crate::{asyncio, io::{self}, log};

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

#[allow(static_mut_refs)]
pub fn disk_count() -> usize {
    unsafe { DISKS.len() }
}

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
    discover_gpt_partitions();
}

const GPT_HEADER_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_ENTRY_SIZE: usize = 128;

#[derive(Clone, Copy, Debug)]
struct PartitionInfo {
    start_lba: u64,
    end_lba: u64,
}

struct PartitionBlockDevice {
    parent: DiskId,
    start_lba: u64,
    block_count: u64,
    block_size: usize,
}

impl PartitionBlockDevice {
    const fn new(parent: DiskId, start_lba: u64, block_count: u64, block_size: usize) -> Self {
        Self {
            parent,
            start_lba,
            block_count,
            block_size,
        }
    }
}

impl AsyncBlockDevice for PartitionBlockDevice {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn read_block_async<'a>(
        &'a mut self,
        block_number: u64,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result> + 'a>> {
        Box::pin(async move {
            if block_number >= self.block_count {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "partition read beyond end of device",
                ));
            }

            let parent = get_disk(self.parent);
            parent
                .read_block_async(self.start_lba + block_number, buf)
                .await
        })
    }

    fn write_block_async<'a>(
        &'a mut self,
        block_number: u64,
        buf: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result> + 'a>> {
        Box::pin(async move {
            if block_number >= self.block_count {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "partition write beyond end of device",
                ));
            }

            let parent = get_disk(self.parent);
            parent
                .write_block_async(self.start_lba + block_number, buf)
                .await
        })
    }
}

fn parse_gpt_partitions(disk_id: DiskId) -> Vec<PartitionInfo> {
    asyncio::run_future(async move {
        let disk = get_disk(disk_id);
        let block_size = disk.block_size();

        if block_size < GPT_ENTRY_SIZE {
            return Vec::new();
        }

        let mut header = alloc::vec![0u8; block_size];
        if disk.read_block_async(1, &mut header).await.is_err() {
            return Vec::new();
        }

        if &header[0..8] != GPT_HEADER_SIGNATURE {
            return Vec::new();
        }

        let entries_lba = u64::from_le_bytes(header[72..80].try_into().unwrap());
        let entry_count = u32::from_le_bytes(header[80..84].try_into().unwrap()) as usize;
        let entry_size = u32::from_le_bytes(header[84..88].try_into().unwrap()) as usize;

        if entry_size < GPT_ENTRY_SIZE || entry_count == 0 {
            return Vec::new();
        }

        let entries_bytes = entry_count.saturating_mul(entry_size);
        let entries_offset = entries_lba.saturating_mul(block_size as u64);
        let mut entries = alloc::vec![0u8; entries_bytes];

        if read_at(disk, entries_offset, &mut entries).await.is_err() {
            return Vec::new();
        }

        let mut partitions = Vec::new();
        for entry in entries.chunks(entry_size) {
            if entry.len() < GPT_ENTRY_SIZE {
                break;
            }

            if entry[..16].iter().all(|b| *b == 0) {
                continue;
            }

            let first_lba = u64::from_le_bytes(entry[32..40].try_into().unwrap());
            let last_lba = u64::from_le_bytes(entry[40..48].try_into().unwrap());

            if first_lba == 0 || last_lba < first_lba {
                continue;
            }

            partitions.push(PartitionInfo {
                start_lba: first_lba,
                end_lba: last_lba,
            });
        }

        partitions
    })
}

fn discover_gpt_partitions() {
    let physical_disk_count = disk_count();

    for disk_index in 0..physical_disk_count {
        let Some(parent) = DiskId::new(disk_index) else {
            continue;
        };

        let partitions = parse_gpt_partitions(parent);
        if partitions.is_empty() {
            continue;
        }

        let block_size = get_disk(parent).block_size();

        for (partition_index, partition) in partitions.into_iter().enumerate() {
            let block_count = partition.end_lba - partition.start_lba + 1;
            let dev = Box::leak(Box::new(PartitionBlockDevice::new(
                parent,
                partition.start_lba,
                block_count,
                block_size,
            )));

            let partition_disk = register_disk(dev);
            log::info!(
                "disk: registered GPT partition {} from disk {:?} as {:?} (start_lba={}, blocks={})",
                partition_index,
                parent,
                partition_disk,
                partition.start_lba,
                block_count,
            );
        }
    }
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
