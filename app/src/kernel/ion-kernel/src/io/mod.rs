//! I/O operations in Ion OS.
//! 
//! This similar to `std::io` in the fact it provides similar functionality, but the implementation details are quite different.
//! # Features
//! This module provides [`Read`], [`Write`], and [`Seek`], which are the basic functionalities of I/O.
//! 
//! Read:
//! - Read data.
//! - Size hints
//! 
//! Write:
//! - Write data.
//! - Usually implements [`Read`]
//! 
//! Seek:
//! - move the internal cursor.

pub mod read;
pub mod error;
pub mod tools;
pub mod write;
pub mod seek;
pub mod buf_read;

#[allow(unused)]
pub use read::*;
#[allow(unused)]
pub use error::*;
#[allow(unused)]
pub use tools::*;
#[allow(unused)]
pub use write::*;
#[allow(unused)]
pub use seek::*;