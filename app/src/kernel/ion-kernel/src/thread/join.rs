use alloc::boxed::Box;
use x86_64::instructions;

use crate::{
    hlt_loop, scheduler::{self, SCHEDULER}, serial_println, thread::Thread
};

/// Handle used to retrieve the result of a spawned thread.
pub struct JoinHandle<T: Send + 'static> {
    result: *mut Option<T>,
}

impl<T: Send + 'static> JoinHandle<T> {
    /// Blocks until the thread finishes.
    pub fn join(self) -> T {
        loop {
            let res = unsafe { &*self.result };
            if res.is_some() {
                break;
            }
            instructions::hlt();
        }

        unsafe {
            // take result and free storage
            let boxed = Box::from_raw(self.result);
            boxed.expect("thread finished but result missing")
        }
    }

    /// Non-blocking check.
    pub fn try_join(&self) -> Option<&T> {
        unsafe { (&*self.result).as_ref() }
    }
}

pub fn spawn<
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
>(f: F, stack_size: usize) -> JoinHandle<T> {
    /// Heap object shared between parent and thread.
    struct Meta<T: Send + 'static> {
        func: Option<Box<dyn FnOnce() -> T + Send + 'static>>,
        result: *mut Option<T>,
    }

    let rsp: usize;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp); }
    serial_println!("rsp = {:#x}", rsp);


    serial_println!("allocating for the result");
    // Heap-allocate result storage
    let result_ptr = Box::into_raw(Box::new(None::<T>));
    serial_println!("allocated");

    // Heap-allocate metadata
    let meta = Box::new(Meta {
        func: Some(Box::new(f)),
        result: result_ptr,
    });

    let meta_ptr = Box::into_raw(meta) as usize;

    extern "C" fn thread_entry<T: Send + 'static>(arg: usize) -> ! {
        // Take ownership of Meta
        let mut meta = unsafe { Box::from_raw(arg as *mut Meta<T>) };

        let func = meta
            .func
            .take()
            .expect("thread entry called twice");
        serial_println!("running func");
        let res = func();
        serial_println!("done");

        serial_println!("setting result");
        unsafe {
            *meta.result = Some(res);
        }
        serial_println!("exiting");
        // Thread must never return
        scheduler::exit_current();
        hlt_loop()
    }
    serial_println!("creating thread.");
    let thread = Thread::new(thread_entry::<T>, meta_ptr, stack_size);
    serial_println!("queuing");
    SCHEDULER.queue(thread);
    serial_println!("queued!");
    
    JoinHandle {
        result: result_ptr,
    }
}
