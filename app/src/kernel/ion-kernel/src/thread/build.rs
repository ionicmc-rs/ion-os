use core::fmt::Debug;

use alloc::{
    alloc::{Allocator, Global},
    boxed::Box,
    string::{String, ToString as _},
    sync::Arc,
    vec::Vec,
};

use crate::{
    address_space::{AddressSpace, kernel_address_space},
    arch, serial_println,
    thread::{self, JoinHandle, Thread, ThreadID, tls::tcb},
};

struct ThreadMetadata {
    name: Option<Vec<u8, Arc<dyn Allocator>>>,
    panicking: bool,
    kill_safe: bool,
}

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
    addr_space: AddressSpace,
}

impl<T: Send + 'static> Debug for ThreadBuilder<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ThreadBuilder")
            .field("name", &self.name)
            .field("kill_safe", &self.safe)
            .finish()
    }
}

struct Meta<T: Send + 'static> {
    func: Box<dyn FnOnce() -> T, Arc<dyn Allocator>>,
    name: String,
    safe: bool,
    alloc: Arc<dyn Allocator>,
}

extern "C" fn thread_entry<T: Send + 'static>(arg: usize) -> ! {
    // Reconstruct the box WITH the correct allocator
    let meta_ptr = arg as *mut Meta<T>;

    let boxed: Box<Meta<T>, Arc<dyn Allocator>> = unsafe {
        let alloc = (*meta_ptr).alloc.clone();
        Box::from_raw_in(meta_ptr, alloc)
    };

    // FULL MOVE OUT of the box
    let Meta {
        func,
        name,
        safe,
        alloc: _, // allocator already used
    } = *boxed;

    // boxed is consumed here; no partial moves exist

    unsafe {
        tcb().name = name;
        tcb().is_kill_safe = safe;
    }

    // Call FnOnce — consumes it
    func();

    // All remaining fields are dropped naturally here

    thread::exit_current();
}

impl<T: Send + 'static> Default for ThreadBuilder<T> {
    fn default() -> Self {
        Self::new()
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
            addr_space: unsafe { kernel_address_space() },
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

    pub fn with_addr_space(mut self, space: AddressSpace) -> Self {
        self.addr_space = space;
        self
    }

    /// Builds the thread, queueing it into the scheduler.
    ///
    /// this an alias for `spawn` that simply does not return the value.
    ///
    /// this *does* return the id, however, as that is often needed
    pub fn build(self) -> ThreadID {
        self.spawn().id()
    }

    #[must_use = "This method does not queue the thread - not using causes drops"]
    pub fn raw(mut self) -> Thread {
        let alloc: Arc<dyn Allocator> = if let Some(v) = self.addr_space.allocator() {
            serial_println!("user space detected");
            v
        } else {
            Arc::new(Global)
        };

        let name = self.name.unwrap_or("<unnamed>".to_string());

        // Re-home closure (unsized-safe)
        let func = self.func.take().expect("function should be provided");
        let raw = Box::into_raw(func);
        let func = unsafe { Box::from_raw_in(raw, alloc.clone()) };

        let meta = Meta {
            func,
            name,
            safe: self.safe,
            alloc: alloc.clone(),
        };

        let boxed = Box::new_in(meta, alloc.clone());

        Thread::new(
            thread_entry::<T>,
            Box::into_raw_with_allocator(boxed).0 as usize,
            self.stack,
        )
    }

    /// spawns the thread.
    ///
    /// note: this discards address space, but this will be changed!
    #[must_use = "if you do not need the return value, use `build` instead."]
    pub fn spawn(self) -> JoinHandle<T> {
        super::spawn_at(
            move || {
                unsafe {
                    let name = self.name.unwrap_or("<unnamed>".to_string());

                    tcb().name = name;
                    tcb().is_kill_safe = self.safe;
                }

                if let Some(func) = self.func {
                    func()
                } else {
                    core::panic!("ThreadBuilder received no function to run.");
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
    unsafe { tcb().panicking }
}

pub fn kill_safe() -> bool {
    if arch::tls::read_ptr().is_null() {
        return false;
    }
    unsafe { tcb().is_kill_safe }
}

pub fn set_kill_safe(safe: bool) {
    // Early initialization can run futures before the scheduler creates the
    // boot thread's TCB. In that context there is no per-thread state to set.
    if arch::tls::read_ptr().is_null() {
        return;
    }
    unsafe { tcb().is_kill_safe = safe }
}

#[allow(static_mut_refs)]
pub fn name() -> String {
    if arch::tls::read_ptr().is_null() {
        panic!("use of TLS before initialization");
    }
    unsafe { tcb().name.clone() }
}

pub fn set_name(s: impl Into<String>) {
    if arch::tls::read_ptr().is_null() {
        return;
    }

    tcb().name = s.into();
}

/// # Safety
/// must only be called from panic handler.
pub unsafe fn set_panic() {
    if arch::tls::read_ptr().is_null() {
        return;
    }
    unsafe { tcb().panicking = true }
}
