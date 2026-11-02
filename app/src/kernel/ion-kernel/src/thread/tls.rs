//! tls.rs
//!
//! Invariants (x86_64 local-exec TLS):
//! - FS.base points to ThreadSelf (TCB)
//! - TLS ends exactly at FS.base
//! - TLS variables live at NEGATIVE offsets from FS.base

#![allow(unsafe_op_in_unsafe_fn)]

use core::{alloc::Layout, cmp, panic::PanicInfo};

use alloc::{boxed::Box, string::String, vec::Vec};
use linked_list_allocator::align_up;

use crate::{arch, log, process::ProcessID, serial_println, thread};

unsafe extern "C" {
    static __tdata_start: u8;
    static __tdata_end: u8;
    static __tbss_start: u8;
    static __tbss_end: u8;

    // Authoritative TLS bounds (from linker)
    static __tls_start: u8;
    static __tls_end: u8;
}

#[repr(C)]
pub struct ThreadSelf {
    pub self_ptr: *mut ThreadSelf,

    // TLS backing store (immediately below TCB)
    pub tls_base: *mut u8,
    pub tls_size: usize,

    pub stack_base: *mut u8,
    pub stack_size: usize,

    pub errno: i32,
    pub panicking: bool,
    pub panic_hooks: Vec<Box<dyn FnOnce(&PanicInfo) + Send>>,
    pub name: String,
    pub is_kill_safe: bool,
    pub pid: Option<ProcessID>,
}

#[derive(Debug)]
pub struct TlsAllocation {
    base: *mut u8,
    layout: Layout,
}

impl Drop for TlsAllocation {
    fn drop(&mut self) {
        unsafe {
            alloc::alloc::dealloc(self.base, self.layout);
        }
    }
}

pub unsafe fn alloc_thread_self(
    stack_base: *mut u8,
    stack_size: usize,
) -> (*mut ThreadSelf, TlsAllocation) {

    // --- TLS size (AUTHORITATIVE, from linker) ---
    let tls_size = (&__tls_end as *const u8 as usize) - (&__tls_start as *const u8 as usize);

    // Optional sanity: make sure linker TLS >= section TLS
    let tdata_size = (&__tdata_end as *const u8 as usize) - (&__tdata_start as *const u8 as usize);

    let tbss_size = (&__tbss_end as *const u8 as usize) - (&__tbss_start as *const u8 as usize);

    debug_assert!(tls_size >= tdata_size + tbss_size);

    let tcb_size = core::mem::size_of::<ThreadSelf>();

    // --- ABI alignments ---
    let tls_align = 16; // x86_64 local-exec (LLVM/GCC)
    let tcb_align = core::mem::align_of::<ThreadSelf>();
    let alloc_align = cmp::max(tls_align, tcb_align);

    // Round TLS block to TLS alignment
    let tls_rounded = align_up(tls_size as *mut u8, tls_align) as usize;

    let total_size = tls_rounded + tcb_size;
    assert!(total_size <= 4096);

    let layout = Layout::from_size_align(total_size, alloc_align).unwrap();

    let base = thread::critical_moment_raw(|| alloc::alloc::alloc(layout));
    assert!(!base.is_null());

    // --- TCB at top ---
    let tcb = base.add(tls_rounded) as *mut ThreadSelf;

    // --- TLS immediately below TCB ---
    let tls_start = (tcb as *mut u8).sub(tls_rounded);

    // --- Initialize TLS ---
    core::ptr::copy_nonoverlapping(&__tdata_start as *const u8, tls_start, tdata_size);

    core::ptr::write_bytes(tls_start.add(tdata_size), 0, tbss_size);

    // --- Initialize TCB ---
    core::ptr::write(
        tcb,
        ThreadSelf {
            self_ptr: tcb,
            tls_base: tls_start,
            tls_size,
            stack_base,
            stack_size,
            
            errno: 0,
            panicking: false,
            panic_hooks: Vec::new(),
            name: String::from("<unnamed>"),
            is_kill_safe: true,
            pid: None,
        },
    );

    // --- Sanity checks ---
    debug_assert_eq!(tls_start.add(tls_rounded), tcb as *mut u8);
    debug_assert_eq!((tcb as usize) % tcb_align, 0);

    (tcb, TlsAllocation { base, layout })
}

#[inline(always)]
pub unsafe fn install_tls(thread_self: *mut ThreadSelf) {
    arch::tls::write_ptr(thread_self.cast());
}

#[track_caller]
pub fn tcb() -> &'static mut ThreadSelf {
    let ptr = arch::tls::read_ptr().cast_mut();
    assert!(!ptr.is_null());
    unsafe { &mut *ptr.cast::<ThreadSelf>() }
}
