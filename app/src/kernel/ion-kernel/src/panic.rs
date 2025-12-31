use core::panic::PanicInfo;

use cfg_if::cfg_if;

use crate::{serial_println, text::{Color, println, set_print_color}};

/// This function is called on panic.
#[panic_handler]
pub fn panic(info: &PanicInfo) -> ! {
    let message = info.message();
    let loc = info.location();
    let unwind = info.can_unwind();
    set_print_color(Color::Blue, Color::Black);
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
    set_print_color(Color::Blue, Color::Black);
    cfg_if! {
        if #[cfg(debug_assertions)] {
            println!("=> note: debug assertions are ON.");
            serial_println!("=> note: debug assertions are ON.")
        } else {
            println!("=> note: Debug assertions are OFF.");
            serial_println!("=> note: Debug assertions are OFF.");
            set_print_color(Color::Green, Color::Black);
            println!("=> help: It is recommended to use debug assertions when developing.");
            serial_println!("=> help: It is recommended to use debug assertions when developing.");
        }
    }

    loop {}
}