//! Tools for [`Read`](super::Read)ing and [`Write`](super::Write)ing

use core::cmp::min;

use alloc::{string::String, vec::Vec};

use crate::io::{
    BufRead, Error, ErrorKind, IoSlice, IoSliceMut, Read, Seek, SeekFrom, Write,
    concurrent::{read::AsyncRead, seek::AsyncSeek, write::AsyncWrite},
};

/// An empty stream.
///
/// This means all read operations return `Ok(0)`, and some fail due to lack of data.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Default, Hash)]
pub struct Empty;

impl Read for Empty {
    fn read(&mut self, _buf: &mut [u8]) -> super::Result<usize> {
        Ok(0)
    }
    fn read_vectored(&mut self, _bufs: &mut [super::IoSliceMut<'_>]) -> super::Result<usize> {
        Ok(0)
    }
    fn read_exact(&mut self, mut _buf: &mut [u8]) -> super::Result {
        // we are not a stream of 0s, we are empty, so this fails.

        // we cannot fill buf, so we fail.
        Err(Error::EARLY_EOF)
    }
    fn size_hint(&self) -> Option<usize> {
        Some(0)
    }
    fn read_to_end(&mut self, _buf: &mut alloc::vec::Vec<u8>) -> super::Result<usize> {
        Ok(0)
    }
    fn read_to_string(&mut self, _buf: &mut alloc::string::String) -> super::Result<usize> {
        Ok(0)
    }
}

impl Write for Empty {
    fn write(&mut self, _buf: &[u8]) -> super::Result<usize> {
        // discarded. unlike sink however, this returns EOF
        Ok(0)
    }
    fn write_all(&mut self, mut _buf: &[u8]) -> super::Result<()> {
        // we are not a stream of 0s, we are empty, so this fails.

        Err(Error::WRITE_TO_EMPTY)
    }
    fn write_all_vectored(&mut self, _bufs: &[super::IoSlice<'_>]) -> super::Result<()> {
        Ok(())
    }
    fn write_fmt(&mut self, _args: core::fmt::Arguments<'_>) -> super::Result<()> {
        Ok(())
    }
    fn write_vectored(&mut self, _bufs: &[super::IoSlice<'_>]) -> super::Result<usize> {
        Ok(0)
    }
}

impl Seek for Empty {
    fn seek(&mut self, _: super::SeekFrom) -> Result<u64> {
        Ok(0) // No data to seek
    }
}

impl BufRead for Empty {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        Ok(&[])
    }

    fn consume(&mut self, _amt: usize) {}
}

/// Emulates writing, but discards the value.
///
/// this does not implement [`Read`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Sink;

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> super::Result<usize> {
        Ok(buf.len()) // emulate writing.
    }
    fn write_all(&mut self, mut _buf: &[u8]) -> super::Result<()> {
        Ok(())
    }
    fn write_all_vectored(&mut self, _bufs: &[super::IoSlice<'_>]) -> super::Result<()> {
        Ok(())
    }

    fn write_fmt(&mut self, _args: core::fmt::Arguments<'_>) -> super::Result<()> {
        Ok(())
    }
}

impl Seek for Sink {
    fn seek(&mut self, _: super::SeekFrom) -> Result<u64> {
        Ok(0) // this is fine because sink discards all data.
    }
}

use super::Result;

impl<R: Read + ?Sized> Read for &mut R {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        (**self).read(buf)
    }

    #[inline]
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        (**self).read_vectored(bufs)
    }

    #[inline]
    fn is_read_vectored(&self) -> bool {
        (**self).is_read_vectored()
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        (**self).read_to_end(buf)
    }

    #[inline]
    fn read_to_string(&mut self, buf: &mut String) -> Result<usize> {
        (**self).read_to_string(buf)
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        (**self).read_exact(buf)
    }

    fn is_endless(&self) -> bool {
        (**self).is_endless()
    }

    fn size_hint(&self) -> Option<usize> {
        (**self).size_hint()
    }
}
impl<W: Write + ?Sized> Write for &mut W {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        (**self).write(buf)
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<usize> {
        (**self).write_vectored(bufs)
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        (**self).is_write_vectored()
    }

