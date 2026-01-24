//! panicking.
use core::{cell::Cell, panic::PanicInfo, sync::atomic::compiler_fence};

use alloc::{boxed::Box, string::String, vec::Vec};
use x86_64::registers::model_specific::FsBase;

use crate::{
    c_lib::libc::{get_errno, perror},
    hlt_loop,
    io::Error,
    scheduler, serial_println,
    text::{Color, WRITER},
    thread,
};

#[thread_local]
static THREAD_HOOK: Cell<Vec<Box<dyn FnOnce()>>> = Cell::new(Vec::new());

/// Sets the function run at the end of panics.
///
/// note: this does not replace the panic function, but it runs alongside it at the end.
///
/// this is usually useful to poison mutexes while we don't have unwinding.
pub fn add_hook(f: impl FnOnce() + 'static) {
    if thread::current().is_none() {
        panic!("Attempted to set a panic hook from the boot stack.");
    }
    let mut old = THREAD_HOOK.take();
    old.push(Box::new(f));
    THREAD_HOOK.set(old);
}

/// takes the current hooks, leaving nothing in its place.
pub fn take_hooks() -> Vec<Box<dyn FnOnce()>> {
    if thread::current().is_none() {
        panic!("Attempted to take a panic hook from the boot stack.");
    }
    THREAD_HOOK.take()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if FsBase::read().as_u64() == 0 {
        // panic before thread init - unsafe to use panic thread
        panic_raw(info);
    }

    panic_thread(info);
}

/// panics the current thread.
pub fn panic_thread(info: &PanicInfo) -> ! {
    if thread::panicking() {
        serial_println!("double panic - halt");
        hlt_loop()
    }
    unsafe { thread::build::set_panic() };
    let loc = info.location().unwrap();
    let message = info.message();
    let tname = thread::name().map(|v|
        String::from_utf8(v.to_vec()).unwrap()
    )
    .unwrap_or_else(|| "<unnamed>".into());
    match info.can_unwind() {
        true => serial_println!("(unwind) thread `{tname}` panicked at {loc}"),
        false => serial_println!("(abort) thread `{tname}` panicked at {loc}"),
    }

    serial_println!("{message}");

    let c = c"=> Last THREAD Error";
    perror(c.as_ptr().cast_mut());

    let mut lock = match WRITER.try_lock() {
        Some(v) => v,
        None => {
            for hook in take_hooks() {
                hook()
            }
            unsafe { scheduler::panic_current() };
        }
    };

    use core::fmt::Write;
    lock.set_color(Color::LightCyan, Color::Black);
    let _ = match info.can_unwind() {
        true => writeln!(lock, "(unwind) thread `{tname}` panicked at {loc}"),
        false => writeln!(lock, "(abort) thread `{tname}` panicked at {loc}"),
    };
    lock.set_color(Color::White, Color::Black);

    let _ = writeln!(lock, "{message}");

    lock.set_color(Color::LightCyan, Color::Black);
    let err = Error::last_os_err();
    let _ = writeln!(lock, "=> Last THREAD Error: {err}");

    // everything before must stay before, and everything after must stay after.
    compiler_fence(core::sync::atomic::Ordering::SeqCst);

    if tname.as_str() == "main" {
        hlt_loop(); // main thread having an error implies initialization error, which is fatal.
    }
    for hook in take_hooks() {
        hook()
    }
    drop(lock); // ensure other threads can still print.
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

    if let Some(mut wlock) = WRITER.try_lock() {
        wlock.set_color(Color::LightCyan, Color::Black);
        let message = info.message();
        let loc = info.location();
        let unwind = info.can_unwind();
        if let Some(loc) = loc {
            if unwind {
                writeln!(wlock, "Unwinding panic caused at {loc}: ");
            } else {
                writeln!(wlock, "abort: panic caused at {loc}: ");
            }
        } else if unwind {
            writeln!(wlock, "Unwinding panic caused at unknown location: ");
        } else {
            writeln!(wlock, "abort: panic caused at unknown location: ");
        }
        wlock.set_color(Color::White, Color::Black);
        writeln!(wlock, "{message}");
        wlock.set_color(Color::LightCyan, Color::Black);
        let err_code = get_errno();
        if let Some(meaning) = err_code.meaning() {
            let err_code = *err_code;
            writeln!(
                wlock,
                "=> Last OS Error: {} (os error {})",
                meaning, err_code
            );
        } else {
            let err_code = *err_code;
            writeln!(wlock, "=> Last OS Error: {}", err_code);
        }

        drop(wlock);
    }
    hlt_loop()
}
