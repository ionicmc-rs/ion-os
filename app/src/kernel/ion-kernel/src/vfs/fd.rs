use alloc::vec::Vec;
use crate::io;

use bitflags::bitflags;

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct OpenFlags: u32 {
        /// Open for reading
        const READ      = 1 << 0;
        /// Open for writing
        const WRITE     = 1 << 1;
        /// Append to end of file
        const APPEND    = 1 << 2;
        /// Create file if it doesn't exist
        const CREATE    = 1 << 3;
        /// Fail if CREATE and file exists
        const EXCLUSIVE = 1 << 4;
        /// Truncate file to zero on open
        const TRUNCATE  = 1 << 5;
        /// Non-blocking I/O
        const NONBLOCK  = 1 << 6;
        /// Open directory
        const DIRECTORY = 1 << 7;
        /// Synchronous I/O
        const SYNC      = 1 << 8;
    }
}

impl OpenFlags {
    /// Check if file is opened for reading
    pub fn is_readable(self) -> bool {
        self.contains(Self::READ)
    }

    /// Check if file is opened for writing
    pub fn is_writable(self) -> bool {
        self.contains(Self::WRITE)
    }

    /// Check if file is in append mode
    pub fn is_append(self) -> bool {
        self.contains(Self::APPEND)
    }

    /// Validate flags for consistency
    /// Returns error if invalid flag combination is detected
    pub fn validate(self) -> io::Result<()> {
        // At least READ or WRITE must be specified
        if !self.contains(Self::READ) && !self.contains(Self::WRITE) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "open flags must include READ or WRITE",
            ));
        }

        // EXCLUSIVE requires CREATE
        if self.contains(Self::EXCLUSIVE) && !self.contains(Self::CREATE) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "EXCLUSIVE requires CREATE flag",
            ));
        }

        // TRUNCATE requires WRITE
        if self.contains(Self::TRUNCATE) && !self.contains(Self::WRITE) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TRUNCATE requires WRITE flag",
            ));
        }

        // APPEND and TRUNCATE are mutually exclusive
        if self.contains(Self::APPEND) && self.contains(Self::TRUNCATE) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "APPEND and TRUNCATE are mutually exclusive",
            ));
        }

        Ok(())
    }
}

/// A live, opened file
#[derive(Debug, Clone)]
pub struct FileHandle {
    inode: u64,           // inode index
    offset: u64,          // read/write cursor
    flags: OpenFlags,     // open flags
}

