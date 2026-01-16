use alloc::{string::String, vec::Vec};

use crate::io::{Error, ErrorKind, IoSliceMut, Read, Result};

pub trait AsyncRead: Read {
    /// reads data into the buffer, returning the number of bytes read.
    ///
    /// readers may have incomplete reads, in which case, [`ErrorKind::Interrupted`](super::ErrorKind::Interrupted) should be returned.
    ///
    /// if this function returns `Ok(0)`, that means EOF has been reached, and that the reader is complete.
    async fn async_read(&mut self, buf: &mut [u8]) -> Result<usize>;
    /// Read into multiple buffers (scatter read).
    async fn async_read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        // Default: just call `read` on the first non-empty slice
        for buf in bufs {
            if !buf.is_empty() {
                return self.async_read(buf.get_mut()).await;
            }
        }
        Ok(0)
    }

    /// Whether this reader has an efficient `async_read_vectored`.
    fn is_async_read_vectored(&self) -> bool {
        false
    }

    /// Reads exactly the number of bytes required to fill `buf`, returning on error or EOF.
    ///
    /// This function makes no assumptions on the reader or the buffer, call sparingly.
    async fn async_read_exact(&mut self, mut buf: &mut [u8]) -> Result {
        while !buf.is_empty() {
            match self.async_read(buf).await {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(ref e) if matches!(e.kind(), Some(ErrorKind::Interrupted)) => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(Error::EARLY_EOF)
        } else {
            Ok(())
        }
    }

    /// Reads the until an EOF ([`Ok(0)`](Ok))
    async fn async_read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        if self.is_endless() {
            return Err(Error::UNREADABLE_INFINITE);
        }
        let mut ibuf = [0u8; 1024];
        let mut total = 0usize;
        loop {
            let n = self.async_read(&mut ibuf).await?;
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
    async fn async_read_to_string(&mut self, buf: &mut String) -> Result<usize> {
        let mut vec = alloc::vec![];
        let read = self.async_read_to_end(&mut vec).await?;
        *buf = String::from_utf8(vec).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(read)
    }
}
