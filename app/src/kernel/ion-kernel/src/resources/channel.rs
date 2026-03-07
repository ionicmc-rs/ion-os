//! Ring-buffer backed channel resources.
//!
//! The channel endpoints implement [`FdDispatch`], so they can be opened in
//! the same resource tables as files and other kernel resources.

use alloc::{boxed::Box, sync::Arc, vec, vec::Vec};
use core::sync::atomic::{AtomicU64, Ordering};

use crate::io::{self, ErrorKind};

use super::fd::{FdDispatch, OpenFlags};

static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct RingBuffer {
    data: Vec<u8>,
    head: usize,
    len: usize,
}

impl RingBuffer {
    fn new(capacity: usize) -> io::Result<Self> {
        if capacity == 0 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "ring channel capacity must be greater than zero",
            ));
        }

        Ok(Self {
            data: vec![0; capacity],
            head: 0,
            len: 0,
        })
    }

    fn capacity(&self) -> usize {
        self.data.len()
    }

    fn push(&mut self, byte: u8) {
        if self.len < self.capacity() {
            let tail = (self.head + self.len) % self.capacity();
            self.data[tail] = byte;
            self.len += 1;
            return;
        }

        // Full ring: overwrite oldest data.
        self.data[self.head] = byte;
        self.head = (self.head + 1) % self.capacity();
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }

        let byte = self.data[self.head];
        self.head = (self.head + 1) % self.capacity();
        self.len -= 1;
        Some(byte)
    }

    fn read_into(&mut self, out: &mut [u8]) -> usize {
        let mut read = 0;
        while read < out.len() {
            let Some(byte) = self.pop() else {
                break;
            };
            out[read] = byte;
            read += 1;
        }
        read
    }

    fn write_from(&mut self, input: &[u8]) -> usize {
        for &byte in input {
            self.push(byte);
        }
        input.len()
    }
}

#[derive(Debug)]
struct ChannelCore {
    ring: RingBuffer,
}

/// Access mode for a channel endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMode {
    Read,
    Write,
    ReadWrite,
}

impl ChannelMode {
    fn flags(self) -> OpenFlags {
        match self {
            Self::Read => OpenFlags::READ | OpenFlags::NONBLOCK,
            Self::Write => OpenFlags::WRITE | OpenFlags::NONBLOCK,
            Self::ReadWrite => OpenFlags::READ | OpenFlags::WRITE | OpenFlags::NONBLOCK,
        }
    }

    fn is_readable(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    fn is_writable(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

/// A channel endpoint implementing [`FdDispatch`].
#[derive(Debug)]
pub struct ChannelEndpoint {
    channel_id: u64,
    core: Arc<spin::Mutex<ChannelCore>>,
    mode: ChannelMode,
    cursor: u64,
}

impl ChannelEndpoint {
    fn new(channel_id: u64, core: Arc<spin::Mutex<ChannelCore>>, mode: ChannelMode) -> Self {
        Self {
            channel_id,
            core,
            mode,
            cursor: 0,
        }
    }
}

/// Create a ring-buffer channel with dedicated read/write endpoints.
pub fn ring_channel(capacity: usize) -> io::Result<(ChannelEndpoint, ChannelEndpoint)> {
    let channel_id = NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed);
    let core = Arc::new(spin::Mutex::new(ChannelCore {
        ring: RingBuffer::new(capacity)?,
    }));

    Ok((
        ChannelEndpoint::new(channel_id, Arc::clone(&core), ChannelMode::Read),
        ChannelEndpoint::new(channel_id, core, ChannelMode::Write),
    ))
}

/// Create a ring-buffer channel as boxed dispatch resources.
pub fn ring_channel_boxed(
    capacity: usize,
) -> io::Result<(Box<dyn FdDispatch>, Box<dyn FdDispatch>)> {
    let (reader, writer) = ring_channel(capacity)?;
    Ok((Box::new(reader), Box::new(writer)))
}

impl FdDispatch for ChannelEndpoint {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<u64> {
        if !self.mode.is_readable() {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "channel endpoint is not readable",
            ));
        }

        if buf.is_empty() {
            return Ok(0);
        }

        let mut core = self.core.lock();
        let read = core.ring.read_into(buf);

        self.cursor = self.cursor.saturating_add(read as u64);
        Ok(read as u64)
    }

    fn write(&mut self, data: &[u8]) -> io::Result<u64> {
        if !self.mode.is_writable() {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "channel endpoint is not writable",
            ));
        }

        if data.is_empty() {
            return Ok(0);
        }

        let mut core = self.core.lock();
        let written = core.ring.write_from(data);
        self.cursor = self.cursor.saturating_add(written as u64);
        Ok(written as u64)
    }

    fn seek(&mut self, _pos: u64) -> io::Result<u64> {
        Err(ErrorKind::NotSeekable.into())
    }

    fn seek_relative(&mut self, _delta: i64) -> io::Result<u64> {
        Err(ErrorKind::NotSeekable.into())
    }

    fn rewind(&mut self) {}

    fn tell(&self) -> io::Result<u64> {
        Ok(self.cursor)
    }

    fn flags(&self) -> OpenFlags {
        self.mode.flags()
    }

    fn is_readable(&self) -> bool {
        self.mode.is_readable()
    }

    fn is_writable(&self) -> bool {
        self.mode.is_writable()
    }

    fn truncate(&mut self, _size: u64) -> io::Result<()> {
        Err(io::Error::new(
            ErrorKind::Unsupported,
            "truncate is unsupported for channel endpoints",
        ))
    }

    fn close(self: Box<Self>) -> io::Result<()> {
        Ok(())
    }

    fn metadata(&self) -> io::Result<(u64, u64, OpenFlags)> {
        Ok((self.channel_id, self.cursor, self.flags()))
    }
}
