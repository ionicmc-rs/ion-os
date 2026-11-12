//! Main entry point

use alloc::{boxed::Box, string::String, vec::Vec};
use elf::endian::AnyEndian;
use pc_keyboard::{DecodedKey, KeyCode, KeyState};

#[cfg(feature = "test")]
use super::trivial_assertion;
#[allow(unused)]
use crate::{
    arch::interrupts,
    c_lib, hlt_loop, lib_alloc, serial_println,
    test::run_tests,
    text,
    thread::{
        self, ThreadBuilder,
        build::TPriority,
        sync::{Mutex, MutexGuard},
    },
};
use crate::{
    arch::{
        self,
        interrupts::keyboard::{KEYBOARD, SC_QUEUE},
    },
    asyncio, event,
    graphics::{
        self, Color, Pos, Rect, Surface,
        font::{BitmapFont, draw_glyph, draw_text, set_fallback_font},
        with_renderer,
    },
    io::{File, OpenOptions, Read},
    log::{self, error},
    privilege::UserID,
    process::{self, FdHandle, spawn_proc},
    resources::{self, ResourceTable, channel, fd},
    scheduler::{self, current_scheduler},
    thread::Thread,
};

struct TtyCursor {
    col: u32,
    row: u32,
    columns: u32,
    rows: u32,
    glyph_width: u32,
    glyph_height: u32,
    wrap_pending: bool,
    escape_state: u8,
    escape_param: u16,
    dirty: Option<Rect>,
}

impl TtyCursor {
    fn new(width: u32, height: u32, font: &BitmapFont) -> Self {
        let glyph_width = (font.glyph_width as u32).max(1);
        let glyph_height = (font.glyph_height as u32).max(1);

        Self {
            col: 0,
            row: 0,
            columns: (width / glyph_width).max(1),
            rows: (height / glyph_height).max(1),
            glyph_width,
            glyph_height,
            wrap_pending: false,
            escape_state: 0,
            escape_param: 0,
            dirty: None,
        }
    }

    fn mark_dirty(&mut self, rect: Rect) {
        self.dirty = Some(match self.dirty {
            Some(old) => {
                let x = old.x().min(rect.x());
                let y = old.y().min(rect.y());
                let right = old.right().max(rect.right());
                let bottom = old.bottom().max(rect.bottom());
                Rect::new(x, y, right - x, bottom - y)
            }
            None => rect,
        });
    }

    fn take_dirty(&mut self) -> Option<Rect> {
        self.dirty.take()
    }

    fn scroll_if_needed(&mut self, surface: &mut Surface) {
        if self.row < self.rows {
            return;
        }

        let scroll_height = self.glyph_height.min(surface.height);
        let pitch = surface.pitch_bytes as usize;
        let copy_bytes = surface.height.saturating_sub(scroll_height) as usize * pitch;

        unsafe {
            core::ptr::copy(
                surface.base.add(scroll_height as usize * pitch),
                surface.base,
                copy_bytes,
            );
        }

        let clear_y = surface.height - scroll_height;
        surface.fill_rect(
            Rect::new(0, clear_y, surface.width, scroll_height),
            Color::BLACK,
        );
        self.row = self.rows - 1;
        self.mark_dirty(Rect::new(0, 0, surface.width, surface.height));
    }

    fn line_feed(&mut self, surface: &mut Surface) {
        self.col = 0;
        self.wrap_pending = false;
        self.row += 1;
        self.scroll_if_needed(surface);
    }

    fn put_printable(&mut self, surface: &mut Surface, font: &BitmapFont, byte: u8) {
        if self.wrap_pending {
            self.line_feed(surface);
        }

        let byte = if byte >= 0x80 { b'?' } else { byte };
        if byte < b' ' || byte == 0x7f {
            return;
        }

        let x = self.col * self.glyph_width;
        let y = self.row * self.glyph_height;
        let cell = Rect::new(x, y, self.glyph_width, self.glyph_height);
        surface.fill_rect(cell, Color::BLACK);

        if let Some(glyph) = font.glyph(byte) {
            draw_glyph(surface, x, y, font, glyph, Color::WHITE);
        }
        self.mark_dirty(cell);

        if self.col + 1 >= self.columns {
            self.wrap_pending = true;
        } else {
            self.col += 1;
        }
    }

