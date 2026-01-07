//! Main entry point
use crate::{hlt_loop, serial_println};

/// final init.
pub extern "C" fn kernel_main() -> ! {
    serial_println!("sup, world");
    serial_println!("works fine!");
    hlt_loop()
}