use core::fmt::Display;

use alloc::boxed::Box;

use crate::{
    hlt_loop, scheduler::{self, SCHEDULER}, serial_println, thread::{self, Thread, ThreadID}
};

/// Handle used to retrieve the result of a spawned thread.
pub struct JoinHandle<T: Send + 'static> {
    result: *mut Option<T>,
    id: ThreadID
}


/// empty, but this will be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanicError(ThreadID);

impl Display for PanicError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Attempted to join thread id {}, but it panicked", self.0.0)
    }
}

impl<T: Send + 'static> JoinHandle<T> {
    /// Blocks until the thread finishes.
    pub fn join(self) -> Result<T, PanicError> {
        loop {
            let res = unsafe { &*self.result };
            if res.is_some() {
                break;
            }

            if scheduler::has_panicked(self.id) {
                return Err(PanicError(self.id));
            }

            thread::yield_now();
        }

        unsafe {
            // take result and free storage
            let boxed = Box::from_raw(self.result);
            Ok(boxed.expect("thread finished but result missing"))
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

    #[allow(unused, dead_code)]
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
    let id = thread.id();
    serial_println!("queuing");
    SCHEDULER.queue(thread);
    serial_println!("queued!");
    
    JoinHandle {
        result: result_ptr,
        id
    }
}
