//! Tools for panicking
//! 
//! see [`core::panicking`]
use core::panic::PanicInfo;

use crate::{hlt_loop, serial_println};

/// This function is called on panic.
/// 
/// Ideally, you never call this function directly - in fact, you cannot; as a type of [`PanicInfo`] can never be created by users.
/// # Example
/// using the panic macro
/// ```
/// panic!("Abort Ship!");
/// ```
#[panic_handler]
#[allow(unused_must_use)]
pub fn panic(info: &PanicInfo) -> ! {
    serial_println!("{info}");
    hlt_loop();
    // use core::fmt::Write;
    // x86_64::instructions::interrupts::disable();
    // let message = info.message();
    // let loc = info.location();
    // let unwind = info.can_unwind();
    // if let Some(loc) = loc {
    //     if unwind {
    //         serial_println!("Unwinding panic caused at {}: ", loc);
    //     } else {
    //         serial_println!("abort: panic caused at {}: ", loc);
    //     }
    // } else if unwind {
    //     serial_println!("Unwinding panic caused at unknown location: ");
    // } else {
    //     serial_println!("abort: panic caused at unknown location: ");
    // }

    // serial_println!("{}", message);
    // let c = c"=> Last OS Error";
    // perror(c.as_ptr().cast_mut());


    // if let Some(mut wlock) = WRITER.try_lock() {
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
    //     writeln!(wlock, "{message}");
    //     let err_code = get_errno();
    //     if let Some(meaning) = err_code.meaning() {
    //         let err_code = *err_code;
    //         writeln!(wlock, "=> Last OS Error: {} (os error {})", meaning, err_code);
    //     } else {
    //         let err_code = *err_code;
    //         writeln!(wlock, "=> Last OS Error: {}", err_code);
    //     }
    
    //     drop(wlock);
    // }
    // hlt_loop()
}