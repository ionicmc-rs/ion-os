//! [`Read`]ing data.
//! 
//! *for more info, see `std::io` (add link)*
//! 
//! Reads allow you to get the internal data in complex in structures, they can fail with IO Errors, or pass with the internal
//! bytes.

use alloc::{string::String, vec::Vec};

use crate::io::{Error, ErrorKind};

use super::{Result};

// IoSlice

/// An IO Slice used for reads, mutable.
// TODO: better docs and API
#[derive(Debug)]
pub struct IoSliceMut<'a> {
    inner: &'a mut [u8],
}

impl<'a> IoSliceMut<'a> {
    /// Returns a new [`IoSliceMut`] with the passed in buffer.
    pub fn new(buf: &'a mut [u8]) -> IoSliceMut<'a> {
        IoSliceMut { inner: buf }
    }

    /// returns the mutable reference
    pub fn get_mut(&mut self) -> &mut [u8] {
        self.inner
    }

    /// returns the length of the slice.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// returns wether the slice is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// Read Trait

/// Returns if the result of a [`read`](Read::read) is an EOF ([`Ok(0)`](Ok)).
/// 
/// # Example
/// ```
/// if is_eof(reader.read()) {
///     panic!("Early EOF");
/// }
/// ```
pub fn is_eof(res: &Result<usize>) -> bool {
    matches!(res, Ok(0))
}

/// A chain of two readers
#[derive(Debug, Clone)]
pub struct Chain<R1: Read, R2: Read> {
    first: R1,
    second: R2
}

impl<R1: Read, R2: Read> Read for Chain<R1, R2> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let first = self.first.read(buf);
        if is_eof(&first) {
            self.second.read(buf)
        } else {
            first
        }
    }
}

/// A reader adapter that reads at most `limit` bytes from the inner reader.
#[derive(Debug, Clone)]
pub struct Take<R> {
    inner: R,
    limit: u64,
}

impl<R> Take<R> {
    /// Create a new `Take` wrapping `inner` with a byte limit.
    pub fn new(inner: R, limit: u64) -> Self {
        Self { inner, limit }
    }

    /// Get the remaining limit.
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Set a new limit.
    pub fn set_limit(&mut self, limit: u64) {
        self.limit = limit;
    }

    /// Access the inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// returns a ref to the reader
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// returns a mutable ref to the reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
}

impl<R: Read> Read for Take<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.limit == 0 {
            return Ok(0); // EOF
        }

        // Clamp requested length to remaining limit
        let max = core::cmp::min(buf.len() as u64, self.limit) as usize;
        let n = self.inner.read(&mut buf[..max])?;
        self.limit -= n as u64;
        Ok(n)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        if self.limit == 0 {
            return Ok(0);
        }

        // Clamp total length across slices
        let mut total_len = 0usize;
        for b in bufs.iter_mut() {
            total_len += b.get_mut().len();
            if total_len as u64 >= self.limit {
                break;
            }
        }

        let max = core::cmp::min(total_len as u64, self.limit) as usize;
        // Construct a temporary slice view limited to `max`
        let mut tmp = [0u8; 1024]; // stack buffer for small reads
        let n = self.inner.read(&mut tmp[..max])?;
        // Scatter into bufs
        let mut remaining = &tmp[..n];
        let mut written = 0usize;
        for b in bufs.iter_mut() {
            let dst = b.get_mut();
            if remaining.is_empty() {
                break;
            }
            let m = core::cmp::min(dst.len(), remaining.len());
            dst[..m].copy_from_slice(&remaining[..m]);
            remaining = &remaining[m..];
            written += m;
        }

        self.limit -= written as u64;
        Ok(written)
    }

    fn is_read_vectored(&self) -> bool {
        true
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result {
        if buf.len() as u64 > self.limit {
            return Err(Error::EARLY_EOF);
        }
        self.inner.read_exact(buf)?;
        self.limit -= buf.len() as u64;
        Ok(())
    }
}
/// Readable data.
/// 
/// The trait provides methods for reading data as bytes. with the one required method: [`read`](Read::read).
/// 
/// note that [`read`](Read::read) may involve allocation.
/// 
/// note that for [`Seek`](super::Seek)ers, the same cursor (should) be used, so if you need contents twice, call `rewind()` 
pub trait Read {
    /// reads data into the buffer, returning the number of bytes read.
    /// 
    /// readers may have incomplete reads, in which case, [`ErrorKind::Interrupted`](super::ErrorKind::Interrupted) should be returned.
    /// 
    /// if this function returns `Ok(0)`, that means EOF has been reached, and that the reader is complete.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    /// Read into multiple buffers (scatter read).
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        // Default: just call `read` on the first non-empty slice
        for buf in bufs {
            if !buf.is_empty() {
                return self.read(buf.get_mut());
            }
        }
        Ok(0)
    }

    /// Whether this reader has an efficient `read_vectored`.
    fn is_read_vectored(&self) -> bool {
        false
    }

    /// Reads exactly the number of bytes required to fill `buf`, returning on error or EOF.
    /// 
    /// This function makes no assumptions on the reader or the buffer, call sparingly.
    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(ref e) if matches!(e.kind(), Some(ErrorKind::Interrupted)) => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() { Err(Error::EARLY_EOF) } else { Ok(()) }
    }

    /// Reads the until an EOF ([`Ok(0)`](Ok))
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        if self.is_endless() {
            return Err(Error::UNREADABLE_INFINITE);
        }
        let mut ibuf = [0u8; 1024];
        let mut total = 0usize;
        loop {
            let n = self.read(&mut ibuf)?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&ibuf[..n]);
            total += n;
        }
        Ok(total)
    }

    /// Reads the entire reader to the buffer, as a [`String`].
    /// 
    /// see [`read_to_end`](Read::read_to_end)
    fn read_to_string(&mut self, buf: &mut String) -> Result<usize> {
        let mut vec = alloc::vec![];
        let read = self.read_to_end(&mut vec)?;
        *buf = String::from_utf8(vec).map_err(|e| {
            Error::new(ErrorKind::InvalidData, e)
        })?;
        Ok(read)
    }

    /// chains the two readers
    fn chain<R: Read>(self, next: R) -> Chain<Self, R> 
    where
        Self: Sized
    {
        Chain { first: self, second: next }
    }

    /// Wether or not this reader never returns EOF.
    fn is_endless(&self) -> bool {
        false
    }

    /// optional size hint to the reader
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
/// Default buffer size
pub const DEFAULT_BUF_SIZE: usize = 1024;
