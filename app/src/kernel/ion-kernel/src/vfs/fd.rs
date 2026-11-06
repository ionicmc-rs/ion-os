//! VFS File Descriptor management
//!
//! This module provides the VFS-specific file descriptor implementation.
//! It builds on top of the generic resources module, adding file-specific
//! functionality like async disk I/O.

use crate::{
    disk::{self, DiskId},
    io::{self, Error, ErrorKind},
    resources::fd::{FdDispatch, OpenFlags, ResourceTable},
    vfs::{self, fs::parse::read_inode},
};
use alloc::{boxed::Box, vec::Vec};

/// A live, opened file
#[derive(Debug, Clone)]
pub struct FileHandle {
    inode: u64,       // inode index
    offset: u64,      // read/write cursor
    flags: OpenFlags, // open flags
    disk: DiskId,
}

impl FileHandle {
    /// Create a new file handle with validated flags
    pub async fn new(inode: u64, flags: OpenFlags, disk: DiskId) -> io::Result<Self> {
        flags.validate()?;

        let file = read_inode(disk::get_disk(disk), inode).await?;

        if file.kind() != crate::vfs::structures::InodeKind::File {
            return Err(Error::new(
                ErrorKind::IsADirectory,
                "attempted to open a directory as a file",
            ));
        }

        Ok(Self {
            inode,
            offset: if flags.contains(OpenFlags::APPEND) {
                file.size()
            } else {
                0
            },
            flags,
            disk,
        })
    }

    /// Get the inode index
    pub fn inode(&self) -> u64 {
        self.inode
    }

    /// Get the current read/write offset
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Set the read/write offset
    pub fn set_offset(&mut self, offset: u64) {
        self.offset = offset;
    }

    /// Advance the offset by a given amount
    pub fn advance_offset(&mut self, count: u64) {
        self.offset = self.offset.saturating_add(count);
    }

    /// Get the open flags
    pub fn flags(&self) -> OpenFlags {
        self.flags
    }

    /// Check if handle is readable
    pub fn is_readable(&self) -> bool {
        self.flags.is_readable()
    }

    /// Check if handle is writable
    pub fn is_writable(&self) -> bool {
        self.flags.is_writable()
    }

    /// Verify this handle has required permissions
    pub fn check_readable(&self) -> io::Result<()> {
        if !self.is_readable() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file handle is not opened for reading",
            ));
        }
        Ok(())
    }

    /// Verify this handle has required permissions
    pub fn check_writable(&self) -> io::Result<()> {
        if !self.is_writable() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file handle is not opened for writing",
            ));
        }
        Ok(())
    }

    /// Reset offset to beginning (for seeking back to start)
    pub fn rewind(&mut self) {
        self.offset = 0;
    }

    /// Seek to absolute position
    pub fn seek_absolute(&mut self, pos: u64) {
        self.offset = pos;
    }

    /// Seek relative to current position
    pub fn seek_relative(&mut self, delta: i64) {
        if delta < 0 {
            self.offset = self.offset.saturating_sub((-delta) as u64);
        } else {
            self.offset = self.offset.saturating_add(delta as u64);
        }
    }

    /// Get the disk ID
    pub fn disk(&self) -> DiskId {
        self.disk
    }
}

// Implement FdDispatch for FileHandle
impl FdDispatch for FileHandle {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<u64> {
        // Synchronous read - for async use, call the async methods directly
        // This is a blocking implementation
        let disk = disk::get_disk(self.disk);

        // Use block_on pattern via the asyncio module
        crate::asyncio::run_future(async {
            let inode = read_inode(disk, self.inode).await?;
            let bytes = inode.read(disk, self.offset, buf).await?;
            self.advance_offset(bytes as u64);
            Ok(bytes as u64)
        })
    }

    fn write(&mut self, data: &[u8]) -> io::Result<u64> {
        // Synchronous write - for async use, call the async methods directly
        crate::asyncio::run_future(async {
            let disk = disk::get_disk(self.disk);
            let mut inode = read_inode(disk, self.inode).await?;

            if data.len() as u64 > inode.data_size() {
                let alloc =
                    vfs::fs::alloc::alloc_data(disk, (data.len() + (data.len() / 2)) as u64)
                        .await?;
                inode.set_data_offset(alloc);
                inode.set_total_data_size((data.len() + (data.len() / 2)) as u64);
            }

            inode.write_as_file(disk, data).await?;
            self.advance_offset(data.len() as u64);
            Ok(data.len() as u64)
        })
    }

    fn seek(&mut self, pos: u64) -> io::Result<u64> {
        self.seek_absolute(pos);
        Ok(self.offset)
    }

    fn seek_relative(&mut self, delta: i64) -> io::Result<u64> {
        self.seek_relative(delta);
        Ok(self.offset)
    }

    fn rewind(&mut self) {
        self.offset = 0;
    }

    fn tell(&self) -> io::Result<u64> {
        Ok(self.offset)
    }

