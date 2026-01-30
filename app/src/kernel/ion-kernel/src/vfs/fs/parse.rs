//! Metadata Parsing.

use core::fmt::Display;

use alloc::{vec::Vec};

use crate::{disk::{self, read_at, write_at}, io::{self, Error, ErrorKind}, log, vfs::{fs::IonFSVersion, structures::{INODE_SIZE, Inode, InodeKind, InodeParseErr}}};

// Super Blocks

#[derive(Debug)]
pub enum MetadataParseErr {
    IoFail(io::Error),
    MagicNotFound,
    UnknownVersion,
    /// Block Size does not match driver's
    SizeMismatch,
    /// The rest of the block is not zeros
    ExtraDataStored,
    TooManyInodeBlocks(u64)
}

impl Display for MetadataParseErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MetadataParseErr::IoFail(v) => v.fmt(f),
            MetadataParseErr::ExtraDataStored => write!(f, "extra data stored to block"),
            MetadataParseErr::MagicNotFound => write!(f, "could not find the required verification number"),
            MetadataParseErr::SizeMismatch => write!(f, "disk block size reported by driver does not match stored number"),
            MetadataParseErr::TooManyInodeBlocks(v) => write!(f, "invalid number of inode blocks stored {v}"),
            MetadataParseErr::UnknownVersion => write!(f, "unknown version")
        }
    }
}

