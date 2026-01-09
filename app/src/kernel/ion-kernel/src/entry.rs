//! Main entry point

use crate::{
    hlt_loop, serial_println, thread::{self, ThreadBuilder, build::TPriority, sync::{Mutex, MutexGuard}}
};

static MUTEX: Mutex<u32> = Mutex::new(10);

/// final init.
pub extern "C" fn kernel_main() -> ! {
    ThreadBuilder::new()
        .with_name("need_kbd")
        .with_entry(|| {
            let lock = MUTEX.lock();
            assert_eq!(lock, 10);
            MutexGuard::add_hook_for(lock);
            panic!("poison lock");
        })
        .with_priority(TPriority::High)
        .build();
    ThreadBuilder::new()
        .with_name("waiting_mutex")
        .with_entry(|| {
            let x = MUTEX.lock();
            serial_println!("lock: {x}"); // should not run!
        })
        .build();
    thread::exit_current();
    hlt_loop()
}