    fn flags(&self) -> OpenFlags {
        self.flags
    }

    fn is_readable(&self) -> bool {
        self.flags.is_readable()
    }

    fn is_writable(&self) -> bool {
        self.flags.is_writable()
    }

    fn truncate(&mut self, size: u64) -> io::Result<()> {
        crate::asyncio::run_future(async {
            let disk = disk::get_disk(self.disk);
            let mut inode = read_inode(disk, self.inode).await?;

            if inode.size() < size {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "cannot truncate a file to a greater size",
                ));
            }

            let mut buf = alloc::vec![0; inode.size() as usize];
            let bytes = inode.read(disk, 0, &mut buf).await?;

            if bytes as u64 != inode.size() {
                return Err(Error::new(
                    ErrorKind::Interrupted,
                    "could not fully read inode",
                ));
            }

            buf.resize(inode.data_size() as usize, 0);
            buf[(size as usize)..].fill(0);
            inode.write_as_file(disk, &buf).await?;

            Ok(())
        })
    }

    fn close(self: Box<Self>) -> io::Result<()> {
        // File handles don't need special cleanup
        Ok(())
    }

    fn metadata(&self) -> io::Result<(u64, u64, OpenFlags)> {
        Ok((self.inode, self.offset, self.flags))
    }
}

/// Re-export OpenFlags from resources for convenience
pub use crate::resources::fd::OpenFlags as VfsOpenFlags;

/// Type alias for the VFS file descriptor table using FileHandle
pub type FdTable = ResourceTable<FileHandle>;

// Keep the old name for backwards compatibility
pub use FdTable as FileDescriptorTable;

static GLOBAL_FD_TABLE: spin::Mutex<FdTable> = spin::Mutex::new(FdTable::new());

// ============================================================================
// Global File Descriptor API
// ============================================================================

/// Open a file and get a file descriptor
///
/// # Arguments
/// - `inode`: The inode index of the file to open
/// - `flags`: Open flags (READ, WRITE, APPEND, etc.)
///
/// # Returns
/// - Ok(fd) on success
/// - Err if flags are invalid or too many files are open
pub async fn open(disk: DiskId, inode: u64, flags: OpenFlags) -> io::Result<u32> {
    let handle = FileHandle::new(inode, flags, disk).await?;
    let mut table = GLOBAL_FD_TABLE.lock();
    table.alloc(handle)
}

/// Close a file descriptor
///
/// # Arguments
/// - `fd`: File descriptor to close
///
/// # Returns
/// - Ok(()) on success
/// - Err if FD is invalid or already closed
pub fn close(fd: u32) -> io::Result<()> {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.close(fd)
}

/// Read from a file descriptor (async)
///
/// Reads data from the file at the current offset and advances the offset.
///
/// # Arguments
/// - `fd`: File descriptor to read from
/// - `buf`: Buffer to read into
///
/// # Returns
/// - Ok(bytes_read) on success
/// - Err if FD is invalid or not readable
pub async fn read(fd: u32, buf: &mut [u8]) -> io::Result<u64> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };

    let (disk_id, inode_index, offset) = handle.with(|h| {
        h.check_readable()?;
        io::Result::Ok((h.disk(), h.inode(), h.offset()))
    })?;

    let disk = disk::get_disk(disk_id);
    let mut inode = read_inode(disk, inode_index).await?;

    let bytes = inode.read(disk, offset, buf).await?;
    handle.with_mut(|h| h.advance_offset(bytes as u64));
    Ok(bytes as u64)
}

/// Write to a file descriptor (async)
///
/// Writes data to the file at the current offset and advances the offset.
///
/// # Arguments
/// - `fd`: File descriptor to write to
/// - `data`: Data to write
///
/// # Returns
/// - Ok(bytes_written) on success
/// - Err if FD is invalid or not writable
pub async fn write(fd: u32, data: &[u8]) -> io::Result<u64> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };

    let (disk_id, inode_index) = handle.with(|h| {
        h.check_writable()?;
        io::Result::Ok((h.disk(), h.inode()))
    })?;

    let disk = disk::get_disk(disk_id);
    let mut inode = read_inode(disk, inode_index).await?;

    if data.len() as u64 > inode.data_size() {
        let alloc =
            vfs::fs::alloc::alloc_data(disk, (data.len() + (data.len() / 2)) as u64).await?;
        inode.set_data_offset(alloc);
        inode.set_total_data_size((data.len() + (data.len() / 2)) as u64);
    }

    inode.write_as_file(disk, data).await?;
    handle.with_mut(|h| h.advance_offset(data.len() as u64));
    Ok(data.len() as u64)
}

/// Seek to an absolute position in a file
///
/// # Arguments
/// - `fd`: File descriptor to seek in
/// - `pos`: Absolute position to seek to
///
/// # Returns
/// - Ok(new_position) on success
/// - Err if FD is invalid
pub fn seek(fd: u32, pos: u64) -> io::Result<u64> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };
    Ok(handle.with_mut(|h| {
        h.seek_absolute(pos);
        h.offset()
    }))
}

