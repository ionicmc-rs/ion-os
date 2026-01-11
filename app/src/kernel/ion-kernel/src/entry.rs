//! Main entry point

#[allow(unused)]
use crate::{
    c_lib, hlt_loop, interrupts, lib_alloc, serial_println, test::run_tests, text, thread::{self, ThreadBuilder, build::TPriority, sync::{Mutex, MutexGuard}}, trivial_assertion
};

/// final init.
pub extern "C" fn kernel_main() -> ! {
    cfg_if::cfg_if! {
        if #[cfg(feature = "test")] {
            run_tests(&[
                // all tests go here

                // control, test for tests
                &trivial_assertion,
                // interrupts
                &interrupts::test::test_breakpoint,
                // VGA
                &text::test_println_output,
                // Alloc
                &lib_alloc::tests::test_large_alloc,
                &lib_alloc::tests::test_freed_mem_used,
                &lib_alloc::tests::test_alloc_tools,
                // C Lib / LibC
                &c_lib::libc::mem::test_malloc
            ]);
        } else {
            thread::exit_current();
            hlt_loop()
        }
    }
}
