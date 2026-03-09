//! Main entry point

use alloc::{boxed::Box, string::String, vec::Vec};
use elf::endian::AnyEndian;
use pc_keyboard::{DecodedKey, KeyCode, KeyState};

#[cfg(feature = "test")]
use super::trivial_assertion;
use crate::{arch::{self, interrupts::keyboard::{KEYBOARD, SC_QUEUE, SCANCODES_PUSHED}}, asyncio, event, graphics::{self, Color, Pos, font::{BitmapFont, draw_text, set_fallback_font}, with_renderer}, io::OpenOptions, log::{self, error}, privilege::UserID, process::{self, FdHandle, spawn_proc}, resources::{self, ResourceTable, channel, fd}, scheduler::{self, SCHEDULER}, smp, thread::Thread};
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

fn handle_kbd() -> ! {
    {
        let sc_q = unsafe { SC_QUEUE.assume_init_ref() };
        loop {
            thread::block_until_true(|| {
                SCANCODES_PUSHED.load(core::sync::atomic::Ordering::SeqCst) != 0
            });

            SCANCODES_PUSHED.fetch_sub(1, core::sync::atomic::Ordering::SeqCst);
            
            let sc = sc_q.try_lock().map(|mut q| q.pop()).flatten().expect("scancode queue was empty after being woken");

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



/// final init.
pub fn kernel_main() {
    #[cfg(debug_assertions)]
    event::add_key_listener(|ev| {
        if ev.state != KeyState::Up {
            use pc_keyboard::KeyCode;

            match ev.code {
                KeyCode::Backspace => {
                    // use crate::text::WRITER;

                    // WRITER.lock().backspace();
                }
                KeyCode::Delete => {
                    // use crate::text::WRITER;

                    // WRITER.lock().delete_row();
                }
                KeyCode::Q => {
                    use crate::{address_space, arch, boot_info, disk::{self, DiskId}, log::debug, process::current_process};

                    debug!("DEBUG KEY PRESSED:");
                    debug!("Thread: \n{:#?}", thread::current());
                    debug!("Process ID: {:?}", current_process());
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
                            // use crate::text::print;

                            log::debug!("key: {c}");
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


    let font = asyncio::run_future(async {
        let file = OpenOptions::new()
            .read(true)
            .open("@/fonts/lat9w-16.psf")
            .await
            .expect("The fallback font file has not been loaded");

        let font_data = file
            .read_all()
            .await
            .expect("could not read the fallback font file");

        font_data
    });

    // we will leak this font because it must always be loaded

    let font = font.leak();

    let font = BitmapFont::parse_psf1(font).expect("corrupted font data in fallback font");

    set_fallback_font(font);

    let fb = graphics::framebuffer();

    graphics::renderer::init(fb);

    with_renderer(|r| {        
        draw_text(&mut r.back, Pos::new(20, 20), &font, "[ion-os] starting ion os...\n[ion-os] this process may be lengthy - please be patient...", Color::WHITE);

        r.present();
    });

    // SMP

    log::debug!("attempting to start APs (cores)");

    smp::start();

    log::debug!("finished starting APs!");

    // init (CPU0)

    let mut table = ResourceTable::new();

    let (rstdout, wstdout) = channel::ring_channel_boxed(4096).unwrap();
    let (rstderr, wstderr) = channel::ring_channel_boxed(4096).unwrap();
    let (rstdin, wstdin) = channel::ring_channel_boxed(4096).unwrap();

    let rsdtout = fd::alloc(rstdout).unwrap();
    let wstodut = fd::alloc(wstdout).unwrap();
    let rstderr = fd::alloc(rstderr).unwrap();
    let wstderr = fd::alloc(wstderr).unwrap();
    let rstdin = fd::alloc(rstdin).unwrap();
    let wstdin = fd::alloc(wstdin).unwrap();

    let rstdout = table.alloc(FdHandle(rsdtout)).unwrap();
    table.alloc(FdHandle(wstodut)).unwrap();
    let rstderr = table.alloc(FdHandle(rstderr)).unwrap();
    table.alloc(FdHandle(wstderr)).unwrap();
    table.alloc(FdHandle(rstdin)).unwrap();
    table.alloc(FdHandle(wstdin)).unwrap();

    let mut pid = asyncio::run_future(process::exec(
        UserID::os(), 
        "@/core/ish", 
        &["@/core/ish"], 
        None,
        table
    )).unwrap();

    {
        event::add_key_listener(move |ev| {
            let bytes: &[u8] = match KEYBOARD.lock().process_keyevent(ev.clone()) {
                Some(DecodedKey::Unicode(c)) => {
                    &[c as u8]
                }

                Some(DecodedKey::RawKey(code)) => {
                    let res: &[u8] = match code {
                        KeyCode::Return => b"\n" as &[u8],
                        KeyCode::Backspace => b"\x08" as &[u8],
                        KeyCode::Tab => b"\t" as &[u8],
                        KeyCode::ArrowUp => b"\x1b[A" as &[u8],
                        KeyCode::ArrowDown => b"\x1b[B" as &[u8],
                        KeyCode::ArrowLeft => b"\x1b[D" as &[u8],
                        KeyCode::ArrowRight => b"\x1b[C" as &[u8],
                        _ => {&[]}
                    };
                    res
                }

                None => {&[]}
            };
            if ev.state != KeyState::Up {
                process::write_proc_fd(pid, wstdin, bytes);
                log::debug!("wrote: {bytes:?}");
            }
        });

        // new thread for temporary TTY shell

        thread::spawn(move || {
            with_renderer(|r| {
                r.clear(Color::BLACK);
            });
            let total_col = 80;
            let total_lines = 10;
            let mut row = 0;
            let mut col = 0;
            loop {
                let mut buf_stdout = [0u8];
                let mut buf_stderr = [0u8];
    
                process::read_proc_fd(pid, rstdout, &mut buf_stdout);
                process::read_proc_fd(pid, rstderr, &mut buf_stderr);
                with_renderer(|r| {

                    let mut changed = false;

                    if buf_stdout != [0u8] {
                        changed = true;

                        if buf_stdout[0] == b'\n' {
                            col = 0;
                            row += 1;
                        } else {
                            draw_text(&mut r.back, Pos::new(col * 10, row * 10), &font, str::from_utf8(&buf_stdout).unwrap(), Color::WHITE);
                            col += 1;
                            if col >= total_col {
                                col = 0;
                                row += 1;
                            }
                        }
                    }
                    if buf_stderr != [0u8] {
                        changed = true;

                        if buf_stderr[0] == b'\n' {
                            col = 0;
                            row += 1;
                        } else {
                            draw_text(&mut r.back, Pos::new(col * 10, row * 10), &font, str::from_utf8(&buf_stderr).unwrap(), Color::WHITE);
                            col += 1;
                            if col >= total_col {
                                col = 0;
                                row += 1;
                            }
                        }
                    }

                    if row >= total_lines {
                        todo!();
                    }

                    if changed {
                        r.present();
                    }
                });
            }
        }, 16 * 1024);
    }

    // log::debug!("queued thread for test ELF");
    
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
                // &text::test_println_output,
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
