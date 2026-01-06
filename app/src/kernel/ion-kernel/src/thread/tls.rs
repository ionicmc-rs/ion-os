//! tls.rs
//!
//! x86_64 Thread-Local Storage (SysV ABI, Variant II)
//! GS.base points to ThreadSelf (TCB)
//! TLS variables live at NEGATIVE offsets from GS.base
//!
//! REQUIREMENTS:
//! - static kernel
//! - target JSON: "tls-model": "local-exec"
//! - linker provides __tdata_start, __tdata_end, __tbss_end
//! - NO swapgs anywhere

#![allow(unsafe_op_in_unsafe_fn)]

use core::alloc::Layout;
use x86_64::{VirtAddr, registers::model_specific::{FsBase, GsBase}};

unsafe extern "C" {
    static __tdata_start: u8;
    static __tdata_end: u8;
    static __tbss_end: u8;
}

/// ABI-visible Thread Control Block
/// GS.base MUST point here
#[repr(C)]
pub struct ThreadSelf {
    /// Required by SysV ABI
    pub self_ptr: *mut ThreadSelf,

    /// TLS backing store (below this struct)
    pub tls_base: *mut u8,
    pub tls_size: usize,

    /// Optional but useful
    pub stack_base: *mut u8,
    pub stack_size: usize,
}

/// Allocate TLS + TCB for one thread.
///
/// Memory layout (low → high):
///   [ TLS (.tdata + .tbss) ][ ThreadSelf ]
///                                ↑
///                              GS.base
pub unsafe fn alloc_thread_self(
    stack_base: *mut u8,
    stack_size: usize,
) -> *mut ThreadSelf {
    let tdata_start = &__tdata_start as *const u8 as usize;
    let tdata_end   = &__tdata_end   as *const u8 as usize;
    let tbss_end    = &__tbss_end    as *const u8 as usize;

    let tdata_size = tdata_end - tdata_start;
    let tbss_size  = tbss_end - tdata_end;
    let tls_size   = tdata_size + tbss_size;

    let tcb_size   = core::mem::size_of::<ThreadSelf>();
    let total_size = tls_size + tcb_size;

    let layout = Layout::from_size_align(total_size, 16).unwrap();
    let base = alloc::alloc::alloc_zeroed(layout);
    
    // pin the tls data.

    // Copy initialized TLS (.tdata)
    core::ptr::copy_nonoverlapping(
        &__tdata_start,
        base,
        tdata_size,
    );

    // TCB is AFTER TLS
    let tcb = base.add(tls_size) as *mut ThreadSelf;

    (*tcb).self_ptr   = tcb;
    (*tcb).tls_base   = base;
    (*tcb).tls_size   = tls_size;
    (*tcb).stack_base = stack_base;
    (*tcb).stack_size = stack_size;

    tcb
}

/// Install TLS for the currently running thread.
/// Call this in your scheduler BEFORE switching context.
#[inline(always)]
pub unsafe fn install_tls(thread_self: *mut ThreadSelf) {
    _ = FsBase::read();
    GsBase::write(VirtAddr::new(thread_self as u64));
}
