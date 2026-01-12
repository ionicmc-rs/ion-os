use core::fmt;

use crate::io::{Error, ErrorKind, IoSlice, Result, Write};

pub trait AsyncWrite: Write {
    /// Attempt to write some bytes from `buf`.
    /// Returns the number of bytes written, which may be less than `buf.len()`.
    async fn async_write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Flush buffered data. For non-buffered writers, this is a no-op.
    async fn async_flush(&mut self) -> Result<()> {
        Ok(())
    }

    /// Vectored write. Default loops over slices and calls `write`.
    async fn async_write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<usize> {
        let mut total = 0usize;
        for b in bufs {
            let slice = b.as_slice();
            if slice.is_empty() {
                continue;
            }
            let n = self.async_write(slice).await?;
            total += n;
            if n < slice.len() {
                break;
            }
        }
        Ok(total)
    }

    /// Indicates whether `async_write_vectored` is specialized.
    async fn is_async_write_vectored(&self) -> bool {
        false
    }

    /// Write the entire buffer or return an error if unable.
    async fn async_write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            let n = self.async_write(buf).await?;
            if n == 0 {
                return Err(Error::EARLY_EOF); // or your equivalent
            }
            buf = &buf[n..];
        }
        Ok(())
    }

    /// Write all slices completely or return an error.
    async fn async_write_all_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<()> {
        for b in bufs {
            self.async_write_all(b.as_slice()).await?;
        }
        Ok(())
    }

    /// Write formatted text using `core::fmt`.
    /// 
    /// the default implementation returns `WouldBlock` because it is not possible
    async fn async_write_fmt(&mut self, _args: fmt::Arguments<'_>) -> Result<()> {
        Err(Error::new(ErrorKind::WouldBlock, "the type did not provide an implementation for `async_write_fmt`"))
    }

    fn can_write_fmt_async(&self) -> bool {
        false
    }
}