//! Main entry point

use alloc::string::String;
use pc_keyboard::KeyState;

#[cfg(feature = "test")]
use super::trivial_assertion;
use crate::{arch::interrupts::keyboard::{KEYBOARD, SC_QUEUE}, asyncio, boot_info, event, io, log::{self, error}, scheduler};
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
static __TLOCAL: u64 = 0xdeadbeef;

/// final init.
pub fn kernel_main() {
    assert_eq!(__TLOCAL, 0xdeadbeef, "tls is not setup correctly");

    #[cfg(debug_assertions)]
    event::add_key_listener(|ev| {
        if ev.state != KeyState::Up {
            use pc_keyboard::KeyCode;

            match ev.code {
                KeyCode::Backspace => {
                    use crate::text::WRITER;

                    WRITER.lock().backspace();
                }
                KeyCode::Delete => {
                    use crate::text::WRITER;

                    WRITER.lock().delete_row();
                }
                KeyCode::Q => {
                    use crate::{address_space, arch, boot_info, disk::{self, DiskId}, log::debug, process::CURRENT_PROCESS};

                    debug!("DEBUG KEY PRESSED:");
                    debug!("Thread: \n{:#?}", thread::current());
                    debug!("Process ID: {:?}", unsafe { CURRENT_PROCESS });
                    debug!("Context: \n{:#?}", arch::ctx::current());
                    debug!("Boot Info: \n{:#?}", boot_info());
                    debug!("Kernel Address Space: \n{:#?}", unsafe { address_space::kernel_address_space() });

                    if let Some(id) = DiskId::new(0) {
                        use crate::vfs::fs::parse::read_superblock;

                        let dev = disk::get_disk(id);

                        debug!("Disk0 Data:");
                        asyncio::run_future(async {
                            use crate::vfs::fs::parse::{read_alloc_block, read_inode};

                            debug!("Super Block: \n{:#?}", read_superblock(dev).await);
                            debug!("Alloc Block: \n{:#?}", read_alloc_block(dev).await);
                            debug!("Root Inode: \n{:#?}", read_inode(dev, 0).await);
                        });
                        debug!("End of Dump")
                    }
                }
                _ => {
                    use pc_keyboard::DecodedKey;

                    match KEYBOARD.lock().process_keyevent(ev) {
                        Some(DecodedKey::Unicode(c)) => {
                            use crate::text::print;

                            print!("{c}");
                        }
                        Some(DecodedKey::RawKey(k)) => {
                            serial_println!("non decoded key: {k:?}");
                        }
                        None => () // nothing
                    }
                }
            }
        }
    });

    // keyboard input
    ThreadBuilder::<!>::new()
        .with_name("keyboard_handler")
        .with_priority(TPriority::High)
        .with_stack(64 * 1024)
        .with_entry(handle_kbd)
        .build();

    
    let fb_pointer = boot_info().frame_buffer;

    log::debug!("reading {fb_pointer:?}");

    let framebuffer = unsafe { boot_info().frame_buffer.read() }; // leave memory unchanged

    log::debug!("{framebuffer:#?}");


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
        }
    }
}
