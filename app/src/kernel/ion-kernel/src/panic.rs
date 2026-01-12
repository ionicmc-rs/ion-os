//! Tools for panicking
//! 
//! see [`core::panicking`]
use core::panic::PanicInfo;

use crate::{c_lib::libc::{error::get_errno, perror}, hlt_loop, serial_println, text::{Color, println, set_print_color}};

/// This function is called on panic.
/// 
/// Ideally, you never call this function directly - in fact, you cannot; as a type of [`PanicInfo`] can never be created by users.
/// # Example
/// using the panic macro
/// ```
/// panic!("Abort Ship!");
/// ```
#[panic_handler]
pub fn panic(info: &PanicInfo) -> ! {
    let message = info.message();
    let loc = info.location();
    let unwind = info.can_unwind();
    set_print_color(Color::LightCyan, Color::Black);
    if let Some(loc) = loc {
        if unwind {
            println!("Unwinding panic caused at {loc}: ");
            serial_println!("Unwinding panic caused at {}: ", loc);
        } else {
            println!("abort: panic caused at {loc}: ");
            serial_println!("abort: panic caused at {}: ", loc);
        }
    } else if unwind {
        println!("Unwinding panic caused at unknown location: ");
        serial_println!("Unwinding panic caused at unknown location: ");
    } else {
        println!("abort: panic caused at unknown location: ");
        serial_println!("abort: panic caused at unknown location: ");
    }
    set_print_color(Color::White, Color::Black);
    println!("{message}");
    serial_println!("{}", message);
    set_print_color(Color::LightCyan, Color::Black);
    let err_code = get_errno();
    let c = c"=> Last OS Error";
    perror(c.as_ptr().cast_mut());
    if let Some(meaning) = err_code.meaning() {
        let err_code = *err_code;
        println!("=> Last OS Error: {} (os error {})", meaning, err_code);
    } else {
        let err_code = *err_code;
        println!("=> Last OS Error: {}", err_code);
    }

    // Here we disable all things enabled

    x86_64::instructions::interrupts::disable();

    hlt_loop()
}