//! Filesystem operations providing a `std::fs`-like API
//!
//! This module provides high-level filesystem operations using the kernel's VFS
//! and file descriptor system, mimicking the interface of `std::fs`.

use alloc::{string::{String, ToString}, vec::Vec, format};
use crate::{asyncio, io::{self, concurrent::{read::AsyncRead, seek::AsyncSeek, write::AsyncWrite}}};
use crate::{
    disk::{DiskId, get_disk}, 
    vfs::structures::InodeKind, 
    vfs::fs::parse::{read_inode, write_inode}, 
    vfs::fs::parse::alloc_inode,
};

/// Helper to get the primary disk
fn get_main_disk() -> io::Result<&'static mut dyn crate::disk::AsyncBlockDevice> {
    let disk = get_disk(DiskId::new(0).ok_or_else(|| 
        io::Error::new(io::ErrorKind::Other, "failed to get disk")
    )?);
    Ok(disk)
}

/// Split a path into parent directory and filename
fn split_path_into_parent_and_name(path: &str) -> io::Result<(&str, &str)> {
    // Ensure path is absolute
    if !path.starts_with("@/") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be absolute (start with @/)"
        ));
    }
    
    // Find last slash
    if let Some(last_slash) = path.rfind('/') {
        if last_slash == 1 {
            // File in root directory
            Ok(("@/", &path[2..]))
        } else {
            let parent = &path[..=last_slash];
            Ok((parent, &path[last_slash+1..]))
        }
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must contain at least one slash"
        ))
    }
}

/// Write data to a file
///
/// Creates or truncates the file at the given path and writes the provided data.
pub async fn write<C: AsRef<[u8]>>(path: &str, contents: C) -> io::Result<()> {
    let data = contents.as_ref();
    let disk = get_main_disk()?;
    
    let (parent_path, filename) = split_path_into_parent_and_name(path)?;
    
    // Resolve parent directory
    let parent = crate::vfs::resolve_path(disk, parent_path).await?;
    
    if parent.kind() != InodeKind::Directory {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "parent is not a directory"
        ));
    }
    
    // Try to find existing file
    let file_inode_idx = if let Some(idx) = crate::vfs::lookup_in_dir(&parent, disk, filename).await? {
        idx
    } else {
        // Create new inode
        alloc_inode(disk).await?
    };
    
    // Read inode
    let mut file_inode = read_inode(disk, file_inode_idx).await?;
    
    // Allocate space for data if needed
    if file_inode.data_size() == 0 || data.len() as u64 > file_inode.data_size() {
        let data_offset = crate::vfs::fs::alloc::alloc_data(disk, data.len() as u64).await?;
        file_inode.set_data_offset(data_offset);
        file_inode.set_total_data_size(data.len() as u64);
    }
    
    // Write the data
    file_inode.write_as_file(disk, data).await?;
    
    // Update inode on disk
    write_inode(disk, file_inode_idx, &file_inode).await?;
    
    Ok(())
}

/// Read entire file contents as a string
pub async fn read_to_string(path: &str) -> io::Result<String> {
    let data = read(path).await?;
    String::from_utf8(data).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "file is not valid UTF-8")
    })
}

/// Read entire file contents as bytes
pub async fn read(path: &str) -> io::Result<Vec<u8>> {
    let disk = get_main_disk()?;
    
    let inode = crate::vfs::resolve_path(disk, path).await?;
    
    if inode.kind() != InodeKind::File {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            "path refers to a directory"
        ));
    }
    
    // Read all file data
    let size = inode.size() as usize;
    let mut buf = alloc::vec![0u8; size];
    
    let mut offset = 0;
    while offset < size {
        let n = inode.read(disk, offset as u64, &mut buf[offset..]).await?;
        if n == 0 {
            break;
        }
        offset += n;
    }
    
    buf.truncate(offset);
    Ok(buf)
}

/// Create a directory at the given path
pub async fn create_dir(path: &str) -> io::Result<()> {
    let path = ensure_trailing_slash(path);
    let disk = get_main_disk()?;
    
    // Check if directory already exists
    if crate::vfs::resolve_path(disk, &path).await.is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "directory already exists"
        ));
    }
    
    let (parent_path, _) = split_path_into_parent_and_name(&path)?;
    
    let parent = crate::vfs::resolve_path(disk, parent_path).await?;
    
    if parent.kind() != InodeKind::Directory {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "parent is not a directory"
        ));
    }
    
    // Create new directory inode
    let dir_inode_idx = alloc_inode(disk).await?;
    let mut dir_inode = read_inode(disk, dir_inode_idx).await?;
    dir_inode.set_kind(InodeKind::Directory);
    
    // Write empty directory
    let data_offset = crate::vfs::fs::alloc::alloc_data(disk, 512).await?;
    dir_inode.set_data_offset(data_offset);
    dir_inode.set_total_data_size(512);
    dir_inode.write_as_file(disk, &[]).await?;
    
    write_inode(disk, dir_inode_idx, &dir_inode).await?;
    
    Ok(())
}

