//! Buffered Reading
use alloc::string::String;
use alloc::vec::Vec;

use crate::io::{Error, ErrorKind, Result};
use crate::io::Read;

/// Ion OS equivalent of `std::io::BufRead`.
pub trait BufRead: Read {
    /// Returns the contents of the internal buffer.
    fn fill_buf(&mut self) -> Result<&[u8]>;

    /// Discards `amt` bytes from the buffer.
    fn consume(&mut self, amt: usize);

    /// Reads until the delimiter byte is found.
    fn read_until(&mut self, delim: u8, buf: &mut Vec<u8>) -> Result<usize> {
        let mut total = 0;
        loop {
            let available = self.fill_buf()?;
            if available.is_empty() {
                return Ok(total);
            }
            if let Some(pos) = available.iter().position(|&b| b == delim) {
                buf.extend_from_slice(&available[..=pos]);
                self.consume(pos + 1);
                total += pos + 1;
                return Ok(total);
            }
            buf.extend_from_slice(available);
            let len = available.len();
            self.consume(len);
            total += len;
        }
    }

    /// Reads a line into the provided `String`.
    fn read_line(&mut self, buf: &mut String) -> Result<usize> {
        let mut tmp = Vec::new();
        let n = self.read_until(b'\n', &mut tmp)?;
        match core::str::from_utf8(&tmp) {
            Ok(s) => buf.push_str(s),
            Err(e) => return Err(Error::new(ErrorKind::InvalidData, e)),
        }
        Ok(n)
    }

    /// Reads exactly enough bytes to fill `buf`.
    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            let n = self.read(buf)?;
            if n == 0 {
                return Err(Error::EARLY_EOF);
            }
            buf = &mut buf[n..];
        }
        Ok(())
    }

    /// Returns true if there is still data left in the buffer or source.
    fn has_data_left(&mut self) -> Result<bool> {
        let available = self.fill_buf()?;
        Ok(!available.is_empty())
    }

    /// Returns an iterator over lines of this reader.
    fn lines(self) -> Lines<Self>
    where
        Self: Sized,
    {
        Lines { buf: self }
    }

    /// Returns an iterator over slices split by a delimiter.
    fn split(self, delim: u8) -> Split<Self>
    where
        Self: Sized,
    {
        Split { buf: self, delim }
    }
}

/// Iterator over lines.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Lines<B: BufRead> {
    buf: B,
}

impl<B: BufRead> Iterator for Lines<B> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut s = String::new();
        match self.buf.read_line(&mut s) {
            Ok(0) => None,
            Ok(_) => Some(Ok(s)),
            Err(e) => Some(Err(e)),
        }
    }
}

/// Iterator over slices split by a delimiter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Split<B: BufRead> {
    buf: B,
    delim: u8,
}
impl<B: BufRead> Iterator for Split<B> {
    type Item = Result<Vec<u8>>;
    
    fn next(&mut self) -> Option<Self::Item> {
        let mut v = Vec::new();
        match self.buf.read_until(self.delim, &mut v) {
            Ok(0) => None,
            Ok(_) => Some(Ok(v)),
            Err(e) => Some(Err(e)),
        }
    }
}