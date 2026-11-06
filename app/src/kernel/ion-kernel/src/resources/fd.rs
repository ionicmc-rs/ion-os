//! Generic File Descriptor Table
//!
//! This module provides a generic resource management system that can handle
//! any type of resource, not just files. Types implementing the [`FdDispatch`]
//! trait can be registered with the global resource table.

use crate::io;
use alloc::{boxed::Box, sync::Arc, vec::Vec};

use bitflags::bitflags;

// ============================================================================
// OpenFlags
// ============================================================================

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

// ============================================================================
// FdDispatch Trait
// ============================================================================

/// Trait for dispatching file descriptor operations
///
/// Any type that implements this trait can be managed by the global
/// resource table. This allows the FD table to be generic over resource types.
///
/// Note: This trait uses synchronous methods. The global resource table
/// provides async wrappers around these methods.
///
/// # Example
/// ```
/// struct MyResource {
///     data: Vec<u8>,
///     offset: usize,
/// }
///
/// impl FdDispatch for MyResource {
///     fn read(&mut self, buf: &mut [u8]) -> io::Result<u64> {
///         // Implementation
///         Ok(0)
///     }
///
///     fn write(&mut self, data: &[u8]) -> io::Result<u64> {
///         // Implementation
///         Ok(data.len() as u64)
///     }
///
///     fn close(self: Box<Self>) -> io::Result<()> {
///         // Implementation
///         Ok(())
///     }
/// }
/// ```
pub trait FdDispatch: Send + Sync {
    /// Read data from the resource (synchronous)
    ///
    /// # Arguments
    /// - `buf`: Buffer to read into
    ///
    /// # Returns
    /// - Ok(bytes_read) on success
    fn read(&mut self, buf: &mut [u8]) -> io::Result<u64>;

    /// Write data to the resource (synchronous)
    ///
    /// # Arguments
    /// - `data`: Data to write
    ///
    /// # Returns
    /// - Ok(bytes_written) on success
    fn write(&mut self, data: &[u8]) -> io::Result<u64>;

    /// Seek to an absolute position
    ///
    /// # Arguments
    /// - `pos`: Absolute position to seek to
    ///
    /// # Returns
    /// - Ok(new_position) on success
    fn seek(&mut self, pos: u64) -> io::Result<u64>;

    /// Seek relative to current position
    ///
    /// # Arguments
    /// - `delta`: Relative offset (can be negative)
    ///
    /// # Returns
    /// - Ok(new_position) on success
    fn seek_relative(&mut self, delta: i64) -> io::Result<u64>;

    /// Rewind to beginning
    fn rewind(&mut self);

    /// Get current position
    fn tell(&self) -> io::Result<u64>;

    /// Get the open flags
    fn flags(&self) -> OpenFlags;

    /// Check if readable
    fn is_readable(&self) -> bool;

    /// Check if writable
    fn is_writable(&self) -> bool;

    /// Verify this handle has required read permissions
    fn check_readable(&self) -> io::Result<()> {
        if !self.is_readable() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "resource is not opened for reading",
            ));
        }
        Ok(())
    }

    /// Verify this handle has required write permissions
    fn check_writable(&self) -> io::Result<()> {
        if !self.is_writable() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "resource is not opened for writing",
            ));
        }
        Ok(())
    }

    /// Truncate resource to specified size
    ///
    /// # Arguments
    /// - `size`: New size
    fn truncate(&mut self, size: u64) -> io::Result<()>;

    /// Close the resource
    ///
    /// This consumes the resource and performs cleanup.
    fn close(self: Box<Self>) -> io::Result<()>;

    /// Get metadata about the resource (inode, offset, flags)
    ///
    /// Returns (inode, offset, flags) for file resources
    fn metadata(&self) -> io::Result<(u64, u64, OpenFlags)>;
}

// Implement FdDispatch for Box<dyn FdDispatch> to allow dynamic dispatch
impl FdDispatch for Box<dyn FdDispatch> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<u64> {
        (**self).read(buf)
    }

    fn write(&mut self, data: &[u8]) -> io::Result<u64> {
        (**self).write(data)
    }

    fn seek(&mut self, pos: u64) -> io::Result<u64> {
        (**self).seek(pos)
    }

    fn seek_relative(&mut self, delta: i64) -> io::Result<u64> {
        (**self).seek_relative(delta)
    }

    fn rewind(&mut self) {
        (**self).rewind()
    }

    fn tell(&self) -> io::Result<u64> {
        (**self).tell()
    }

    fn flags(&self) -> OpenFlags {
        (**self).flags()
    }

    fn is_readable(&self) -> bool {
        (**self).is_readable()
    }

    fn is_writable(&self) -> bool {
        (**self).is_writable()
    }

    fn truncate(&mut self, size: u64) -> io::Result<()> {
        (**self).truncate(size)
    }

    fn close(self: Box<Self>) -> io::Result<()> {
        (*self).close()
    }

    fn metadata(&self) -> io::Result<(u64, u64, OpenFlags)> {
        (**self).metadata()
    }
}

