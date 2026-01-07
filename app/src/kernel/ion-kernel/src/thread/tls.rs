//! tls.rs
//!
//!
//! Invariants:
//! - Only `.tdata` exists
//! - `.tdata` MUST end immediately before the TCB
//! - FS.base points to ThreadSelf (TCB)
//! - TLS variables live at NEGATIVE offsets from FS.base
//!
//! This matches LLVM x86_64 local-exec TLS exactly.

#![allow(unsafe_op_in_unsafe_fn, unused)]

use core::{alloc::Layout, slice};
use alloc::{alloc::dealloc, boxed::Box};
use x86_64::{VirtAddr, align_up, registers::model_specific::FsBase};

use crate::{serial_println, thread};

unsafe extern "C" {
    safe static __tdata_start: u8;
    safe static __tdata_end: u8;
    safe static __tbss_start: u8;
    safe static __tbss_end: u8;
}


/// ABI-visible Thread Control Block
/// FS.base MUST point here
#[repr(C)]
pub struct ThreadSelf {
    /// Required by SysV ABI (points to itself)
    pub self_ptr: *mut ThreadSelf,

    /// TLS backing store (immediately below this struct)
    pub tls_base: *mut u8,
    pub tls_size: usize,

    /// Optional metadata
    pub stack_base: *mut u8,
    pub stack_size: usize,
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


/// Allocate TLS + TCB for one thread.
///
/// Memory layout (low → high):
///
///   [ TLS (.tdata only) ][ ThreadSelf ]
///                              ↑
///                           FS.base
///
/// IMPORTANT:
/// `.tdata` MUST end immediately before the TCB.
/// Nothing may be placed between them.
pub unsafe fn alloc_thread_self(
    stack_base: *mut u8,
    stack_size: usize,
) -> (*mut ThreadSelf, TlsAllocation) {
    serial_println!("allocating TLS");

    // ---- TLS boundaries (authoritative from linker) ----
    let tdata_start = &__tdata_start as *const u8 as usize;
    let tdata_end   = &__tdata_end   as *const u8 as usize;
    let tbss_start  = &__tbss_start  as *const u8 as usize;
    let tbss_end    = &__tbss_end    as *const u8 as usize;

    let tdata_size = tdata_end - tdata_start;
    let tbss_size  = tbss_end  - tbss_start;
    let tls_size   = tdata_size + tbss_size;

    let pad = 16;

    let tcb_size   = core::mem::size_of::<ThreadSelf>();
    let total_size = tls_size + pad + tcb_size;

    
    
    
    serial_println!(
        ".tdata [{:#x}..{:#x}] size = {}",
        tdata_start,
        tdata_end,
        tdata_size,
    );
    serial_println!(
        ".tbss  [{:#x}..{:#x}] size = {}",
        tbss_start,
        tbss_end,
        tbss_size,
    );
    serial_println!("TLS + TCB total size = {}", total_size);

    // ---- Hard safety limits (allocator contract) ----
    assert!(tdata_size <= 256);
    assert!(total_size <= 4096);
    
    
    // ---- Allocate TLS block + TCB ----
    let base = thread::critical_moment_raw(|| {
        let layout = Layout::from_size_align(total_size, 16).unwrap();
        alloc::alloc::alloc(layout)
    });
    
    assert!(!base.is_null());
    
    
    
    // ---- Initialize TLS ----

    // Copy initialized TLS (.tdata)
    core::ptr::copy_nonoverlapping(
        &__tdata_start as *const u8,
        base,
        tdata_size,
    );
    
    unsafe {
        let src = &__tdata_start as *const u8;
        serial_println!(
            "tdata src bytes: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
            *src.add(0),
            *src.add(1),
            *src.add(2),
            *src.add(3),
            *src.add(4),
            *src.add(5),
            *src.add(6),
            *src.add(7),
        );

        let dst = base;
        serial_println!(
            "tdata dst bytes: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
            *dst.add(0),
            *dst.add(1),
            *dst.add(2),
            *dst.add(3),
            *dst.add(4),
            *dst.add(5),
            *dst.add(6),
            *dst.add(7),
        );
    }
    // Zero uninitialized TLS (.tbss)
    core::ptr::write_bytes(
        base.add(tdata_size),
        0,
        tbss_size,
    );

    
    // ---- Place TCB immediately after TLS ----
    let tcb = base.add(tls_size + pad) as *mut ThreadSelf;
    
    core::ptr::write_unaligned(
        tcb,
        ThreadSelf {
            self_ptr:   tcb,
            tls_base:   base,
            tls_size,
            stack_base,
            stack_size,
        },
    );

    serial_println!("TLS base = {:#p}", base);
    serial_println!("TCB @ {:#p}", tcb);
    serial_println!("Padding Range: {base:p}-{:p}", base.add(pad));

    (tcb, TlsAllocation { base, layout: Layout::from_size_align(total_size, 16).unwrap() })
}


/// Install TLS for the currently running thread.
///
/// MUST be called:
/// - before any Rust code touches `#[thread_local]`
/// - before context switch returns
#[inline(always)]
pub unsafe fn install_tls(thread_self: *mut ThreadSelf) {
    FsBase::write(VirtAddr::new(thread_self as u64));
}