    #[inline]
    fn flush(&mut self) -> Result<()> {
        (**self).flush()
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        (**self).write_all(buf)
    }

    #[inline]
    fn write_all_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<()> {
        (**self).write_all_vectored(bufs)
    }

    #[inline]
    fn write_fmt(&mut self, fmt: core::fmt::Arguments<'_>) -> Result<()> {
        (**self).write_fmt(fmt)
    }
}

impl<S: Seek + ?Sized> Seek for &mut S {
    fn seek(&mut self, from: super::SeekFrom) -> Result<u64> {
        (**self).seek(from)
    }
    fn current_position(&mut self) -> Result<u64> {
        (**self).current_position()
    }
    fn len(&mut self) -> Result<u64> {
        (**self).len()
    }
    fn rewind(&mut self) -> Result<()> {
        (**self).rewind()
    }
    fn seek_relative(&mut self, n: i64) -> Result<()> {
        (**self).seek_relative(n)
    }
}

/// Repeats the same byte.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Default, Hash)]
pub struct Repeat {
    byte: u8,
}

impl Repeat {
    /// Creates a new [`Repeat`] that repeats `byte`
    pub const fn new(byte: u8) -> Self {
        Self { byte }
    }

    /// creates a never-ending stream of zeros
    pub const fn zeros() -> Self {
        Self::new(0)
    }

    /// [`zeros`](Self::zeros) as a const.
    pub const ZEROS: Self = Self::zeros();
}

impl Read for Repeat {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        for byte in buf.iter_mut() {
            *byte = self.byte;
        }
        Ok(buf.len())
    }
}

impl Seek for Repeat {
    fn seek(&mut self, _: super::SeekFrom) -> Result<u64> {
        Ok(0) // this is fine as the reader is identical everywhere.
    }
}

// CURSOR

/// A cursor over an in-memory buffer.
///
/// Behaves like `std::io::Cursor`.
#[derive(Debug, Clone)]
pub struct Cursor<T> {
    inner: T,
    pos: usize,
}

impl<T> Cursor<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, pos: 0 }
    }

    pub fn position(&self) -> u64 {
        self.pos as u64
    }

    pub fn set_position(&mut self, pos: u64) {
        self.pos = pos as usize;
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl Read for Cursor<&[u8]> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.pos >= self.inner.len() {
            return Ok(0); // EOF
        }

        let n = min(buf.len(), self.inner.len() - self.pos);
        buf[..n].copy_from_slice(&self.inner[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

impl Read for Cursor<Vec<u8>> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.pos >= self.inner.len() {
            return Ok(0); // EOF
        }

        let n = min(buf.len(), self.inner.len() - self.pos);
        buf[..n].copy_from_slice(&self.inner[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

impl Write for Cursor<Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let end = self.pos + buf.len();

        if end > self.inner.len() {
            self.inner.resize(end, 0);
        }

        self.inner[self.pos..end].copy_from_slice(buf);
        self.pos = end;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<T: AsRef<[u8]>> Seek for Cursor<T> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let len = self.inner.as_ref().len() as i64;
        let cur = self.pos as i64;

        let new = match pos {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => len + n,
            SeekFrom::Current(n) => cur + n,
        };

        if new < 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "seek is out of bounds"));
        }

        self.pos = new as usize;
        Ok(self.pos as u64)
    }
}

impl<T: AsRef<[u8]>> AsyncSeek for Cursor<T> {
    async fn async_seek(&mut self, from: SeekFrom) -> Result<u64> {
        self.seek(from)
    }
}

impl AsyncRead for Cursor<Vec<u8>> {
    async fn async_read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.read(buf)
    }
}

impl AsyncWrite for Cursor<Vec<u8>> {
    async fn async_write(&mut self, buf: &[u8]) -> Result<usize> {
        self.write(buf)
    }

    async fn async_flush(&mut self) -> Result<()> {
        self.flush()
    }
}

impl AsyncRead for Cursor<&[u8]> {
    async fn async_read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.read(buf)
    }
}