// ============================================================================
// Resource Table
// ============================================================================

/// Generic resource table that manages any type implementing [`FdDispatch`]
///
/// Manages a set of open file descriptors for a process.
/// Provides O(1) allocation and access while maintaining invariants:
/// - File descriptors are always valid integers (0..len)
/// - File descriptors can only be accessed if they're allocated
/// - Closing a descriptor makes it available for reuse
#[derive(Debug)]
pub struct FdEntry<T: FdDispatch> {
    inner: Arc<spin::Mutex<T>>,
}

impl<T: FdDispatch> Clone for FdEntry<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: FdDispatch> FdEntry<T> {
    /// Create a new refcounted entry.
    pub fn new(handle: T) -> Self {
        Self {
            inner: Arc::new(spin::Mutex::new(handle)),
        }
    }

    /// Execute a read-only operation while holding the entry lock.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let guard = self.inner.lock();
        f(&*guard)
    }

    /// Execute a mutable operation while holding the entry lock.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut guard = self.inner.lock();
        f(&mut *guard)
    }

    /// Current number of file descriptors sharing this handle.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    /// Close the resource only when this is the last FD reference.
    fn try_close(self) -> io::Result<()> {
        match Arc::try_unwrap(self.inner) {
            Ok(mutex) => {
                let resource = mutex.into_inner();
                Box::new(resource).close()
            }
            Err(_) => Ok(()),
        }
    }
}

#[derive(Debug)]
pub struct ResourceTable<T: FdDispatch> {
    entries: Vec<Option<FdEntry<T>>>,
    next_reuse: usize, // hint for next reuse candidate
}