impl core::error::Error for MetadataParseErr {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::IoFail(v) => Some(v),
            _ => None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuperBlock {
    version: IonFSVersion,
    reserved: u64,
    // offset for where user data starts
    user_data_start: u64
}


pub const ION_FS_MAGIC_NUMBER: u32 = 0xab87beef;

pub async fn read_superblock(disk: &mut dyn disk::AsyncBlockDevice) -> Result<SuperBlock, MetadataParseErr> {
    // directly read the first sector
    let mut buf = alloc::vec![0; disk.block_size()];
    disk.read_block_async(0, &mut buf).await
        .map_err(MetadataParseErr::IoFail)?;


    // first 4 bytes are the magic number

    if u32::from_le_bytes(*buf[0..4].as_array().unwrap()) != ION_FS_MAGIC_NUMBER {
        return Err(MetadataParseErr::MagicNotFound);
    }

    // next 4 bytes are the version number (validated later)

    let version = *buf[4..8].as_array::<4>().unwrap();    

    // next 8 bytes are the block size, just for checking

    // TODO: when we support 32bit, ensure the correct size is used
    if usize::from_le_bytes(*buf[8..16].as_array().unwrap()) != disk.block_size() {
        return Err(MetadataParseErr::SizeMismatch);
    }

    // next 8 bytes are reserved

    // next 8 bytes are where the user data region starts
    
    let user_data_start = u64::from_le_bytes(*buf[24..32].as_array().unwrap());

    if !buf[32..].iter().all(|v| *v == 0) {
        return Err(MetadataParseErr::ExtraDataStored);
    }
    Ok(SuperBlock {
        version: IonFSVersion::from_array(version).ok_or(MetadataParseErr::UnknownVersion)?,
        reserved: 0,
        user_data_start
    })
}


pub async fn write_superblock(disk: &mut dyn disk::AsyncBlockDevice, version: IonFSVersion, inode_count: u64, user_data_start: u64) -> io::Result {
    let mut buf = Vec::with_capacity(disk.block_size());

    // write magic number

    buf.extend(ION_FS_MAGIC_NUMBER.to_le_bytes());

    // next is the version number

    buf.extend(version.to_array());

    // block size

    buf.extend(disk.block_size().to_le_bytes());

    // inode entry count

    buf.extend(inode_count.to_le_bytes());

    // user data start

    buf.extend(user_data_start.to_le_bytes());

    // fill the rest with zeros

    buf.resize(buf.capacity(), 0);

    disk.write_block_async(0, &buf).await
}

// Alloc Block

pub const ION_FS_ALLOC_MAGIC_NUMBER: u32 = 0xab87befd;

pub struct AllocBlock {
    version: IonFSVersion,
    pub(super) next_free: u64,
    pub(super) inode_blocks: Vec<u64>
}

pub async fn read_alloc_block(disk: &mut dyn disk::AsyncBlockDevice) -> Result<AllocBlock, MetadataParseErr> {
    // read entire block directly
    let mut buf = alloc::vec![0; disk.block_size()];
    disk.read_block_async(1, &mut buf).await
        .map_err(MetadataParseErr::IoFail)?;

    // first 4 bytes are the magic number

    if u32::from_le_bytes(*buf[0..4].as_array().unwrap()) != ION_FS_ALLOC_MAGIC_NUMBER {
        return Err(MetadataParseErr::MagicNotFound);
    }

    // next 4 bytes are the version number (validated later)

    let version = *buf[4..8].as_array::<4>().unwrap();

    // next 8 bytes are the number for the next free block.

    let next_free = u64::from_le_bytes(*buf[8..16].as_array().unwrap());

    // the number of Inode blocks in the coming list

    let inode_block_count = u64::from_le_bytes(*buf[16..24].as_array().unwrap());

    let max_entries = (disk.block_size() as u64 - 24) / 8;
    if inode_block_count > max_entries {
        return Err(MetadataParseErr::TooManyInodeBlocks(inode_block_count))
    }

    let mut inode_blocks = Vec::with_capacity(inode_block_count as usize);

    // Inode blocks actually used
    for n in 0..inode_block_count {
        let lower: usize = 24 + (n as usize * 8);
        let upper: usize = lower + 8;
        let read = u64::from_le_bytes(*buf[lower..upper].as_array().unwrap());
        inode_blocks.push(read);
    }

    Ok(AllocBlock {
        next_free,
        version: IonFSVersion::from_array(version).ok_or(MetadataParseErr::UnknownVersion)?,
        inode_blocks
    })
}

pub async fn write_alloc_block(disk: &mut dyn disk::AsyncBlockDevice, block: &AllocBlock) -> io::Result {
    let mut buf = Vec::with_capacity(disk.block_size());
    buf.extend(ION_FS_ALLOC_MAGIC_NUMBER.to_le_bytes());

    buf.extend(block.version.to_array());

    buf.extend(block.next_free.to_le_bytes());

    buf.extend((block.inode_blocks.len() as u64).to_le_bytes());

    buf.extend(block.inode_blocks.iter().flat_map(|v| v.to_le_bytes()));

    buf.resize(buf.capacity(), 0);

    disk.write_block_async(1, &buf).await?;
    
    Ok(())
}

// Inode Blocks

pub struct InodeBlock {
    free_hint: u16,
    nodes: Vec<Inode>
}

pub async fn read_inode_block(disk: &mut dyn disk::AsyncBlockDevice, block: u64) -> io::Result<InodeBlock> {
    let mut buf = alloc::vec![0; disk.block_size()];
    disk.read_block_async(block, &mut buf).await?;
    
    // first 2 bytes are free hint

    let free_hint = u16::from_le_bytes(*buf[0..2].as_array().unwrap());

    // reserved 2 bytes

    // Inodes

    let mut vector = alloc::vec![];

    for n in (4..disk.block_size()).step_by(INODE_SIZE) {
        if n + INODE_SIZE > buf.len() {
            break;
        }
        vector.push(Inode::from_bytes(*buf[n..(n+INODE_SIZE)].as_array().unwrap())
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
            );
    }

    Ok(InodeBlock { free_hint, nodes: vector })
}

// Inode

fn inodes_per_block(block_size: usize) -> usize {
    (block_size - INODE_BLOCK_HEADER_SIZE) / INODE_SIZE
}

fn inode_location(
    alloc: &AllocBlock,
    block_size: usize,
    inode_id: u64,
) -> (u64, usize) {
    let per_block = inodes_per_block(block_size);
    let block_idx = inode_id as usize / per_block;
    let slot = inode_id as usize % per_block;
    (alloc.inode_blocks[block_idx], slot)
}

#[track_caller]
pub async fn read_inode(disk: &mut dyn disk::AsyncBlockDevice, index: u64) -> io::Result<Inode> {
    let alloc = read_alloc_block(disk).await
        .map_err(Error::other)?;

    let inodes_per_block =
        (disk.block_size() - INODE_BLOCK_HEADER_SIZE)
        / INODE_SIZE;

    let max_inodes =
        alloc.inode_blocks.len() as u64 *
        inodes_per_block as u64;

    if index >= max_inodes {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            alloc::format!("attempt to read inode (index {index}), which doesn't exist"),
        ));
    }

    unsafe { read_inode_unchecked(disk, &alloc, index).await }
}

const INODE_BLOCK_HEADER_SIZE: usize = 4;

/// # Safety
/// Caller must guarantee:
/// - `index` refers to a valid allocated inode
/// - `alloc_block` is trusted and consistent
#[track_caller]
pub async unsafe fn read_inode_unchecked(
    disk: &mut dyn disk::AsyncBlockDevice,
    alloc_block: &AllocBlock,
    index: u64,
) -> io::Result<Inode> {
    log::debug!("Reading inode index {}", index);
    let block_size = disk.block_size();
    
    // how many inodes fit in ONE inode table block
    let inodes_per_block =
        (block_size - INODE_BLOCK_HEADER_SIZE) / INODE_SIZE;

    if inodes_per_block == 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "inode size larger than block payload",
        ));
    }

    // map global inode index → inode table block + slot
    let block_index = (index / inodes_per_block as u64) as usize;
    let slot_index  = (index % inodes_per_block as u64) as usize;

    // SAFETY: caller guarantees index is valid
    let inode_block_number = *alloc_block
        .inode_blocks
        .get(block_index)
        .ok_or_else(|| {
            Error::new(ErrorKind::InvalidInput, "inode index out of range")
        })?;

    // byte offset of the inode inside the disk
    let offset =
        inode_block_number * block_size as u64 +
        INODE_BLOCK_HEADER_SIZE as u64 +
        (slot_index * INODE_SIZE) as u64;

    let mut buf = [0u8; INODE_SIZE];
    read_at(disk, offset, &mut buf).await?;

    Inode::from_bytes(buf)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

