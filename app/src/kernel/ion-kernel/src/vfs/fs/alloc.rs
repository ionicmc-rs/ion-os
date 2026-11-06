use crate::{
    disk,
    io::{self, Error},
    vfs::fs::parse::{read_alloc_block, write_alloc_block},
};

pub async fn alloc_data(disk: &mut dyn disk::AsyncBlockDevice, size: u64) -> io::Result<u64> {
    if size == 0 {
        return Ok(0);
    }

    let mut alloc = read_alloc_block(disk).await.map_err(Error::other)?;

    let block_size = disk.block_size() as u64;
    let blocks = (size + block_size - 1) / block_size;

    let data_offset = alloc.next_free * block_size;

    alloc.next_free += blocks;

    write_alloc_block(disk, &alloc).await?;

    Ok(data_offset)
}

/// returns the block index of the newly allocated inode table block
pub async fn alloc_inode_table(disk: &mut dyn disk::AsyncBlockDevice) -> io::Result<u64> {
    let blk_size = disk.block_size();
    let data = alloc_data(disk, blk_size as u64).await?;

    let mut alloc = read_alloc_block(disk).await.map_err(Error::other)?;

    alloc.inode_blocks.push(data);

    write_alloc_block(disk, &alloc).await?;

    Ok(data / blk_size as u64)
}
