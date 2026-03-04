//! The Ion Kernel
//!
//! This library contains the entirety of the Ion OS Kernel, which includes Rust Lang Items, such as
//! the `#[panic_handler]` (found in [`panic`]), and os operations such as printing using
//! [`text`]_mode

#![no_std]
// required for tests
#![no_main]
#![allow(internal_features)]
#![allow(incomplete_features)]
#![allow(static_mut_refs)]
// #![deny(warnings, missing_docs, missing_abi, missing_debug_implementations)]
#![feature(
    lang_items,
    decl_macro,
    panic_can_unwind,
    custom_test_frameworks,
    try_trait_v2,
    const_trait_impl,
    const_range,
    const_destruct,
    abi_x86_interrupt,
    debug_closure_helpers,
    c_size_t,
    layout_for_ptr,
    allocator_api,
    lazy_type_alias,
    ptr_metadata,
    thread_local,
    const_convert,
    never_type,
    formatting_options
)]
#![test_runner(test::run_tests)] // this is unused but silences errors.

extern crate alloc;

use core::mem::MaybeUninit;

use crate::{
    c_lib::{BootInfoC, bit_flags::BitFlags, libc},
    entry::kernel_main,
    io::Error,
    log::{info, trace, warn},
    thread::ThreadBuilder,
};

pub mod asyncio;
pub mod c_lib;
pub mod entry;
/// tools for registering event listeners
pub mod event;
pub mod init;
pub mod io;
pub mod lib_alloc;
pub mod log;
pub mod panic;
pub mod scheduler;
pub mod serial;
pub mod resources;
pub mod test;
pub mod text;
pub mod thread;
pub mod stack;
pub mod arch;
pub mod address_space;
pub mod privilege;
pub mod process;
pub mod disk;
pub mod vfs;
pub mod graphics;
pub mod mmio;

cfg_if::cfg_if! {
    if #[cfg(feature = "test")] {
        use crate::test::{TestInfo};

        fn trivial_assertion(mut inf: TestInfo) {
            inf.should_panic();

            assert_eq!(1, 2, "well, i expected this.")
        }
    }
}

macro feature_missing {
    ($feature:ident) => {
        libc::set_errno(7);
        panic!("The feature `{}` is disabled, but is required for Ion OS.\n\nCaused By:\n    The System does not meet the minimum requirements.", stringify!($feature));
    },
    ($feature:ident, optional) => {
        $crate::log::warn!("The feature `{}` is disabled, but is strongly recommended.", stringify!($feature));
    }
}

#[allow(unused)]
fn assert_cpuid_features(edx: BitFlags, ecx: BitFlags) {
    // edx
    if !edx.read_flag(0) {
        feature_missing!(FPU);
    }

    if !edx.read_flag(5) {
        feature_missing!(MSR);
    }

    if !edx.read_flag(6) {
        feature_missing!(PAE);
    }

    if !edx.read_flag(8) {
        feature_missing!(CX8);
    }

    if !edx.read_flag(9) {
        feature_missing!(APIC);
    }

    if !edx.read_flag(15) {
        feature_missing!(CMOV);
    }

    if !edx.read_flag(24) {
        feature_missing!(FXSR);
    }

    // optionals
    if !edx.read_flag(3) {
        feature_missing!(PSE, optional);
    }

    if !edx.read_flag(4) {
        feature_missing!(TSC, optional);
    }

    if !edx.read_flag(25) || !edx.read_flag(26) {
        feature_missing!(SSE_GENERAL, optional);
    }

    // ecx

    if !ecx.read_flag(13) {
        feature_missing!(CX16);
    }

    // optionals
    if !ecx.read_flag(23) {
        feature_missing!(POPCNT, optional);
    }

    if !ecx.read_flag(27) {
        feature_missing!(OSXSAVE, optional);
    }

    if !ecx.read_flag(5) && cfg!(debug_assertions) {
        warn!(
            "Your Virtual Machine does not support VMX, It is recommended to switch over to one that does."
        );
    }

    if !ecx.read_flag(17) {
        feature_missing!(PCID, optional);
    }

    if !ecx.read_flag(21) {
        feature_missing!(x2APIC, optional);
    }

    if !ecx.read_flag(26) {
        feature_missing!(XSAVE, optional);
    }

    if !ecx.read_flag(28) {
        feature_missing!(AVX, optional);
    }

    if !ecx.read_flag(0) || !ecx.read_flag(19) || !ecx.read_flag(20) || !ecx.read_flag(9) {
        feature_missing!(SSE_ADDITIONAL, optional);
    }
}

static mut BOOT_INFO: MaybeUninit<c_lib::BootInfo> = MaybeUninit::uninit();

#[allow(static_mut_refs)]
pub fn boot_info() -> &'static c_lib::BootInfo {
    unsafe { BOOT_INFO.assume_init_ref() } // this function must not be called before storage
}

/// The entry to the kernel
///
/// Do Not call - at all.
/// # Safety
/// The boot_info ptr must point to valid data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_kernel_entry(boot_info: *const BootInfoC) -> ! {
    serial_println!("\nWelcome User of QEMU! Thank you for using Ion OS");

    // Read the pointer
    // Safety: the pointer is guaranteed always to be valid, as this is passed in from C. other calls
    // Violate the unsafe precondition.
    let boot_info = unsafe { boot_info.read() };

    let boot_info = boot_info.into_inner();

    let boot_info = boot_info
        .unwrap_or_else(|_| {
            let err = Error::last_os_err();
            libc::set_errno(6);
            panic!("Invalid Boot Info: {err}")
        })
        .into_rust();
    
    #[allow(static_mut_refs)]
    unsafe { BOOT_INFO.write(boot_info.clone()) };

    assert_cpuid_features(boot_info.cpuid_edx, boot_info.cpuid_ecx);

    // FIXME(asm): this tag is currently always corrupt
    let _ptr = unsafe { boot_info.multiboot_info.into_inner().as_ref().unwrap() };

    match init::init(boot_info) {
        Ok(()) => {
            info!("Initialized Ion OS basics");
        }
        Err(e) => {
            trace!("Handling Init Err: {e}");
            if e.is_fatal() {
                panic!("Error while initializing Ion OS: {e}")
            }
        }
    }

    // this is the main thread.
    log::debug!("initializing threads");
    thread::init();
    log::debug!("making main thread");
    let builder = ThreadBuilder::new()
        .with_name("main")
        .with_entry(kernel_main)
        .with_stack(64 * 1024);

    log::debug!("building main thread");

    builder.build();

    // from this point onwards - execution is passed to interrupt handlers and whatever threads we have now (and whatever threads we make.)
    log::info!("starting threads");
    unsafe { thread::start() };
    log::error!("return to the boot stack is prohibited");
    hlt_loop();
}

/// Halts the CPU forever.
///
/// Only used in panics, and the Rust Kernel Entry.
pub fn hlt_loop() -> ! {
    arch::cpu::halt_loop()
}
