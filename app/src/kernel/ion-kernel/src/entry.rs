//! Main entry point

use alloc::{string::String};

use crate::{asyncio, io::Cursor};
#[allow(unused)]
use crate::{
    c_lib, hlt_loop, interrupts, lib_alloc, serial_println, test::run_tests, text, thread::{self, ThreadBuilder, build::TPriority, sync::{Mutex, MutexGuard}}, 
};
#[cfg(feature = "test")]
use super::trivial_assertion;

/// final init.
pub fn kernel_main() {
    let fut = async {
        use crate::io::concurrent::read::AsyncRead;
        let binding = String::from("Hi, world!");
        let mut curs = Cursor::new(binding.as_bytes());
        let mut s = Default::default();
        match curs.async_read_to_string(&mut s).await {
            Ok(v) if v != binding.len() => {
                panic!("partial read");
            }
            Ok(_) => (),
            Err(e) => panic!("{e}")
        }
        s
    };

    let fut_res = asyncio::run_future(fut);

    serial_println!("future: {fut_res}");

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
