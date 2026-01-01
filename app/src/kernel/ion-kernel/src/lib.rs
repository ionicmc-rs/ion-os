//! The Ion Kernel
//! 
//! This library contains the entirety of the Ion OS Kernel, which includes Rust Lang Items, such as
//! the `#[panic_handler]` (found in [`panic`]), and os operations such as printing using 
//! [`text`]_mode

#![no_std]
// required for tests
#![no_main]
#![allow(internal_features)]
#![deny(
    warnings,
    missing_docs,
    missing_abi,
    missing_debug_implementations
)]
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
    debug_closure_helpers
)]

use cfg_if::cfg_if;

use crate::{c_lib::{BootInfoC, bit_flags::BitFlags}, log::{info, warn}, text::println};


/// module for panicking
pub mod panic;
/// module for link with the C Kernel Entry.
pub mod c_lib;
/// module for printing to the VGA Buffer.
pub mod text;
pub mod test;
/// module for initializing the Kernel.
pub mod init;
/// module for handling interrupts.
pub mod interrupts;
/// module for logging
pub mod log;
/// serial printing
pub mod serial;
/// Memory and Paging Operations
pub mod mem;
// /// Allocation tools
// pub mod lib_alloc;


cfg_if::cfg_if! {
    if #[cfg(feature = "test")] {
        use crate::test::{TestInfo, TestResult, test_assert_eq, run_tests};

        fn trivial_assertion(_: TestInfo) -> TestResult {
            test_assert_eq!(1, 1, "Huh?")
        }
    }
}

macro feature_missing {
    ($feature:ident) => {
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
        warn!("Your Virtual Machine does not support VMX, It is recommended to switch over to one that does.");
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

// TODO: Move these to `c_lib`
/// The entry to the kernel
/// 
/// Do Not call - at all.
/// # Safety
/// The boot_info ptr must point to valid data.
#[unsafe(no_mangle)]
#[cfg_attr(feature = "test", expect(unreachable_code, reason = "panics always occur at end of tests"))]
pub unsafe extern "C" fn rust_kernel_entry(boot_info: *const BootInfoC) -> ! {

    serial_println!("\nWelcome User of QEMU! Thank you for using Ion OS");

    // initialize first to catch page faults/double faults
    match init::init() {
        Ok(()) => info!("Initialized Ion OS."),
        Err(e) => {
            println!("Handling Err...\n{e:#?}");
            if e.is_fatal() {
                panic!("Error while initializing Ion OS: {e}")
            }
        }
    }

    
    // Read the pointer
    // Safety: the pointer is guaranteed always to be valid, as this is passed in from C. other calls
    // Violate the unsafe precondition.
    let boot_info = unsafe { boot_info.read() };

    let boot_info = boot_info.into_inner().unwrap_or_else(|e| {
        panic!("Invalid Boot Info:\n {e:#?}")
    }).into_rust();
    
    // TODO: load boot data here into global var
    
    // x86_64::instructions::interrupts::int3();
    
    assert_cpuid_features(boot_info.cpuid_edx, boot_info.cpuid_ecx);

    serial_println!("Initialized");

    cfg_if! {
        if #[cfg(feature = "test")] {
            run_tests(&[
                // all tests go here
                &trivial_assertion,
                &interrupts::test::test_breakpoint,
                &text::test_println_output
            ]);
            panic!("End of tests; you can now exit.");
        } else {
            println!("Not Testing");
        }
    }


    hlt_loop()
}

/// Halts the CPU forever.
/// 
/// Only used in panics, and the Rust Kernel Entry.
pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}