pub async unsafe fn write_inode_unchecked(
    disk: &mut dyn disk::AsyncBlockDevice,
    alloc: &AllocBlock,
    inode_id: u64,
    inode: &Inode,
) -> io::Result {
    let (block, slot) =
        inode_location(alloc, disk.block_size(), inode_id);

    let offset =
        block * disk.block_size() as u64 +
        INODE_BLOCK_HEADER_SIZE as u64 +
        (slot * INODE_SIZE) as u64;

    write_at(disk, offset, &inode.as_bytes()).await
}

pub async fn alloc_inode(
    disk: &mut dyn disk::AsyncBlockDevice,
) -> io::Result<u64> {
    let mut alloc = read_alloc_block(disk).await.map_err(Error::other)?;
    let per_block = inodes_per_block(disk.block_size());

    for (block_idx, &block) in alloc.inode_blocks.iter().enumerate() {
        let free_hint = read_inode_block_header(disk, block).await?;

        if free_hint == 0xFFFF {
            continue;
        }

        let inode_id =
            block_idx as u64 * per_block as u64 + free_hint as u64;

        let inode = Inode::used(); // or however you create it

        unsafe {
            write_inode_unchecked(disk, &alloc, inode_id, &inode).await?;
        }

        // update free_hint
        let next = free_hint + 1;
        let new_hint = if next as usize >= per_block {
            0xFFFF
        } else {
            next
        };

        write_inode_block_header(disk, block, new_hint).await?;

        return Ok(inode_id);
    }


    // no space → allocate new inode table block
    let new_block = alloc.next_free;
    alloc.next_free += 1;
    alloc.inode_blocks.push(new_block);
    write_alloc_block(disk, &alloc).await?;

    // zero block
    disk.write_block_async(
        new_block,
        &alloc::vec![0u8; disk.block_size()],
    ).await?;

    Ok((alloc.inode_blocks.len() as u64 - 1) * per_block as u64)
}

pub async fn free_inode(
    disk: &mut dyn disk::AsyncBlockDevice,
    inode_id: u64,
) -> io::Result {
    if inode_id == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "cannot free root inode",
        ));
    }

    let alloc = read_alloc_block(disk).await.map_err(Error::other)?;
    let inode = Inode::unused();

    unsafe {
        write_inode_unchecked(disk, &alloc, inode_id, &inode).await?
    }

    let per_block = inodes_per_block(disk.block_size());

    let block_idx = inode_id as usize / per_block;
    let slot = inode_id as usize % per_block;

    let block = alloc.inode_blocks[block_idx];

    let current_hint = read_inode_block_header(disk, block).await?;

    if current_hint == 0xFFFF || slot < current_hint as usize {
        write_inode_block_header(disk, block, slot as u16).await?;
    }
    Ok(())
}

pub async fn write_inode(
    disk: &mut dyn disk::AsyncBlockDevice,
    inode_id: u64,
    inode: &Inode,
) -> io::Result<()> {
    // root inode rules (optional but recommended)
    if inode_id == 0 && !inode.is_used() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "cannot invalidate root inode",
        ));
    }

    // load allocation metadata
    let alloc = read_alloc_block(disk).await
        .map_err(Error::other)?;

    let inodes_per_block =
        (disk.block_size() - INODE_BLOCK_HEADER_SIZE)
        / INODE_SIZE;

    if inodes_per_block == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "inode size larger than block",
        ));
    }

    let max_inodes =
        alloc.inode_blocks.len() as u64 *
        inodes_per_block as u64;

    // bounds check
    if inode_id >= max_inodes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "inode index out of range",
        ));
    }

    // perform the actual write
    unsafe {
        write_inode_unchecked(disk, &alloc, inode_id, inode).await
    }
}

async fn read_inode_block_header(
    disk: &mut dyn disk::AsyncBlockDevice,
    block: u64,
) -> io::Result<u16> {
    let blk_size = disk.block_size();
    let mut buf = [0u8; 2];
    read_at(disk, block * blk_size as u64, &mut buf).await?;
    Ok(u16::from_le_bytes(buf))
}

async fn write_inode_block_header(
    disk: &mut dyn disk::AsyncBlockDevice,
    block: u64,
    free_hint: u16,
) -> io::Result<()> {
    let blk_size = disk.block_size();
    write_at(
        disk,
        block * blk_size as u64,
        &free_hint.to_le_bytes(),
    )
    .await
}
