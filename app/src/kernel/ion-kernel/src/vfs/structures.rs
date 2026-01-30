use core::{ffi::CStr, fmt::Display};

use alloc::vec::Vec;

use crate::{disk::write_at, io, log, serial_println};

/// One Node in the file system
#[repr(C)]
#[derive(Debug)]
pub struct Inode {
    used: bool,
    kind: InodeKind,
    data_size: u64,
    data_offset: u64,
    // used size rather than allocated
    size: u64
}

pub const INODE_SIZE: usize = 26;

#[derive(Debug, Clone, Copy)]
pub enum InodeParseErr {
    InvalidKind(u8),
    InvalidBool(u8)
}

impl Display for InodeParseErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidKind(x) => write!(f, "invalid kind for inode: {x}"),
            Self::InvalidBool(x) => write!(f, "invalid usage marker for inode: {x}")
        }
    }
}

impl core::error::Error for InodeParseErr {}

impl Inode {
    pub(super) fn from_bytes(bytes: [u8; INODE_SIZE]) -> Result<Self, InodeParseErr> {
        // first byte is wether this inode is used.
        let used = if bytes[0] == 0 {
            false
        } else if bytes[0] == 1 {
            true
        } else {
            return Err(InodeParseErr::InvalidBool(bytes[0]));
        };
        // second byte is the kind
        let kind = InodeKind::from_u8(bytes[1])?;
        let data_size = u64::from_le_bytes(*bytes[2..10].as_array().unwrap());
        let data_offset = u64::from_le_bytes(*bytes[10..18].as_array().unwrap());
        let used_size = u64::from_le_bytes(*bytes[18..26].as_array().unwrap());

        Ok(Self { kind, data_size, data_offset, used, size: used_size })
    }

    pub(super) fn as_bytes(&self) -> [u8; INODE_SIZE] {
        let mut res = [0; INODE_SIZE];
        res[0] = self.used as u8;
        res[1] = self.kind as u8;

        for (i, x) in self.data_size.to_le_bytes().iter().enumerate() {
            res[i+2] = *x; 
        }
        for (i, x) in self.data_offset.to_le_bytes().iter().enumerate() {
            res[i+10] = *x; 
        }

        for (i, x) in self.size.to_le_bytes().iter().enumerate() {
            res[i+18] = *x;
        }

        res
    }

    pub(super) fn used() -> Self {
        Self { used: true, kind: InodeKind::File, data_size: 0, data_offset: 0, size: 0 }
    }

    pub(super) fn unused() -> Self {
        // perfectly safe here
        unsafe { core::mem::zeroed() }
    }

    /// Gets the offset for index n, ensuring the index is valid
    /// 
    /// this is the offset from block 1, not block 0
    pub(super) fn offset_for_index(index: u64) -> u64 {
        size_of::<Self>() as u64 * index
    }

    pub fn is_used(&self) -> bool {
        self.used
    }
}

impl Inode {
    pub async fn read(
        &self,
        disk: &mut dyn crate::disk::AsyncBlockDevice,
        offset: u64,
        buf: &mut [u8],
    ) -> crate::io::Result<usize> {
        // EOF check
        if offset >= self.size {
            return Ok(0);
        }

        // clamp read length to file size
        let remaining = (self.size - offset) as usize;
        let to_read = core::cmp::min(buf.len(), remaining);

        let disk_offset = self.data_offset + offset;

        crate::disk::read_at(disk, disk_offset, &mut buf[..to_read]).await?;

        Ok(to_read)
    }

    pub fn set_kind(&mut self, kind: InodeKind) {
        self.kind = kind;
    }

    pub unsafe fn set_usage(&mut self, used: bool) {
        self.used = used;
    }

    pub fn kind(&self) -> InodeKind {
        self.kind
    }

    pub fn set_data_offset(&mut self, offset: u64) {
        self.data_offset = offset;
    }

    pub fn set_size(&mut self, size: u64) {
        self.size = size;
    }

    pub fn set_total_data_size(&mut self, size: u64) {
        self.data_size = size;
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn data_size(&self) -> u64 {
        self.data_size
    }

    pub fn data_offset(&self) -> u64 {
        self.data_offset
    }

    /// Writes to the inode directory entries.
    /// 
    /// The format is:
    /// ```text
    /// [name_len: u16][name: bytes of size name_len][inode_index: u64]
    /// ```
    /// 
    /// for each entry
    pub async fn write_as_dir(&mut self, disk: &mut dyn crate::disk::AsyncBlockDevice, entires: &[(&[u8], u64)]) -> io::Result {
        self.write_as_file(
            disk, 
            entires.iter()
                .flat_map(|(name, idx)| {
                    let name_len = name.len() as u16;
                    let mut res = Vec::with_capacity(2 + name.len() + 8);
                    res.extend_from_slice(&name_len.to_le_bytes());
                    res.extend_from_slice(name);
                    res.extend_from_slice(&idx.to_le_bytes());
                    res
                })
                .collect::<Vec<u8>>()
                .as_slice()
        ).await
    }

    /// Directly writes data as a file
    /// 
    /// files are just raw data blobs, so this writes the data directly
    pub async fn write_as_file(&mut self, disk: &mut dyn crate::disk::AsyncBlockDevice, data: &[u8]) -> io::Result {
        if data.len() > self.data_size as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "data exceeds allocated size (did you forget to update allocation data in the inode?)"));
        }

        self.size = data.len() as u64;

        crate::disk::write_at(disk, self.data_offset, data).await
    }

    pub async fn read_dir(
        &self,
        disk: &mut dyn crate::disk::AsyncBlockDevice,
    ) -> io::Result<Vec<(Vec<u8>, u64)>> {
        if self.kind != InodeKind::Directory {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "inode is not a directory",
            ));
        }

        let mut buf = alloc::vec![0u8; self.size as usize];
        let mut read = 0;

        while read < buf.len() {
            let n = self.read(disk, read as u64, &mut buf[read..]).await?;
            if n == 0 {
                break;
            }
            read += n;
        }

        parse_dir_entries(&buf)
    }
}

fn parse_dir_entries(buf: &[u8]) -> io::Result<Vec<(Vec<u8>, u64)>> {
    let mut entries = Vec::new();
    let mut off = 0;

    while off < buf.len() {
        if off + 2 > buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "truncated directory entry (name_len)",
            ));
        }

        let name_len = u16::from_le_bytes([buf[off], buf[off + 1]]) as usize;
        off += 2;

        if off + name_len + 8 > buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "truncated directory entry (name/inode)",
            ));
        }

        let name = buf[off..off + name_len].to_vec();
        off += name_len;

        let inode = u64::from_le_bytes(
            buf[off..off + 8].try_into().unwrap()
        );
        off += 8;

        entries.push((name, inode));
    }

    Ok(entries)
}



#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeKind {
    File,
    Directory
}

impl InodeKind {
    fn from_u8(byte: u8) -> Result<Self, InodeParseErr> {
        match byte {
            0 => Ok(Self::File),
            1 => Ok(Self::Directory),
            invalid => Err(InodeParseErr::InvalidKind(invalid))
        }
    }
}