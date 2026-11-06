//! Generic resource management module
//!
//! This module provides a generic file descriptor (FD) table that can manage
//! any type of resource, not just files. Any type that implements the
//! [`FdDispatch`] trait can be registered with the global resource table.

pub mod channel;
pub mod fd;

pub use channel::{ChannelEndpoint, ChannelMode, ring_channel, ring_channel_boxed};
pub use channel::{
    ChannelEndpoint as StreamEndpoint, ring_channel as ring_stream,
    ring_channel_boxed as ring_stream_boxed,
};
pub use fd::{FdDispatch, OpenFlags, ResourceTable};