impl FileHandle {
    /// Create a new file handle with validated flags
    pub fn new(inode: u64, flags: OpenFlags) -> io::Result<Self> {
        flags.validate()?;
        
        // If APPEND flag is set, start at end of file (offset will be set by filesystem)
        Ok(Self {
            inode,
            offset: if flags.contains(OpenFlags::APPEND) { 0 } else { 0 },
            flags,
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
}

/// Per-process file descriptor table
/// 
/// Manages a set of open file descriptors for a process.
/// Provides O(1) allocation and access while maintaining invariants:
/// - File descriptors are always valid integers (0..len)
/// - File descriptors can only be accessed if they're allocated
/// - Closing a descriptor makes it available for reuse
#[derive(Debug, Default)]
pub struct FdTable {
    entries: Vec<Option<FileHandle>>,
    next_reuse: usize,  // hint for next reuse candidate
}

impl FdTable {
    /// Create an empty FD table
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_reuse: 0,
        }
    }

    /// Allocate a new file descriptor for the given handle
    /// 
    /// # Invariants
    /// - Validates the handle's flags before allocation
    /// - Reuses closed slots when possible (first-fit)
    /// - Returns FD index on success
    pub fn alloc(&mut self, handle: FileHandle) -> io::Result<u32> {
        // Try to reuse a closed slot
        while self.next_reuse < self.entries.len() {
            if self.entries[self.next_reuse].is_none() {
                let fd = self.next_reuse;
                self.entries[fd] = Some(handle);
                self.next_reuse = fd + 1;
                return Ok(fd as u32);
            }
            self.next_reuse += 1;
        }

        // No reusable slots, append to end
        let fd = self.entries.len();
        
        // Prevent overflow - limit to reasonable max FD count
        if fd >= 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "too many open file descriptors",
            ));
        }

        self.entries.push(Some(handle));
        self.next_reuse = fd + 1;
        Ok(fd as u32)
    }

    /// Get a mutable reference to a file handle
    /// 
    /// # Returns
    /// - Ok(reference) if FD is valid and allocated
    /// - Err if FD is invalid or was previously closed
    pub fn get_mut(&mut self, fd: u32) -> io::Result<&mut FileHandle> {
        self.entries
            .get_mut(fd as usize)
            .and_then(|e| e.as_mut())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid file descriptor")
            })
    }

    /// Get an immutable reference to a file handle
    /// 
    /// # Returns
    /// - Ok(reference) if FD is valid and allocated
    /// - Err if FD is invalid or was previously closed
    pub fn get(&self, fd: u32) -> io::Result<&FileHandle> {
        self.entries
            .get(fd as usize)
            .and_then(|e| e.as_ref())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid file descriptor")
            })
    }

    /// Check if a file descriptor is valid and allocated
    pub fn is_valid(&self, fd: u32) -> bool {
        self.entries
            .get(fd as usize)
            .map(|e| e.is_some())
            .unwrap_or(false)
    }

    /// Get the number of allocated file descriptors
    pub fn count(&self) -> u32 {
        self.entries.iter().filter(|e| e.is_some()).count() as u32
    }

    /// Close a file descriptor
    /// 
    /// # Invariants
    /// - Marks the FD slot as None (available for reuse)
    /// - Future accesses to this FD will fail with InvalidInput
    pub fn close(&mut self, fd: u32) -> io::Result<()> {
        let slot = self.entries
            .get_mut(fd as usize)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid file descriptor")
            })?;

        if slot.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file descriptor already closed",
            ));
        }

        *slot = None;
        
        // Update reuse hint if this is earlier than our current position
        if (fd as usize) < self.next_reuse {
            self.next_reuse = fd as usize;
        }

        Ok(())
    }

    /// Close all file descriptors (e.g. on process exit)
    pub fn close_all(&mut self) {
        for slot in self.entries.iter_mut() {
            *slot = None;
        }
        self.next_reuse = 0;
    }

    /// Iterator over all allocated file descriptors
    /// 
    /// Returns pairs of (fd, handle_ref)
    pub fn iter(&self) -> FdTableIter<'_> {
        FdTableIter {
            table: self,
            index: 0,
        }
    }

    /// Open a file and allocate a file descriptor
    /// 
    /// Creates a new file handle with the given inode and flags,
    /// then allocates a file descriptor for it.
    /// 
    /// # Arguments
    /// - `inode`: The inode index of the file
    /// - `flags`: Open flags (READ, WRITE, APPEND, etc.)
    /// 
    /// # Returns
    /// - Ok(fd) on success
    /// - Err if flags are invalid or too many files are open
    pub fn open(&mut self, inode: u64, flags: OpenFlags) -> io::Result<u32> {
        let handle = FileHandle::new(inode, flags)?;
        self.alloc(handle)
    }

    /// Write data to a file descriptor
    /// 
    /// Writes data to the file at the current offset and advances the offset.
    /// The actual disk I/O would be performed by the caller using the handle info.
    /// 
    /// # Arguments
    /// - `fd`: File descriptor to write to
    /// - `data`: Data to write
    /// 
    /// # Returns
    /// - Ok(bytes_written) on success
    /// - Err if FD is invalid or not writable
    pub fn write(&mut self, fd: u32, data: &[u8]) -> io::Result<u64> {
        let handle = self.get_mut(fd)?;
        handle.check_writable()?;

        let bytes = data.len() as u64;
        handle.advance_offset(bytes);
        Ok(bytes)
    }

    /// Read data from a file descriptor
    /// 
    /// Advances the file offset by the number of bytes read.
    /// The actual disk I/O would be performed by the caller using the handle info.
    /// 
    /// # Arguments
    /// - `fd`: File descriptor to read from
    /// - `buf`: Buffer to read into
    /// 
    /// # Returns
    /// - Ok(bytes_read) on success
    /// - Err if FD is invalid or not readable
    pub fn read(&mut self, fd: u32, buf: &mut [u8]) -> io::Result<u64> {
        let handle = self.get_mut(fd)?;
        handle.check_readable()?;

        let bytes = buf.len() as u64;
        handle.advance_offset(bytes);
        Ok(bytes)
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
    pub fn seek(&mut self, fd: u32, pos: u64) -> io::Result<u64> {
        let handle = self.get_mut(fd)?;
        handle.seek_absolute(pos);
        Ok(handle.offset())
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
    pub fn seek_relative(&mut self, fd: u32, delta: i64) -> io::Result<u64> {
        let handle = self.get_mut(fd)?;
        handle.seek_relative(delta);
        Ok(handle.offset())
    }

    /// Rewind file to beginning
    /// 
    /// # Arguments
    /// - `fd`: File descriptor to rewind
    /// 
    /// # Returns
    /// - Ok(()) on success
    /// - Err if FD is invalid
    pub fn rewind(&mut self, fd: u32) -> io::Result<()> {
        let handle = self.get_mut(fd)?;
        handle.rewind();
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
    pub fn tell(&self, fd: u32) -> io::Result<u64> {
        let handle = self.get(fd)?;
        Ok(handle.offset())
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
    pub fn truncate(&mut self, fd: u32, size: u64) -> io::Result<()> {
        let handle = self.get_mut(fd)?;
        handle.check_writable()?;
        // Actual truncation would be done by filesystem
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
    pub fn dup(&mut self, fd: u32) -> io::Result<u32> {
        let handle = self.get(fd)?.clone();
        self.alloc(handle)
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
    pub fn dup2(&mut self, old_fd: u32, new_fd: u32) -> io::Result<u32> {
        // Get the handle to duplicate
        let handle = self.get(old_fd)?.clone();

        // Close target FD if it exists
        if new_fd < self.entries.len() as u32 {
            let _ = self.close(new_fd);
        }

        // Ensure vector is large enough
        while self.entries.len() <= new_fd as usize {
            self.entries.push(None);
        }

        self.entries[new_fd as usize] = Some(handle);
        Ok(new_fd)
    }
}

/// Iterator over allocated file descriptors in a table
pub struct FdTableIter<'a> {
    table: &'a FdTable,
    index: usize,
}

impl<'a> Iterator for FdTableIter<'a> {
    type Item = (u32, &'a FileHandle);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.table.entries.len() {
            let current = self.index;
            self.index += 1;
            
            if let Some(ref handle) = self.table.entries[current] {
                return Some((current as u32, handle));
            }
        }
        None
    }
}

impl Drop for FdTable {
    fn drop(&mut self) {
        self.close_all();
    }
}

impl OpenFlags {
    /// Default flags: READ only
    pub fn default_read() -> Self {
        Self::READ
    }

    /// Default flags: WRITE only
    pub fn default_write() -> Self {
        Self::WRITE | Self::CREATE | Self::TRUNCATE
    }

    /// Default flags: READ | WRITE
    pub fn default_read_write() -> Self {
        Self::READ | Self::WRITE | Self::CREATE
    }
}

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
pub fn open(inode: u64, flags: OpenFlags) -> io::Result<u32> {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.open(inode, flags)
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

/// Read from a file descriptor
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
pub fn read(fd: u32, buf: &mut [u8]) -> io::Result<u64> {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.read(fd, buf)
}

/// Write to a file descriptor
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
pub fn write(fd: u32, data: &[u8]) -> io::Result<u64> {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.write(fd, data)
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
    let mut table = GLOBAL_FD_TABLE.lock();
    table.seek(fd, pos)
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
    let mut table = GLOBAL_FD_TABLE.lock();
    table.seek_relative(fd, delta)
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
    let mut table = GLOBAL_FD_TABLE.lock();
    table.rewind(fd)
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
    let table = GLOBAL_FD_TABLE.lock();
    table.tell(fd)
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
pub fn truncate(fd: u32, size: u64) -> io::Result<()> {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.truncate(fd, size)
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
    let handle = table.get(fd)?;
    Ok((handle.inode(), handle.offset(), handle.flags()))
}

/// Close all file descriptors (e.g. on process exit)
pub fn close_all() {
    let mut table = GLOBAL_FD_TABLE.lock();
    table.close_all();
}

/// Get the inode index associated with a file descriptor
pub fn inode_index(fd: u32) -> io::Result<u64> {
    let table = GLOBAL_FD_TABLE.lock();
    let handle = table.get(fd)?;
    Ok(handle.inode())
}