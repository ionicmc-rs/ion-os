use alloc::{string::String, vec::Vec};

use crate::io::{BufRead, Error, ErrorKind, Result, concurrent::read::AsyncRead};

pub trait AsyncBufRead: BufRead + AsyncRead {
    /// Returns the contents of the internal buffer.
    async fn async_fill_buf(&mut self) -> Result<&[u8]>;

    /// Discards `amt` bytes from the buffer.
    async fn async_consume(&mut self, amt: usize);

    /// Reads until the delimiter byte is found.
    async fn async_read_until(&mut self, delim: u8, buf: &mut Vec<u8>) -> Result<usize> {
        let mut total = 0;
        loop {
            let available = self.async_fill_buf().await?;
            if available.is_empty() {
                return Ok(total);
            }
            if let Some(pos) = available.iter().position(|&b| b == delim) {
                buf.extend_from_slice(&available[..=pos]);
                self.async_consume(pos + 1).await;
                total += pos + 1;
                return Ok(total);
            }
            buf.extend_from_slice(available);
            let len = available.len();
            self.async_consume(len).await;
            total += len;
        }
    }

    /// Reads a line into the provided `String`.
    async fn async_read_line(&mut self, buf: &mut String) -> Result<usize> {
        let mut tmp = Vec::new();
        let n = self.async_read_until(b'\n', &mut tmp).await?;
        match core::str::from_utf8(&tmp) {
            Ok(s) => buf.push_str(s),
            Err(e) => return Err(Error::new(ErrorKind::InvalidData, e)),
        }
        Ok(n)
    }

    /// Reads exactly enough bytes to fill `buf`.
    async fn async_read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            let n = self.async_read(buf).await?;
            if n == 0 {
                return Err(Error::EARLY_EOF);
            }
            buf = &mut buf[n..];
        }
        Ok(())
    }

    /// Returns true if there is still data left in the buffer or source.
    async fn async_has_data_left(&mut self) -> Result<bool> {
        let available = self.async_fill_buf().await?;
        Ok(!available.is_empty())
    }
}
