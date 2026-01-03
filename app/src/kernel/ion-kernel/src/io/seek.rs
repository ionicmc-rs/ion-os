//! Move cursor position with [`Seek`]

use super::Result;

/// Where to seek from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SeekFrom {
    /// From the start
    Start(u64),
    /// From the current cursor position.
    Current(i64),
    /// From the end.
    /// 
    /// note if a [`Seek`]er is endless it may return [`Error::UNSEEKABLE_INFINITE`](super::Error::UNSEEKABLE_INFINITE)
    End(i64)
}

/// The seek trait
#[allow(clippy::len_without_is_empty)]
pub trait Seek {
    /// Seeks from the argument, and returns a position that can be used with [`SeekFrom::Start`]
    fn seek(&mut self, from: SeekFrom) -> Result<u64>;

    /// rewinds to the beginning
    fn rewind(&mut self) -> Result<()> {
        self.seek(SeekFrom::Start(0)).map(|_| ())
    }

    /// stream length
    fn len(&mut self) -> Result<u64> {
        self.seek(SeekFrom::End(0))
    }

    /// Returns the current position.
    fn current_position(&mut self) -> Result<u64> {
        self.seek(SeekFrom::Current(0))
    }

    /// Seek from current position
    fn seek_relative(&mut self, n: i64) -> Result<()> {
        self.seek(SeekFrom::Current(n)).map(|_| ())
    }
}