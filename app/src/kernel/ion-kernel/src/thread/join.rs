use core::fmt::Display;

use alloc::boxed::Box;

use crate::{
    hlt_loop,
    scheduler::{self, SCHEDULER},
    serial_println,
    thread::{self, Thread, ThreadID},
};

/// Handle used to retrieve the result of a spawned thread.
pub struct JoinHandle<T: Send + 'static> {
    result: *mut Option<T>,
    id: ThreadID,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinErrorKind {
    Unfinished,
    Panicked, // todo: add panic info here.
}

/// error while joining a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinError(ThreadID, JoinErrorKind);

impl Display for JoinError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let fmt = match self.1 {
            JoinErrorKind::Panicked => "but it panicked",
            JoinErrorKind::Unfinished => "but it did not finish yet",
        };
        write!(f, "Attempted to join thread id {}, {fmt}.", self.0 .0)
    }
}

impl JoinError {
    pub fn kind(&self) -> JoinErrorKind {
        self.1
    }
}

impl<T: Send + 'static> JoinHandle<T> {
    /// Blocks until the thread finishes.
    pub fn join(self) -> Result<T, JoinError> {
        loop {
            let res = unsafe { &*self.result };
            if res.is_some() {
                break;
            }

            if scheduler::has_panicked(self.id) {
                unsafe { scheduler::remove_panic_id(self.id) };
                return Err(JoinError(self.id, JoinErrorKind::Panicked));
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
    pub fn try_join(&self) -> Result<&T, JoinError> {
        if scheduler::has_panicked(self.id) {
            return Err(JoinError(self.id, JoinErrorKind::Panicked));
        }

        unsafe { &*self.result }
            .as_ref()
            .ok_or(JoinError(self.id, JoinErrorKind::Unfinished))
    }
}

pub fn spawn<F: FnOnce() -> T + Send + 'static, T: Send + 'static>(
    f: F,
    stack_size: usize,
) -> JoinHandle<T> {
    /// Heap object shared between parent and thread.
    struct Meta<T: Send + 'static> {
        func: Option<Box<dyn FnOnce() -> T + Send + 'static>>,
        result: *mut Option<T>,
    }

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
        thread::set_name("<joiner>");
        serial_println!("name: {}", thread::name());
        // Take ownership of Meta
        let mut meta = unsafe { Box::from_raw(arg as *mut Meta<T>) };

        let func = meta.func.take().expect("thread entry called twice");

        let res = func();

        unsafe {
            *meta.result = Some(res);
        }
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
        id,
    }
}
