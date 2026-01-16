use crate::io::{Result, Seek, SeekFrom};

/// The seek trait
#[allow(clippy::len_without_is_empty)]
pub trait AsyncSeek: Seek {
    /// Seeks from the argument, and returns a position that can be used with [`SeekFrom::Start`]
    async fn async_seek(&mut self, from: SeekFrom) -> Result<u64>;

    /// rewinds to the beginning
    async fn async_rewind(&mut self) -> Result<()> {
        self.async_seek(SeekFrom::Start(0)).await.map(|_| ())
    }

    /// stream length
    async fn async_len(&mut self) -> Result<u64> {
        self.async_seek(SeekFrom::End(0)).await
    }

    /// Returns the current position.
    async fn async_current_position(&mut self) -> Result<u64> {
        self.async_seek(SeekFrom::Current(0)).await
    }

    /// Seek from current position
    async fn async_seek_relative(&mut self, n: i64) -> Result<()> {
        self.async_seek(SeekFrom::Current(n)).await.map(|_| ())
    }
}