/// Seek relative to current position
///
/// # Arguments
/// - `fd`: File descriptor to seek in
/// - `delta`: Relative offset (can be negative)
///
/// # Returns
/// - Ok(new_position) on success
/// - Err if FD is invalid
pub fn seek_relative(fd: u32, delta: i64) -> io::Result<u64> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };
    Ok(handle.with_mut(|h| {
        h.seek_relative(delta);
        h.offset()
    }))
}

/// Rewind file to beginning
///
/// # Arguments
/// - `fd`: File descriptor to rewind
///
/// # Returns
/// - Ok(()) on success
/// - Err if FD is invalid
pub fn rewind(fd: u32) -> io::Result<()> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };
    handle.with_mut(|h| h.rewind());
    Ok(())
}

/// Get current position in file
///
/// # Arguments
/// - `fd`: File descriptor
///
/// # Returns
/// - Ok(position) on success
/// - Err if FD is invalid
pub fn tell(fd: u32) -> io::Result<u64> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };
    Ok(handle.with(|h| h.offset()))
}

/// Truncate file to specified size
///
/// Note: Actual truncation must be performed by the filesystem.
/// This just validates the FD is writable.
///
/// # Arguments
/// - `fd`: File descriptor
/// - `size`: New size
///
/// # Returns
/// - Ok(()) on success
/// - Err if FD is invalid or not writable
pub async fn truncate(fd: u32, size: u64) -> io::Result<()> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };

    let (disk_id, inode_index) = handle.with(|h| {
        h.check_writable()?;
        io::Result::Ok((h.disk(), h.inode()))
    })?;

    let disk = disk::get_disk(disk_id);
    let mut inode = read_inode(disk, inode_index).await?;

    if inode.size() < size {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "cannot truncate a file to a greater size",
        ));
    }

    let mut buf = alloc::vec![0; inode.size() as usize];
    let bytes = inode.read(disk, 0, &mut buf).await?;

    if bytes as u64 != inode.size() {
        return Err(Error::new(
            ErrorKind::Interrupted,
            "could not fully read inode",
        ));
    }

    buf.resize(inode.data_size() as usize, 0);
    buf[(size as usize)..].fill(0);
    inode.write_as_file(disk, &buf).await?;

    Ok(())
}

/// Duplicate a file descriptor
///
/// Creates a new FD pointing to the same file with the same flags.
///
/// # Arguments
/// - `fd`: File descriptor to duplicate
///
/// # Returns
/// - Ok(new_fd) on success
/// - Err if original FD is invalid or too many files open
pub fn dup(fd: u32) -> io::Result<u32> {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.dup(fd)
}

/// Duplicate file descriptor to a specific number
///
/// If target FD is already open, it will be closed first.
///
/// # Arguments
/// - `old_fd`: Source file descriptor
/// - `new_fd`: Target file descriptor number
///
/// # Returns
/// - Ok(new_fd) on success
/// - Err if old_fd is invalid
pub fn dup2(old_fd: u32, new_fd: u32) -> io::Result<u32> {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.dup2(old_fd, new_fd)
}

/// Check if a file descriptor is valid and allocated
///
/// # Arguments
/// - `fd`: File descriptor to check
///
/// # Returns
/// - true if FD is valid and allocated
pub fn is_valid(fd: u32) -> bool {
    let table = GLOBAL_FD_TABLE.lock();
    table.is_valid(fd)
}

/// Get the number of allocated file descriptors
///
/// # Returns
/// Number of currently open file descriptors
pub fn count() -> u32 {
    let table = GLOBAL_FD_TABLE.lock();
    table.count()
}

/// Get the reference count for a file descriptor's underlying file handle.
pub fn ref_count(fd: u32) -> io::Result<usize> {
    let table = GLOBAL_FD_TABLE.lock();
    table.ref_count(fd)
}

/// Get information about an open file descriptor
///
/// # Arguments
/// - `fd`: File descriptor
///
/// # Returns
/// - Ok((inode, offset, flags)) on success
/// - Err if FD is invalid
pub fn info(fd: u32) -> io::Result<(u64, u64, OpenFlags)> {
    let table = GLOBAL_FD_TABLE.lock();
    table.info(fd)
}

/// Close all file descriptors (e.g. on process exit)
pub fn close_all() {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.close_all();
}

/// Get the inode index associated with a file descriptor
pub fn inode_index(fd: u32) -> io::Result<u64> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };
    Ok(handle.with(|h| h.inode()))
}

pub fn disk_id(fd: u32) -> io::Result<DiskId> {
    let handle = {
        let table = GLOBAL_FD_TABLE.lock();
        table.get(fd)?
    };
    Ok(handle.with(|h| h.disk()))
}
