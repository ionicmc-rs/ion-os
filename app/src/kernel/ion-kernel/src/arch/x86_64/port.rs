//! x86_64 port I/O
#![allow(missing_docs, unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]

use core::{arch::asm, marker::PhantomData};


#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags),
    );
    value
}

#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    asm!(
        "in ax, dx",
        out("ax") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags),
    );
    value
}

#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    asm!(
        "in eax, dx",
        out("eax") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags),
    );
    value
}

#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags),
    );
}

#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") value,
        options(nomem, nostack, preserves_flags),
    );
}

#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value,
        options(nomem, nostack, preserves_flags),
    );
}

/// Standard I/O wait using port 0x80.
#[inline]
pub unsafe fn io_wait() {
    asm!(
        "out 0x80, al",
        in("al") 0u8,
        options(nomem, nostack, preserves_flags),
    );
}

// Wrapper type

trait Access {}

trait PortData: Copy {
    unsafe fn write(&self, port: u16);
    unsafe fn read(port: u16) -> Self;
}

impl PortData for u8 {
    unsafe fn read(port: u16) -> Self {
        unsafe { inb(port) }
    }

    unsafe fn write(&self, port: u16) {
        unsafe { outb(port, *self); }
    }
}

impl PortData for u16 {
    unsafe fn read(port: u16) -> Self {
        unsafe { inw(port) }
    }

    unsafe fn write(&self, port: u16) {
        unsafe { outw(port, *self); }
    }
}

impl PortData for u32 {
    unsafe fn read(port: u16) -> Self {
        unsafe { inl(port) }
    }

    unsafe fn write(&self, port: u16) {
        unsafe { outl(port, *self); }
    }
}

pub enum Read      {}
pub enum Write     {}
pub enum ReadWrite {}

impl Access for Read {}

impl Access for Write {}

impl Access for ReadWrite {}

pub struct Port<T: PortData, A: Access> {
    port: u16,
    _pd: PhantomData<T>,
    _pa: PhantomData<A>
}

impl<T: PortData, A: Access> Port<T, A> {
    pub const fn new(port: u16) -> Self {
        Self { port, _pa: PhantomData, _pd: PhantomData }
    }
}

// These traits are for better error messages

#[diagnostic::on_unimplemented(
    message = "This port cannot be read",
    label = "cannot be read",
    note = "this is because the port has `{Self}` access"
)]
trait ReadAccess: Access {}

#[diagnostic::on_unimplemented(
    message = "This port cannot be written to",
    label = "cannot be written to",
    note = "this is because the port has `{Self}` access"
)]
trait WriteAccess: Access {}

impl ReadAccess for Read {}
impl WriteAccess for Write {}

impl ReadAccess for ReadWrite {}
impl WriteAccess for ReadWrite {}

impl<T: PortData, A: ReadAccess> Port<T, A> {
    /// Reads the underlying port
    /// # Safety
    /// reading ports may have side effects which violate memory safety. view the documentation for your port
    /// and determine wether it is truly safe to read.
    pub unsafe fn read(&self) -> T {
        unsafe { T::read(self.port) }
    }
}

impl<T: PortData, A: WriteAccess> Port<T, A> {
    /// Writes to the underlying port
    /// # Safety
    /// writing to ports may have side effects which violate memory safety. view the documentation for your port
    /// and determine wether it is truly safe to write.
    pub unsafe fn write(&self, val: T) {
        unsafe { val.write(self.port); }
    }
}

// convenience functions

impl<T: PortData, A: WriteAccess + ReadAccess> Port<T, A> {
    /// Reads, and then Writes to the port.
    /// # Safety
    /// see [`Port::read`] and [`Port::write`]
    pub unsafe fn read_and_write(&self, val: T) -> T {
        let res = self.read();
        self.write(val);
        res
    }

    /// Reads, then writes if `f` returns true
    /// # Returns
    /// the value, and wether or not the port was written to.
    /// # Safety
    /// same as [`read`](Self::read) and [`write`](Self::write)
    pub unsafe fn read_and_write_if(&self, val: T, predicate: impl FnOnce(T) -> bool) -> (T, bool) {
        let read = self.read();
        let pred = predicate(read);

        if pred {
            self.write(val);
        }

        (read, pred)
    }

    /// Writes, then reads the underlying port.
    /// # Safety
    /// same as [`read`](Self::read) and [`write`](Self::write) 
    pub unsafe fn write_and_read(&self, val: T) -> T {
        self.write(val);
        self.read()
    }
}

// Note to contributors: do NOT implement `Read` and `Write` (from crate::io) here,
// ports are unsafe to act on and thus should not have a safe way to read/write