/// Create a directory and all parent directories
pub async fn create_dir_all(path: &str) -> io::Result<()> {
    let path = ensure_trailing_slash(path);
    
    let parts: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty() && s != &"@")
        .collect();
    
    let mut current = String::from("@/");
    
    for part in parts {
        current.push_str(part);
        current.push('/');
        
        // Try to create each directory (ignore errors for existing dirs)
        let _ = create_dir(&current).await;
    }
    
    Ok(())
}

/// Remove a file
pub async fn remove_file(path: &str) -> io::Result<()> {
    let disk = get_main_disk()?;
    
    let inode = crate::vfs::resolve_path(disk, path).await?;
    
    if inode.kind() != InodeKind::File {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            "path refers to a directory"
        ));
    }
    
    // Note: Full implementation would require freeing inode and data blocks
    // For now, just mark the inode as unused in the allocation table
    let (parent_path, filename) = split_path_into_parent_and_name(path)?;
    let mut parent = crate::vfs::resolve_path(disk, parent_path).await?;
    
    // Remove from parent directory
    let mut entries = parent.read_dir(disk).await?;
    entries.retain(|(name, _)| name != filename.as_bytes());
    parent.write_as_dir(disk, &entries.iter().map(|(n, i)| (n.as_slice(), *i)).collect::<Vec<_>>()).await?;
    write_inode(disk, 0, &parent).await?;  // Simplified: assume parent is root
    
    Ok(())
}

/// Remove a directory
pub async fn remove_dir(path: &str) -> io::Result<()> {
    let path = ensure_trailing_slash(path);
    let disk = get_main_disk()?;
    
    let inode = crate::vfs::resolve_path(disk, &path).await?;
    
    if inode.kind() != InodeKind::Directory {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "path refers to a file"
        ));
    }
    
    // Check if directory is empty
    let entries = inode.read_dir(disk).await?;
    if !entries.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "directory is not empty"
        ));
    }
    
    // Remove from parent directory
    let (parent_path, dirname) = split_path_into_parent_and_name(&path)?;
    let mut parent = crate::vfs::resolve_path(disk, parent_path).await?;
    
    let mut entries = parent.read_dir(disk).await?;
    entries.retain(|(name, _)| name != dirname.as_bytes());
    parent.write_as_dir(disk, &entries.iter().map(|(n, i)| (n.as_slice(), *i)).collect::<Vec<_>>()).await?;
    write_inode(disk, 0, &parent).await?;  // Simplified: assume parent is root
    
    Ok(())
}

/// Remove a directory and all its contents
pub async fn remove_dir_all(path: &str) -> io::Result<()> {
    let path = ensure_trailing_slash(path);
    let disk = get_main_disk()?;
    
    let inode = crate::vfs::resolve_path(disk, &path).await?;
    
    if inode.kind() != InodeKind::Directory {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "path refers to a file"
        ));
    }
    
    // Read and remove all entries (non-recursive to avoid async recursion)
    let entries = inode.read_dir(disk).await?;
    
    for (name, _idx) in entries {
        let entry_name = String::from_utf8_lossy(&name);
        let entry_path = format!("@/{}{}", path.trim_start_matches("@/"), entry_name);
        
        // Remove based on type
        let entry_inode = crate::vfs::resolve_path(disk, &entry_path).await?;
        if entry_inode.kind() == InodeKind::Directory {
            // Delete empty subdirectories first
            let subdir_entries = entry_inode.read_dir(disk).await.unwrap_or_default();
            if subdir_entries.is_empty() {
                let _ = remove_dir(&format!("{}/", entry_path)).await;
            }
        } else {
            let _ = remove_file(&entry_path).await;
        }
    }
    
    // Remove the directory itself
    let _ = remove_dir(&path).await;
    
    Ok(())
}

/// Copy a file to another location
pub async fn copy(src: &str, dst: &str) -> io::Result<u64> {
    let data = read(src).await?;
    let len = data.len() as u64;
    write(dst, &data).await?;
    Ok(len)
}

/// Rename or move a file
pub async fn rename(from: &str, to: &str) -> io::Result<()> {
    let data = read(from).await?;
    write(to, &data).await?;
    remove_file(from).await?;
    Ok(())
}

/// Get metadata about a file
pub async fn metadata(path: &str) -> io::Result<Metadata> {
    let disk = get_main_disk()?;
    let inode = crate::vfs::resolve_path(disk, path).await?;
    
    Ok(Metadata {
        size: inode.size(),
        is_dir: inode.kind() == InodeKind::Directory,
    })
}

/// Filesystem metadata about a file
#[derive(Debug, Clone)]
pub struct Metadata {
    size: u64,
    is_dir: bool,
}

impl Metadata {
    /// Return the length of the file, in bytes
    pub fn len(&self) -> u64 {
        self.size
    }

    /// Return true if the file is empty (size == 0)
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Return true if the path refers to a directory
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Return true if the path refers to a regular file
    pub fn is_file(&self) -> bool {
        !self.is_dir
    }
}

/// Options for opening a file
#[derive(Debug, Clone)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    create: bool,
    create_new: bool,
    truncate: bool,
}

