//! Main entry point
use crate::{
    hlt_loop, serial_println,
    thread::{self, ThreadBuilder},
};

/// final init.
pub extern "C" fn kernel_main() -> ! {
    serial_println!("sup, world");

    let other = ThreadBuilder::new()
        .with_name("panics")
        .with_entry(|| {
            panic!("panicked!");
        })
        .spawn();
    other.join().expect_err("panics should produce an error");

    serial_println!("and it doesn't crash!");

    thread::exit_current();
    hlt_loop()
}
