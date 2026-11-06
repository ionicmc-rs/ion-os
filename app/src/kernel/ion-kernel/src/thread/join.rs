use core::fmt::Display;

use alloc::{boxed::Box, format};

use crate::{
    address_space::kernel_address_space,
    hlt_loop, log,
    scheduler::{self, current_scheduler},
    serial_println,
    thread::{self, Thread, ThreadID, build::TPriority},
};

/// Handle used to retrieve the result of a spawned thread.
pub struct JoinHandle<T: Send + 'static> {
    result: *mut Option<T>,
    id: ThreadID,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinErrorKind {
    Unfinished,
    Unset,
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
            JoinErrorKind::Unset => "but it prematurely exited",
        };
        write!(f, "Attempted to join thread id {}, {fmt}.", self.0.0)
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
        thread::block_until_end_of(self.id);

        if scheduler::has_panicked(self.id) {
            unsafe { scheduler::remove_panic_id(self.id) };
            return Err(JoinError(self.id, JoinErrorKind::Panicked));
        }

        let res = unsafe { &*self.result };
        if res.is_none() {
            return Err(JoinError(self.id, JoinErrorKind::Unset));
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

    pub fn id(&self) -> ThreadID {
        self.id
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

    // Heap-allocate result storage
    let result_ptr = Box::into_raw(Box::new(None::<T>));

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
        thread::set_name(format!("<joiner {:?}>", meta.result));

        let func = meta.func.take().expect("thread entry called twice");

        let sp: u64;
        unsafe {
            core::arch::asm!("mov {}, rsp", out(reg) sp);
        }
        serial_println!("SP before calling func: {sp:#x} (mod 16: {})", sp % 16);

        let res = func();

        unsafe {
            *meta.result = Some(res);
        }
        // Thread must never return
        scheduler::exit_current();
        hlt_loop()
    }
    // Note: do not support user threads in spawn - user threads must be built via builder.

    // Safety: We fulfill all requirements
    let thread = Thread::new(thread_entry::<T>, meta_ptr, stack_size);
    let id = thread.id();
    current_scheduler().queue(thread);

    JoinHandle {
        result: result_ptr,
        id,
    }
}

pub fn spawn_at<F: FnOnce() -> T + Send + 'static, T: Send + 'static>(
    f: F,
    prio: TPriority,
    stack_size: usize,
) -> JoinHandle<T> {
    /// Heap object shared between parent and thread.
    #[repr(align(16), C)]
    struct Meta<T: Send + 'static> {
        func: Option<Box<dyn FnOnce() -> T + Send + 'static>>,
        result: *mut Option<T>,
    }

    // Heap-allocate result storage
    let result_ptr = Box::into_raw(Box::new(None::<T>));

    // Heap-allocate metadata
    let meta = Box::new(Meta {
        func: Some(Box::new(f)),
        result: result_ptr,
    });

    let meta_ptr = Box::into_raw(meta) as usize;

    #[allow(unused, dead_code)]
    extern "C" fn thread_entry<T: Send + 'static>(arg: usize) -> ! {
        // x86_64::instructions::interrupts::disable();
        // Take ownership of Meta
        let mut meta = unsafe { Box::from_raw(arg as *mut Meta<T>) };
        thread::set_name(format!("<joiner {:?}>", meta.result));

        let func = meta.func.take().expect("thread entry called twice");

        let res = func();

        unsafe {
            *meta.result = Some(res);
        }

        // log::warn!("Leaking memory - this is temporary and must eventually be fixed");
        // core::mem::forget(meta);

        serial_println!("exiting");
        // Thread must never return
        scheduler::exit_current();
        hlt_loop()
    }
    let thread = Thread::new(thread_entry::<T>, meta_ptr, stack_size);
    let id = thread.id();
    current_scheduler().queue_at(prio, thread);

    JoinHandle {
        result: result_ptr,
        id,
    }
}
