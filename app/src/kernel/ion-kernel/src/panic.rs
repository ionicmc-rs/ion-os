//! panicking.
use core::{cell::Cell, mem, panic::PanicInfo, sync::atomic::compiler_fence};

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::{
    arch, asyncio, c_lib::libc::{self, get_errno, perror}, graphics::{self, Color as GColor, Pos, font::{BitmapFont, draw_text_from_panic, try_get_fallback_font}, with_renderer}, hlt_loop, io::{self, Error, OpenOptions, const_err}, log, scheduler, serial_println, thread::{self, tls::tcb}
};

/// Sets the function run at the end of panics.
///
/// note: this does not replace the panic function, but it runs alongside it at the end.
///
/// this is usually useful to poison mutexes while we don't have unwinding.
pub fn add_hook(f: impl FnOnce(&PanicInfo) + Send + 'static) {
    if thread::current().is_none() {
        panic!("Attempted to set a panic hook from the boot stack.");
    }
    (&mut tcb().panic_hooks).push(Box::new(f) as Box<dyn FnOnce(&PanicInfo) + Send + 'static>);
}

/// takes the current hooks, leaving nothing in its place.
pub fn take_hooks() -> Vec<Box<dyn FnOnce(&PanicInfo) + Send + 'static>> {
    if thread::current().is_none() {
        panic!("Attempted to take a panic hook from the boot stack.");
    }
    mem::take(&mut tcb().panic_hooks)
}


#[panic_handler]
fn panic(info: &PanicInfo) -> ! {

    // unsafe {
    //     core::arch::asm!(
    //         "mov rsp, {}",
    //         in(reg) PANIC_STACK.as_ptr()
    //     )
    // }

    // arch::cpu::full_fence(core::sync::atomic::Ordering::SeqCst);

    if arch::tls::read_ptr().is_null() {
        // panic before thread init - unsafe to use panic thread
        panic_raw(info);
    }

    panic_thread(info);
}

/// panics the current thread.
pub fn panic_thread(info: &PanicInfo) -> ! {
    if thread::panicking() {
        serial_println!("(immediate abort) {info}");
        hlt_loop()
    }
    unsafe { thread::build::set_panic() };
    let loc = info.location().unwrap();
    let message = info.message();
    let tname = thread::name();
    let tid = thread::id();
    match info.can_unwind() {
        true => serial_println!("(unwind) thread `{tname}` ({tid:?}) panicked at {loc}"),
        false => serial_println!("(abort) thread `{tname}` ({tid:?}) panicked at {loc}"),
    }

    serial_println!("{message}");

    let c = c"=> Last THREAD Error";
    perror(c.as_ptr().cast_mut());

    // This is the old VGA buffer printing.
    // it's not needed anymore

    // let mut lock = match WRITER.try_lock() {
    //     Some(v) => v,
    //     None => {
    //         for hook in take_hooks() {
    //             hook()
    //         }
    //         unsafe { scheduler::panic_current() };
    //     }
    // };

    // use core::fmt::Write;
    // lock.set_color(Color::LightCyan, Color::Black);
    // let _ = match info.can_unwind() {
    //     true => writeln!(lock, "(unwind) thread `{tname}` panicked at {loc}"),
    //     false => writeln!(lock, "(abort) thread `{tname}` panicked at {loc}"),
    // };
    // lock.set_color(Color::White, Color::Black);

    // let _ = writeln!(lock, "{message}");

    // lock.set_color(Color::LightCyan, Color::Black);
    // let err = Error::last_os_err();
    // let _ = writeln!(lock, "=> Last THREAD Error: {err}");

    if let Some(font) = try_get_fallback_font() {
        let last_os_error = Error::last_os_err();
        let panic_text = alloc::format!("[bug] thread {tname} panicked at {loc}\n")
            + &alloc::format!("{message}\n")
            + &alloc::format!("[err code] {last_os_error}\n\n")
            + &alloc::format!("This is (likely) a bug, you can report it at [BUG REPORT LINK]");

        with_renderer(|r| {
            r.clear(GColor::BLACK);
            r.present();
            draw_text_from_panic(Pos::new(20, 20), &font, &panic_text, GColor::WHITE);
        });
    } else {
        log::error!("fallback font not loaded - cancelling draw")
    }

    // everything before must stay before, and everything after must stay after.
    compiler_fence(core::sync::atomic::Ordering::SeqCst);

    if tname.as_str() == "main" {
        hlt_loop(); // main thread having an error implies initialization error, which is fatal.
    }
    for hook in take_hooks() {
        hook(info)
    }
    // drop(lock); // ensure other threads can still print.
    unsafe { scheduler::panic_current() };
}

/// This function is called on panic.
///
/// Ideally, you never call this function directly - in fact, you cannot; as a type of [`PanicInfo`] can never be created by users.
/// # Example
/// using the panic macro
/// ```
/// panic!("Abort Ship!");
/// ```
#[allow(unused_must_use)]
pub fn panic_raw(info: &PanicInfo) -> ! {
    use core::fmt::Write;
    x86_64::instructions::interrupts::disable();
    let message = info.message();
    let loc = info.location();
    let unwind = info.can_unwind();
    if let Some(loc) = loc {
        if unwind {
            serial_println!("unwinding panic caused at {}: ", loc);
        } else {
            serial_println!("abort: panic caused at {}: ", loc);
        }
    } else if unwind {
        serial_println!("unwinding panic caused at unknown location: ");
    } else {
        serial_println!("abort: panic caused at unknown location: ");
    }

    serial_println!("{}", message);
    let c = c"=> Last OS Error";
    perror(c.as_ptr().cast_mut());

    // if let Some(mut wlock) = WRITER.try_lock() {
    //     wlock.set_color(Color::LightCyan, Color::Black);
    //     let message = info.message();
    //     let loc = info.location();
    //     let unwind = info.can_unwind();
    //     if let Some(loc) = loc {
    //         if unwind {
    //             writeln!(wlock, "Unwinding panic caused at {loc}: ");
    //         } else {
    //             writeln!(wlock, "abort: panic caused at {loc}: ");
    //         }
    //     } else if unwind {
    //         writeln!(wlock, "Unwinding panic caused at unknown location: ");
    //     } else {
    //         writeln!(wlock, "abort: panic caused at unknown location: ");
    //     }
    //     wlock.set_color(Color::White, Color::Black);
    //     writeln!(wlock, "{message}");
    //     wlock.set_color(Color::LightCyan, Color::Black);
    //     let err_code = get_errno();
    //     if let Some(meaning) = err_code.meaning() {
    //         let err_code = *err_code;
    //         writeln!(
    //             wlock,
    //             "=> Last OS Error: {} (os error {})",
    //             meaning, err_code
    //         );
    //     } else {
    //         let err_code = *err_code;
    //         writeln!(wlock, "=> Last OS Error: {}", err_code);
    //     }

    //     drop(wlock);
    // }
    hlt_loop()
}
