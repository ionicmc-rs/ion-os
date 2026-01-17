//! Main entry point


#[cfg(feature = "test")]
use super::trivial_assertion;
use crate::{event::{self}, interrupts::keyboard::{KEYBOARD, SC_QUEUE}, log::error, stack};
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

#[allow(static_mut_refs)]
fn handle_kbd() -> ! {
    {
        let sc_q = unsafe { SC_QUEUE.assume_init_ref() };
        loop {
            thread::block_until_true(|| {
                !sc_q.is_empty()
            });
            serial_println!("scancode has new keys");

            let sc = if let Some(v) =  sc_q.pop() {
                v
            } else {
                serial_println!("nothing");
                thread::yield_now();
                continue;
            };

            serial_println!("adding byte: ");
            let ev = match KEYBOARD
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

            serial_println!("running hooks");
            event::run_kbd_hooks(ev.clone());
            
            serial_println!("running once hooks");
            event::run_kbd_once_hooks(ev);

            serial_println!("running wakers");
            event::wake_kbd_wakers();

            // serial_println!("waking keyboard waiters");
            // unsafe { scheduler::wake_kbd() };
        }
    }
}

/// final init.
pub fn kernel_main() {
    // keyboard input
    ThreadBuilder::<!>::new()
        .with_name("keyboard_handler")
        .with_priority(TPriority::High)
        .with_stack(8 * 4096)
        .with_entry(handle_kbd)
        .build();
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
