use core::panic::PanicInfo;

use crate::{c_lib::libc::{error::get_errno, perror}, hlt_loop, serial_println, text::{Color, println, set_print_color}};

/// This function is called on panic.
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
    if let Some(meaning) = err_code.meaning() {
        let err_code = *err_code;
        serial_println!("=> Last OS Error: {} (os error {})", meaning, err_code);
        println!("=> Last OS Error: {} (os error {})", meaning, err_code);
    } else {
        let err_code = *err_code;
        let c = c"=> Last OS Error";
        perror(c.as_ptr().cast_mut());
        println!("=> Last OS Error: {}", err_code);
    }

    hlt_loop()
}