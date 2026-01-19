use core::fmt::Debug;

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString as _},
};

use crate::{
    address_space::{AddressSpace, kernel_address_space}, arch, hlt_loop, thread::{self, JoinHandle, Thread}
};

struct ThreadMetadata {
    name: String,
    panicking: bool,
    safe: bool,
}

#[thread_local]
static mut THREAD_DATA: ThreadMetadata = ThreadMetadata {
    name: String::new(),
    panicking: false,
    safe: true,
};

#[derive(Default)]
pub enum TPriority {
    High,
    #[default]
    Med,
    Low,
}

pub struct ThreadBuilder<T: Send + 'static> {
    name: Option<String>,
    func: Option<Box<dyn FnOnce() -> T + Send + 'static>>,
    safe: bool,
    stack: usize,
    priority: TPriority,
    addr_space: AddressSpace
}

impl<T: Send + 'static> Debug for ThreadBuilder<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ThreadBuilder")
            .field("name", &self.name)
            .field("preemption_safe", &self.safe)
            .finish()
    }
}

impl<T: Send + 'static> ThreadBuilder<T> {
    pub fn new() -> Self {
        Self {
            name: None,
            func: None,
            safe: true,
            stack: 16 * 1024,
            priority: TPriority::Med,
            addr_space: unsafe { kernel_address_space() }
        }
    }

    pub const fn with_safety(mut self, safe: bool) -> Self {
        self.safe = safe;
        self
    }

    pub const fn set_safety(&mut self, safe: bool) -> &mut Self {
        self.safe = safe;
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn set_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_entry(mut self, f: impl FnOnce() -> T + Send + 'static) -> Self {
        self.func = Some(Box::new(f));
        self
    }

    pub fn runs(&mut self, f: impl FnOnce() -> T + Send + 'static) -> &mut Self {
        self.func = Some(Box::new(f));
        self
    }

    pub const fn with_priority(mut self, prio: TPriority) -> Self {
        self.priority = prio;
        self
    }

    pub const fn set_priority(&mut self, prio: TPriority) -> &mut Self {
        self.priority = prio;
        self
    }

    pub const fn with_stack(mut self, stack: usize) -> Self {
        self.stack = stack;
        self
    }

    pub const fn set_stack(&mut self, stack: usize) -> &mut Self {
        self.stack = stack;
        self
    }

    /// Builds the thread, queueing it into the scheduler.
    /// 
    /// this an alias for `spawn` that simply does not return the value.
    pub fn build(self) {
        _ = self.spawn();
    }

    pub fn raw(self) -> Thread {
        struct Meta<T: Send + 'static> {
            func: Box<dyn FnOnce() -> T>,
            name: String,
            safe: bool,
        }
        extern "C" fn thread_entry<T: Send + 'static>(arg: usize) -> ! {
            let meta = unsafe { (arg as *const Meta<T>).read() };

            unsafe {
                THREAD_DATA.name = meta.name;
                THREAD_DATA.safe = meta.safe;
            }

            (meta.func)();

            // drop the box
            drop(unsafe { Box::from_raw(arg as *mut Meta<T>) });

            thread::exit_current();
            hlt_loop()
        }

        let meta = Meta {
            func: self.func.expect("function should be provided"),
            name: self.name.unwrap_or("<unnamed>".to_string()),
            safe: self.safe,
        };
        let boxed = Box::new(meta);

        Thread::new(thread_entry::<T>, Box::into_raw(boxed) as usize, self.stack, self.addr_space)
    }

    /// spawns the thread.
    /// 
    /// note: this discards address space, but this will be changed!
    #[must_use = "if you do not need the return value, use `build` instead."]
    pub fn spawn(self) -> JoinHandle<T> {
        super::spawn_at(
            move || {
                let id = thread::current().unwrap().id().0;
                unsafe {
                    THREAD_DATA.name = self.name.unwrap_or(format!("<id {id}>"));
                    THREAD_DATA.safe = self.safe;
                }

                if let Some(func) = self.func {
                    func()
                } else {
                    panic!("ThreadBuilder received no function to run.");
                }
            },
            self.priority,
            self.stack,
        )
    }
}

/// Note: calling this function before the first thread is initialized causes page faults.
pub fn panicking() -> bool {
    if arch::tls::read_ptr().is_null() {
        return false;
    }
    unsafe { THREAD_DATA.panicking }
}

pub fn preemption_safe() -> bool {
    if arch::tls::read_ptr().is_null() {
        return false;
    }
    unsafe { THREAD_DATA.safe }
}

#[allow(static_mut_refs)]
pub fn name() -> String {
    if arch::tls::read_ptr().is_null() {
        return Default::default();
    }
    unsafe { THREAD_DATA.name.clone() }
}

pub fn set_name(s: impl Into<String>) {
    if arch::tls::read_ptr().is_null() {
        return;
    }
    unsafe { THREAD_DATA.name = s.into() };
}

/// # Safety
/// must only be called from panic handler.
pub unsafe fn set_panic() {
    if arch::tls::read_ptr().is_null() {
        return;
    }
    unsafe { THREAD_DATA.panicking = true }
}
