//! Module for [`Write`]ing  data

use core::fmt;
use crate::io::{Error, Result};

/// Ion OS equivalent of `std::io::Write`.
pub trait Write {
    /// Attempt to write some bytes from `buf`.
    /// Returns the number of bytes written, which may be less than `buf.len()`.
    fn write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Flush buffered data. For non-buffered writers, this is a no-op.
    fn flush(&mut self) -> Result<()> { Ok(()) }

    /// Vectored write. Default loops over slices and calls `write`.
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<usize> {
        let mut total = 0usize;
        for b in bufs {
            let slice = b.as_slice();
            if slice.is_empty() { continue; }
            let n = self.write(slice)?;
            total += n;
            if n < slice.len() { break; }
        }
        Ok(total)
    }

    /// Indicates whether `write_vectored` is specialized.
    fn is_write_vectored(&self) -> bool { false }

    /// Write the entire buffer or return an error if unable.
    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            let n = self.write(buf)?;
            if n == 0 {
                return Err(Error::EARLY_EOF); // or your equivalent
            }
            buf = &buf[n..];
        }
        Ok(())
    }

    /// Write all slices completely or return an error.
    fn write_all_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<()> {
        for b in bufs {
            self.write_all(b.as_slice())?;
        }
        Ok(())
    }

    /// Write formatted text using `core::fmt`.
    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<()> {
        // Create a shim which translates a `Write` to a `fmt::Write` and saves off
        // I/O errors, instead of discarding them.
        struct Adapter<'a, T: ?Sized + 'a> {
            inner: &'a mut T,
            error: Result<()>,
        }

        impl<T: Write + ?Sized> fmt::Write for Adapter<'_, T> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                match self.inner.write_all(s.as_bytes()) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        self.error = Err(e);
                        Err(fmt::Error)
                    }
                }
            }
        }

        let mut output = Adapter { inner: self, error: Ok(()) };
        match fmt::write(&mut output, args) {
            Ok(()) => Ok(()),
            Err(..) => {
                // Check whether the error came from the underlying `Write`.
                if output.error.is_err() {
                    output.error
                } else {
                    // This shouldn't happen: the underlying stream did not error,
                    // but somehow the formatter still errored?
                    panic!(
                        "a formatting trait implementation returned an error when the underlying stream did not"
                    );
                }
            }
        }
    }
}

/// Simple IoSlice wrapper for vectored writes.
#[derive(Debug, Clone, Copy)]
pub struct IoSlice<'a> {
    inner: &'a [u8],
}
impl<'a> IoSlice<'a> {
    /// Creates a new IO slice
    pub fn new(buf: &'a [u8]) -> Self { Self { inner: buf } }
    /// gets a mutable reference to the IO slice
    pub fn as_slice(&self) -> &[u8] { self.inner }
}