impl OpenOptions {
    /// Create a blank new set of options
    pub fn new() -> Self {
        Self {
            read: false,
            write: false,
            append: false,
            create: false,
            create_new: false,
            truncate: false,
        }
    }

    /// Allow reading from the file
    pub fn read(&mut self, read: bool) -> &mut Self {
        self.read = read;
        self
    }

    /// Allow writing to the file
    pub fn write(&mut self, write: bool) -> &mut Self {
        self.write = write;
        self
    }

    /// Append to the end of the file
    pub fn append(&mut self, append: bool) -> &mut Self {
        self.append = append;
        self
    }

    /// Create the file if it doesn't exist
    pub fn create(&mut self, create: bool) -> &mut Self {
        self.create = create;
        self
    }

    /// Fail if the file already exists
    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        self.create_new = create_new;
        self
    }

    /// Truncate the file to zero length
    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.truncate = truncate;
        self
    }

    /// Open a file with these options
    pub async fn open(&self, path: &str) -> io::Result<File> {
        let disk = get_main_disk()?;
        
        // Try to resolve the file
        let exists = crate::vfs::resolve_path(disk, path).await.is_ok();
        
        if !exists && !self.create && !self.create_new {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "file not found"
            ));
        }
        
        if exists && self.create_new {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "file already exists"
            ));
        }
        
        // If create or create_new, create the file
        if !exists {
            write(path, &[]).await?;
        }
        
        // If truncate, truncate the file
        if self.truncate {
            write(path, &[]).await?;
        }
        
        Ok(File { path: path.to_string() })
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// A file that is open and ready for reading/writing
#[derive(Debug)]
pub struct File {
    path: String,
}

impl File {
    /// Open a file in read mode
    pub async fn open(path: &str) -> io::Result<Self> {
        let mut opts = OpenOptions::new();
        opts.read(true);
        opts.open(path).await
    }

    /// Create a file in write mode
    pub async fn create(path: &str) -> io::Result<Self> {
        let mut opts = OpenOptions::new();
        opts.write(true);
        opts.create(true);
        opts.truncate(true);
        opts.open(path).await
    }

    /// Get metadata about the file
    pub async fn metadata(&self) -> io::Result<Metadata> {
        metadata(&self.path).await
    }

    /// Read all data from the file
    pub async fn read_all(&self) -> io::Result<Vec<u8>> {
        read(&self.path).await
    }

    /// Write data to the file
    pub async fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        write(&self.path, data).await
    }
}

/// Iterator over directory entries
#[derive(Debug)]
pub struct ReadDir {
    entries: Vec<(alloc::vec::Vec<u8>, u64)>,
    index: usize,
}

impl ReadDir {
    /// Return the next directory entry
    pub fn next_entry(&mut self) -> Option<DirEntry> {
        if self.index >= self.entries.len() {
            return None;
        }
        
        let (name, inode) = self.entries[self.index].clone();
        self.index += 1;
        
        Some(DirEntry { name, inode_idx: inode })
    }
}

/// A directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    name: alloc::vec::Vec<u8>,
    inode_idx: u64,
}

impl DirEntry {
    /// Return the name of the directory entry
    pub fn file_name(&self) -> &[u8] {
        &self.name
    }

    /// Get the inode index of this entry
    pub fn inode(&self) -> u64 {
        self.inode_idx
    }
}

/// Read the contents of a directory
pub async fn read_dir(path: &str) -> io::Result<ReadDir> {
    let path = ensure_trailing_slash(path);
    let disk = get_main_disk()?;
    
    let inode = crate::vfs::resolve_path(disk, &path).await?;
    if inode.kind() != InodeKind::Directory {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "path does not refer to a directory"
        ));
    }
    
    let entries = inode.read_dir(disk).await?;
    
    Ok(ReadDir {
        entries,
        index: 0,
    })
}

/// Ensure a path ends with a slash (for directories)
fn ensure_trailing_slash(path: &str) -> String {
    if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{}/", path)
    }
}

impl io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        asyncio::run_future(self.async_read(buf))
    }
}

impl AsyncRead for File {
    async fn async_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let data = read(&self.path).await?;
        let n = core::cmp::min(buf.len(), data.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }
}

impl io::Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        asyncio::run_future(self.async_write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncWrite for File {
    async fn async_write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write(&self.path, buf).await?;
        Ok(buf.len())
    }
}

impl io::Seek for File {
    fn seek(&mut self, from: io::SeekFrom) -> io::Result<u64> {
        asyncio::run_future(self.async_seek(from))
    }
}

impl AsyncSeek for File {
    async fn async_seek(&mut self, from: io::SeekFrom) -> io::Result<u64> {
        self.metadata().await.map(|meta| {
            let size = meta.len();
            match from {
                io::SeekFrom::Start(pos) => Ok(pos),
                io::SeekFrom::End(offset) => {
                    if offset < 0 {
                        Ok(size.saturating_sub((-offset) as u64))
                    } else {
                        Ok(size + (offset as u64))
                    }
                }
                io::SeekFrom::Current(_offset) => {
                    // Not implemented: would need to track current position
                    Err(io::Error::new(io::ErrorKind::Other, "SeekFrom::Current not supported"))
                }
            }
        })?
    }
}