    fn write_bytes(&mut self, surface: &mut Surface, font: &BitmapFont, bytes: &[u8]) {
        for &byte in bytes {
            match self.escape_state {
                1 => {
                    self.escape_state = if byte == b'[' { 2 } else { 0 };
                    self.escape_param = 0;
                    continue;
                }
                2 if byte.is_ascii_digit() => {
                    self.escape_param = self
                        .escape_param
                        .saturating_mul(10)
                        .saturating_add((byte - b'0') as u16);
                    continue;
                }
                2 => {
                    match byte {
                        b'J' if self.escape_param == 2 => {
                            surface.clear(Color::BLACK);
                            self.col = 0;
                            self.row = 0;
                            self.wrap_pending = false;
                            self.mark_dirty(Rect::new(0, 0, surface.width, surface.height));
                        }
                        b'H' => {
                            self.col = 0;
                            self.row = 0;
                            self.wrap_pending = false;
                        }
                        _ => {}
                    }
                    self.escape_state = 0;
                    self.escape_param = 0;
                    continue;
                }
                _ => {}
            }

            match byte {
                0x1b => self.escape_state = 1,
                b'\n' => self.line_feed(surface),
                b'\r' => {
                    self.col = 0;
                    self.wrap_pending = false;
                }
                b'\t' => {
                    let spaces = 4 - (self.col % 4);
                    for _ in 0..spaces {
                        self.put_printable(surface, font, b' ');
                    }
                }
                0x08 => {
                    self.wrap_pending = false;
                    if self.col > 0 {
                        self.col -= 1;
                    } else if self.row > 0 {
                        self.row -= 1;
                        self.col = self.columns - 1;
                    }
                    let cell = Rect::new(
                        self.col * self.glyph_width,
                        self.row * self.glyph_height,
                        self.glyph_width,
                        self.glyph_height,
                    );
                    surface.fill_rect(cell, Color::BLACK);
                    self.mark_dirty(cell);
                }
                _ => self.put_printable(surface, font, byte),
            }
        }
    }
}

fn handle_kbd() -> ! {
    {
        let sc_q = unsafe { SC_QUEUE.assume_init_ref() };
        loop {
            thread::block_until_true(|| !sc_q.is_empty());

            let Some(sc) = sc_q.pop() else {
                continue;
            };

            let ev: pc_keyboard::KeyEvent = match KEYBOARD.lock().add_byte(sc) {
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
                    thread::spawn(
                        || {
                            use crate::{
                                address_space, arch, boot_info,
                                disk::{self, DiskId},
                                log::debug,
                                process::current_process,
                            };

                            debug!("DEBUG KEY PRESSED:");
                            debug!("Thread: \n{:#?}", thread::current());
                            debug!("Process ID: {:?}", current_process());
                            debug!("Context: \n{:#?}", arch::ctx::current());
                            debug!("Boot Info: \n{:#?}", boot_info());
                            debug!("Kernel Address Space: \n{:#?}", unsafe {
                                address_space::kernel_address_space()
                            });

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
                        },
                        64 * 1024,
                    );
                }
                _ => {}
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
    let screen_width = fb.width;
    let screen_height = fb.height;

    graphics::renderer::init(fb);

    with_renderer(|r| {
        draw_text(
            &mut r.back,
            Pos::new(20, 20),
            &font,
            "[ion-os] starting ion os...\n[ion-os] this process may be lengthy - please be patient...",
            Color::WHITE,
        );

        r.present();
    });

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
    let wstdin = table.alloc(FdHandle(wstdin)).unwrap();

    let pid = asyncio::run_future(process::exec(
        UserID::os(),
        "@/core/ish",
        &["@/core/ish"],
        None,
        table,
    ))
    .unwrap();

    {
        event::add_key_listener(move |ev| {
            let bytes: &[u8] = match KEYBOARD.lock().process_keyevent(ev.clone()) {
                Some(DecodedKey::Unicode(c)) => &[c as u8],

                Some(DecodedKey::RawKey(code)) => {
                    let res: &[u8] = match code {
                        KeyCode::Return => b"\n" as &[u8],
                        KeyCode::Backspace => b"\x08" as &[u8],
                        KeyCode::Tab => b"\t" as &[u8],
                        KeyCode::ArrowUp => b"\x1b[A" as &[u8],
                        KeyCode::ArrowDown => b"\x1b[B" as &[u8],
                        KeyCode::ArrowLeft => b"\x1b[D" as &[u8],
                        KeyCode::ArrowRight => b"\x1b[C" as &[u8],
                        _ => &[],
                    };
                    res
                }

                None => &[],
            };
            if ev.state != KeyState::Up {
                process::write_proc_fd(pid, wstdin, bytes);
                log::debug!("wrote: {bytes:?}");
            }
        });

        // new thread for temporary TTY shell
        thread::spawn(
            move || {
                log::info!("starting TTY shell for ish (pid {pid:?})");
                with_renderer(|r| {
                    r.clear(Color::BLACK);
                });

                let mut cursor = TtyCursor::new(screen_width, screen_height, &font);
                loop {
                    let mut buf_stdout = [0u8; 256];
                    let mut buf_stderr = [0u8; 256];

                    let stdout_len = process::read_proc_fd(pid, rstdout, &mut buf_stdout)
                        .unwrap_or(0)
                        .min(buf_stdout.len() as u64) as usize;
                    let stderr_len = process::read_proc_fd(pid, rstderr, &mut buf_stderr)
                        .unwrap_or(0)
                        .min(buf_stderr.len() as u64) as usize;

                    if stdout_len == 0 && stderr_len == 0 {
                        thread::yield_now();
                        continue;
                    }

                    with_renderer(|r| {
                        cursor.write_bytes(&mut r.back, &font, &buf_stdout[..stdout_len]);
                        cursor.write_bytes(&mut r.back, &font, &buf_stderr[..stderr_len]);
                        if let Some(dirty) = cursor.take_dirty() {
                            r.present_region(dirty);
                        }
                    });
                }
            },
            16 * 1024,
        );
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