impl<T: FdDispatch> Default for ResourceTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: FdDispatch> ResourceTable<T> {
    /// Create an empty resource table
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_reuse: 3, // 0, 1, 2 are reserved for the std* outputs
        }
    }

    /// Allocate a new file descriptor for the given resource
    ///
    /// # Invariants
    /// - Reuses closed slots when possible (first-fit)
    /// - Returns FD index on success
    pub fn alloc(&mut self, handle: T) -> io::Result<u32> {
        self.alloc_entry(FdEntry::new(handle))
    }

    fn alloc_entry(&mut self, entry: FdEntry<T>) -> io::Result<u32> {
        // Try to reuse a closed slot
        while self.next_reuse < self.entries.len() {
            if self.entries[self.next_reuse].is_none() {
                let fd = self.next_reuse;
                self.entries[fd] = Some(entry);
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

        self.entries.push(Some(entry));
        self.next_reuse = fd + 1;
        Ok(fd as u32)
    }

    /// Get a mutable reference to a resource
    ///
    /// # Returns
    /// - Ok(entry) if FD is valid and allocated
    /// - Err if FD is invalid or was previously closed
    pub fn get_mut(&mut self, fd: u32) -> io::Result<FdEntry<T>> {
        self.get(fd)
    }

    /// Get an immutable reference to a resource
    ///
    /// # Returns
    /// - Ok(entry) if FD is valid and allocated
    /// - Err if FD is invalid or was previously closed
    pub fn get(&self, fd: u32) -> io::Result<FdEntry<T>> {
        self.entries
            .get(fd as usize)
            .and_then(|e| e.as_ref())
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid file descriptor"))
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
        let slot = self.entries.get_mut(fd as usize).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid file descriptor")
        })?;

        if slot.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file descriptor already closed",
            ));
        }

        // Take ownership and close only if this was the last reference
        let entry = slot.take().unwrap();
        entry.try_close()?;

        // Update reuse hint if this is earlier than our current position
        if (fd as usize) < self.next_reuse {
            self.next_reuse = fd as usize;
        }

        Ok(())
    }

    /// Close all file descriptors (e.g. on process exit)
    pub fn close_all(&mut self) {
        for slot in self.entries.iter_mut() {
            if let Some(entry) = slot.take() {
                let _ = entry.try_close();
            }
        }
        self.next_reuse = 3;
    }

    /// Iterator over all allocated file descriptors
    ///
    /// Returns pairs of (fd, handle_entry)
    pub fn iter(&self) -> ResourceTableIter<'_, T> {
        ResourceTableIter {
            table: self,
            index: 0,
        }
    }

    /// Write data to a resource (synchronous)
    ///
    /// # Arguments
    /// - `fd`: File descriptor to write to
    /// - `data`: Data to write
    ///
    /// # Returns
    /// - Ok(bytes_written) on success
    /// - Err if FD is invalid or not writable
    pub fn write(&mut self, fd: u32, data: &[u8]) -> io::Result<u64> {
        let handle = self.get(fd)?;
        handle.with_mut(|resource| {
            resource.check_writable()?;
            T::write(resource, data)
        })
    }

    /// Read data from a resource (synchronous)
    ///
    /// # Arguments
    /// - `fd`: File descriptor to read from
    /// - `buf`: Buffer to read into
    ///
    /// # Returns
    /// - Ok(bytes_read) on success
    /// - Err if FD is invalid or not readable
    pub fn read(&mut self, fd: u32, buf: &mut [u8]) -> io::Result<u64> {
        let handle = self.get(fd)?;
        handle.with_mut(|resource| {
            resource.check_readable()?;
            T::read(resource, buf)
        })
    }

    /// Seek to an absolute position
    ///
    /// # Arguments
    /// - `fd`: File descriptor to seek in
    /// - `pos`: Absolute position to seek to
    ///
    /// # Returns
    /// - Ok(new_position) on success
    /// - Err if FD is invalid
    pub fn seek(&mut self, fd: u32, pos: u64) -> io::Result<u64> {
        let handle = self.get(fd)?;
        handle.with_mut(|resource| T::seek(resource, pos))
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
        let handle = self.get(fd)?;
        handle.with_mut(|resource| T::seek_relative(resource, delta))
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
        let handle = self.get(fd)?;
        handle.with_mut(|resource| T::rewind(resource));
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
        handle.with(|resource| resource.tell())
    }

    /// Truncate file to specified size
    ///
    /// # Arguments
    /// - `fd`: File descriptor
    /// - `size`: New size
    ///
    /// # Returns
    /// - Ok(()) on success
    /// - Err if FD is invalid or not writable
    pub fn truncate(&mut self, fd: u32, size: u64) -> io::Result<()> {
        let handle = self.get(fd)?;
        handle.with_mut(|resource| {
            resource.check_writable()?;
            T::truncate(resource, size)
        })
    }

    /// Duplicate a file descriptor
    ///
    /// Creates a new FD pointing to the same resource with shared state.
    ///
    /// # Arguments
    /// - `fd`: File descriptor to duplicate
    ///
    /// # Returns
    /// - Ok(new_fd) on success
    /// - Err if original FD is invalid or too many files open
    pub fn dup(&mut self, fd: u32) -> io::Result<u32> {
        let handle = self.get(fd)?;
        self.alloc_entry(handle)
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
        let handle = self.get(old_fd)?;

        // POSIX semantics: dup2(fd, fd) is a no-op if fd is valid.
        if old_fd == new_fd {
            return Ok(new_fd);
        }

        if new_fd as usize >= 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "too many open file descriptors",
            ));
        }

        // Close target FD if it exists
        if new_fd < self.entries.len() as u32 {
            let _ = self.close(new_fd);
        }

        // Ensure vector is large enough
        while self.entries.len() <= new_fd as usize {
            self.entries.push(None);
        }

        self.entries[new_fd as usize] = Some(handle);

        if self.next_reuse == new_fd as usize {
            while self.next_reuse < self.entries.len() && self.entries[self.next_reuse].is_some() {
                self.next_reuse += 1;
            }
        }

        Ok(new_fd)
    }

    /// Get metadata about an open file descriptor
    ///
    /// # Arguments
    /// - `fd`: File descriptor
    ///
    /// # Returns
    /// - Ok((inode, offset, flags)) on success
    /// - Err if FD is invalid
    pub fn info(&self, fd: u32) -> io::Result<(u64, u64, OpenFlags)> {
        let handle = self.get(fd)?;
        handle.with(|resource| resource.metadata())
    }

    /// Get the number of references to the same underlying resource for an FD.
    pub fn ref_count(&self, fd: u32) -> io::Result<usize> {
        let handle = self.get(fd)?;
        Ok(handle.ref_count())
    }
}

impl<T: FdDispatch> Drop for ResourceTable<T> {
    fn drop(&mut self) {
        self.close_all();
    }
}

/// Iterator over allocated file descriptors in a table
pub struct ResourceTableIter<'a, T: FdDispatch> {
    table: &'a ResourceTable<T>,
    index: usize,
}

impl<'a, T: FdDispatch> Iterator for ResourceTableIter<'a, T> {
    type Item = (u32, FdEntry<T>);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.table.entries.len() {
            let current = self.index;
            self.index += 1;

            if let Some(ref handle) = self.table.entries[current] {
                return Some((current as u32, handle.clone()));
            }
        }
        None
    }
}

