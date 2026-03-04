//! Generic resource management module
//!
//! This module provides a generic file descriptor (FD) table that can manage
//! any type of resource, not just files. Any type that implements the
//! [`FdDispatch`] trait can be registered with the global resource table.

pub mod fd;

pub use fd::{FdDispatch, OpenFlags, ResourceTable};

