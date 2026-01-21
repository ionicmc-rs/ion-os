//! Main entry point


use pc_keyboard::KeyState;

#[cfg(feature = "test")]
use super::trivial_assertion;
use crate::{arch::interrupts::keyboard::{KEYBOARD, SC_QUEUE}, event, log::error, scheduler};
#[allow(unused)]
use crate::{
    c_lib, hlt_loop, arch::interrupts, lib_alloc, serial_println,
    test::run_tests,
    text,
    thread::{
        self, ThreadBuilder,
        build::TPriority,
        sync::{Mutex, MutexGuard},
    },
};

#[allow(static_mut_refs)]
fn handle_kbd() -> ! {
    {
        let sc_q = unsafe { SC_QUEUE.assume_init_ref() };
        loop {
            thread::block_until_true(|| {
                !sc_q.is_empty()
            });

            let sc = if let Some(v) =  sc_q.pop() {
                v
            } else {
                thread::yield_now();
                continue;
            };

            let ev: pc_keyboard::KeyEvent = match KEYBOARD
                .lock()
                .add_byte(sc)
            {
                Ok(Some(ev)) => ev,
                Err(e) => {
                    error!("invalid keyboard input: {e:?}");
                    thread::yield_now();
                    continue;
                }
                Ok(None) => {
                    error!("invalid keyboard input: failed to decode");
                    thread::yield_now();
                    continue;
                }
            };

            event::run_kbd_hooks(ev.clone());
            
            event::run_kbd_once_hooks(ev);

            event::wake_kbd_wakers();

            unsafe { scheduler::wake_kbd() };
        }
    }
}

#[thread_local]
static __TLOCAL: usize = 0;

/// final init.
pub fn kernel_main() {

    assert_eq!(__TLOCAL, 0, "tls is not setup correctly");

    #[cfg(debug_assertions)]
    event::add_key_listener(|ev| {
        if ev.state != KeyState::Up {
            serial_println!("Key Pressed: {:?}", ev)
        }
    });

    // keyboard input
    ThreadBuilder::<!>::new()
        .with_name("keyboard_handler")
        .with_priority(TPriority::High)
        .with_stack(16 * 1024)
        .with_entry(handle_kbd)
        .build();

    cfg_if::cfg_if! {
        if #[cfg(feature = "test")] {
            use crate::stack;
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
                &c_lib::libc::mem::test_malloc,
                // Stack
                &stack::tests::tracked_initial_rsp_is_top,
                &stack::tests::tracked_map_preserves_rsp,
                &stack::tests::tracked_rejects_misaligned_rsp,
                &stack::tests::tracked_rejects_out_of_bounds_rsp,
                &stack::tests::tracked_set_rsp_rejects_invalid,
                &stack::tests::tracked_unchecked_preserves_rsp,
                &stack::tests::tracked_used_remaining_correct,
            ]);
        } else {
            thread::exit_current();
            hlt_loop()
        }
    }
}