// ============================================================================
// Global Resource Table (using Box<dyn FdDispatch>)
// ============================================================================

/// Type alias for the global resource table that can hold any FdDispatch type
pub type GlobalResourceTable = ResourceTable<Box<dyn FdDispatch>>;

static GLOBAL_RESOURCE_TABLE: spin::Mutex<GlobalResourceTable> =
    spin::Mutex::new(ResourceTable::new());

// ============================================================================
// Global Resource API
// ============================================================================

/// Register a resource and get a file descriptor
///
/// # Arguments
/// - `resource`: The resource to register
///
/// # Returns
/// - Ok(fd) on success
/// - Err if too many resources are open
pub fn alloc(resource: Box<dyn FdDispatch>) -> io::Result<u32> {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.alloc(resource)
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
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.close(fd)
}

/// Duplicate a file descriptor.
///
/// Creates a new descriptor that references the same underlying resource.
pub fn dup(fd: u32) -> io::Result<u32> {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.dup(fd)
}

/// Duplicate a file descriptor into a specific descriptor number.
pub fn dup2(old_fd: u32, new_fd: u32) -> io::Result<u32> {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.dup2(old_fd, new_fd)
}

/// Read from a file descriptor (synchronous)
///
/// Reads data from the resource at the current offset and advances the offset.
///
/// # Arguments
/// - `fd`: File descriptor to read from
/// - `buf`: Buffer to read into
///
/// # Returns
/// - Ok(bytes_read) on success
/// - Err if FD is invalid or not readable
pub fn read(fd: u32, buf: &mut [u8]) -> io::Result<u64> {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.read(fd, buf)
}

/// Write to a file descriptor (synchronous)
///
/// Writes data to the resource at the current offset and advances the offset.
///
/// # Arguments
/// - `fd`: File descriptor to write to
/// - `data`: Data to write
///
/// # Returns
/// - Ok(bytes_written) on success
/// - Err if FD is invalid or not writable
pub fn write(fd: u32, data: &[u8]) -> io::Result<u64> {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.write(fd, data)
}

/// Seek to an absolute position
///
/// # Arguments
/// - `fd`: File descriptor to seek in
/// - `pos`: Absolute position to seek to
///
/// # Returns
/// - Ok(new_position) on success
/// - Err if FD is invalid
pub fn seek(fd: u32, pos: u64) -> io::Result<u64> {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
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
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.seek_relative(fd, delta)
}

/// Rewind resource to beginning
///
/// # Arguments
/// - `fd`: File descriptor to rewind
///
/// # Returns
/// - Ok(()) on success
/// - Err if FD is invalid
pub fn rewind(fd: u32) -> io::Result<()> {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.rewind(fd)
}

/// Get current position in resource
///
/// # Arguments
/// - `fd`: File descriptor
///
/// # Returns
/// - Ok(position) on success
/// - Err if FD is invalid
pub fn tell(fd: u32) -> io::Result<u64> {
    let table = GLOBAL_RESOURCE_TABLE.lock();
    table.tell(fd)
}

/// Truncate resource to specified size
///
/// # Arguments
/// - `fd`: File descriptor
/// - `size`: New size
///
/// # Returns
/// - Ok(()) on success
/// - Err if FD is invalid or not writable
pub fn truncate(fd: u32, size: u64) -> io::Result<()> {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.truncate(fd, size)
}

/// Check if a file descriptor is valid and allocated
///
/// # Arguments
/// - `fd`: File descriptor to check
///
/// # Returns
/// - true if FD is valid and allocated
pub fn is_valid(fd: u32) -> bool {
    let table = GLOBAL_RESOURCE_TABLE.lock();
    table.is_valid(fd)
}

/// Get the number of allocated file descriptors
///
/// # Returns
/// Number of currently open file descriptors
pub fn count() -> u32 {
    let table = GLOBAL_RESOURCE_TABLE.lock();
    table.count()
}

/// Get the reference count for a file descriptor's underlying resource.
pub fn ref_count(fd: u32) -> io::Result<usize> {
    let table = GLOBAL_RESOURCE_TABLE.lock();
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
    let table = GLOBAL_RESOURCE_TABLE.lock();
    table.info(fd)
}

/// Close all file descriptors (e.g. on process exit)
pub fn close_all() {
    let mut table = GLOBAL_RESOURCE_TABLE.lock();
    table.close_all();
}

/// Get the inode index associated with a file descriptor
pub fn inode_index(fd: u32) -> io::Result<u64> {
    let table = GLOBAL_RESOURCE_TABLE.lock();
    let (inode, _, _) = table.info(fd)?;
    Ok(inode)
}
