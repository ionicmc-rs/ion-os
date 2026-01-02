//! Serial Printing to STDOUT (Terminal)
//! 
//! This module is only for debugging purposes and its functionality is disabled in release mode.
//! # Implementation Details
//! This module writes to port `0xE9` (Mapped to rust in [`SERIAL1`]), which prints to the standard output (terminal).
//! 
//! We support 2 forms of this.
//! 
//! - basic form
//!   - almost never fails
//!   - basic - does not support [`fmt::Arguments`](core::fmt::Arguments)
//! - Port form
//!   - use the active [`SERIAL1`] port
//!   - prone to failure.
//!   - supports all input forms
//! # Example
//! debug logs
//! ```
//! serial_println!("Initialized Ion OS!");
//! serial::dbg::str("An it did not crash!\n");
//! ```

use uart_16550::SerialPort;
use spin::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    /// Serial Port for Standard Output
    /// 
    /// Use the safe abstractions [`serial_println`], [`serial_print`], [`serial::dbg::*`](dbg),
    /// which support actual strings.
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0xE9) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Even though `write_fmt` always returns `Ok(())`, we are better off ignoring the value instead of
    // panicking.
    //
    // this also must run without interrupts, as some of our interrupt handlers print to Serial, 
    // which could cause a deadlock if we are already printing. see 
    // https://os.phil-opp.com/hardware-interrupts/#provoking-a-deadlock
    let _ = interrupts::without_interrupts(|| {
        SERIAL1.lock().write_fmt(args)
    });
}

/// Prints to the host through the serial interface.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

/// Prints to the host through the serial interface, appending a newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr_2021) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr_2021, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*))
}

/// base form for serial prints
pub mod dbg {
    /// print a single byte using asm
    /// 
    /// always works
    #[inline(always)]
    pub fn byte(b: u8) {
        unsafe { core::arch::asm!("out dx, al", in("dx") 0xE9u16, in("al") b); }
    }
    /// print a str to asm
    /// 
    /// always works
    pub fn str(s: &str) {
        for &b in s.as_bytes(){ byte(b); }
    }
}

