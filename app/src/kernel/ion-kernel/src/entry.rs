//! Main entry point

use crossbeam_queue::ArrayQueue;

#[cfg(feature = "test")]
use super::trivial_assertion;
#[allow(unused)]
use crate::{
    c_lib, hlt_loop, interrupts, lib_alloc, serial_println,
    test::run_tests,
    text,
    thread::{
        self, ThreadBuilder,
        build::TPriority,
        sync::{Mutex, MutexGuard},
    },
};
use crate::{
    asyncio, event::{self, take_kbd_wakers}, interrupts::keyboard::{KEYBOARD, SC_QUEUE}, log::error, scheduler
};

fn handle_kbd() -> ! {
    {
        loop {
            thread::block_until_true(|| {
                SC_QUEUE.lock().get_or_init(|| ArrayQueue::new(100)).len() > 0
            });
            let lock = SC_QUEUE.lock();

            let val = lock.get_or_init(|| ArrayQueue::new(100));

            let ev = match KEYBOARD
                .lock()
                .add_byte(val.pop().expect("array should have a value after wait"))
            {
                Ok(Some(ev)) => ev,
                Err(e) => {
                    error!("invalid keyboard input: {e:?}");
                    continue;
                }
                Ok(None) => {
                    error!("invalid keyboard input: failed to decode");
                    continue;
                }
            };

            for hook in event::take_kbd_once_hooks() {
                hook(ev.clone()) // note: clone of 2 bytes is ok - it is actually ok to copy
            }

            for waker in take_kbd_wakers() {
                waker.wake();
            }

            unsafe { scheduler::wake_kbd() };
        }
    }
}

/// final init.
pub fn kernel_main() {

    // keyboard input
    ThreadBuilder::<!>::new()
        .with_name("keyboard_handler")
        .with_priority(TPriority::High)
        .with_stack(16 * 1024)
        .with_entry(handle_kbd)
        .build();

    ThreadBuilder::new()
        .with_name("keyboard_debugger")
        .with_entry(|| {
            loop {
                let key = asyncio::run_future(asyncio::key_input());
                serial_println!("Key Pressed: {key:?}");
            }
        });

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
