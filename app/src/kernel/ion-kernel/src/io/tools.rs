//! Tools for [`Read`](super::Read)ing and [`Write`](super::Write)ing

use alloc::{string::String, vec::Vec};

use crate::io::{BufRead, Error, IoSlice, IoSliceMut, Read, Seek, Write};

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
    byte: u8
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

