#![allow(missing_docs, clippy::missing_safety_doc, missing_debug_implementations)]
#[cfg(debug_assertions)]
use core::ptr::fn_addr_eq;
use core::{sync::atomic::{AtomicU64, Ordering}};

#[cfg(debug_assertions)]
use crate::scheduler::idle_entry;
use crate::{interrupts, scheduler::{self, SCHEDULER}, serial_println, thread::{context::Context, stack::ThreadStack}};

pub mod stack;
pub mod context;
mod join;

pub use join::{JoinHandle, spawn};

pub use crate::scheduler::{exit_current, current, yield_now, critical_moment, critical_moment_raw};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadID(pub u64);

#[derive(Debug)]
pub struct Thread {
    id: ThreadID,
    /// owned memory to ensure [`Drop`] runs on the allocation
    _stack: ThreadStack,
    ctx: context::Context,
    state: State,
    tls: usize,
    // No metadata storage here, we do that with thread local storage.
}

impl Thread {
    pub fn new(
        entry: extern "C" fn(usize) -> !,
        arg: usize,
        stack: usize
    ) -> Self {
        // use critical moment to prevent alloc from rescheduling.
        self::critical_moment_raw(|| {
            serial_println!("creating stack.");
            let mut stack = ThreadStack::of_size(stack);
            serial_println!("created stack.");
            let id = ThreadID(NEXT_ID.fetch_add(1, Ordering::Relaxed));

            #[cfg(debug_assertions)]
            if fn_addr_eq(entry, idle_entry as extern "C" fn(usize) -> !) {
                stack.should_not_drop();
            }
            
            Self { 
                ctx: Context::new(entry, arg, (stack.ptr() as usize) + stack.size()),
                id,
                tls: unsafe { tls::alloc_thread_self(stack.ptr(), stack.size()) as usize },
                _stack: stack,
                state: State::Ready,
            }
        })
    }

    pub fn tls_block(&self) -> usize {
        self.tls
    }

    pub fn set_sleep(&mut self, ticks: u64) {
        let wake_tick = interrupts::ticks().wrapping_add(ticks);
        self.state = State::Sleeping(wake_tick);
    }

    pub fn should_wake(&self) -> bool {
        if let State::Sleeping(wake_tick) = self.state {
            interrupts::ticks() == wake_tick
        } else {
            false
        }
    }

    pub fn wake(&mut self) {
        if let State::Sleeping(tick) = self.state {
            serial_println!("[thread ({})] thread has awoken on tick {}.", self.id.0, tick);
            self.state = State::Ready;
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
    }

    pub fn id(&self) -> ThreadID {
        self.id
    }

    pub fn ctx(&self) -> &Context {
        &self.ctx
    }

    pub fn ctx_mut(&mut self) -> &mut Context {
        &mut self.ctx
    }
}

/// The next available ID
/// # Safety
/// must never be mutated outside [`Thread::new`]
pub static NEXT_ID: AtomicU64 = AtomicU64::new(0);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Ready,
    Terminated,
    Blocked(*const bool),
    Running,
    Sleeping(u64)
}

// TOOLS

#[track_caller] // for panics.
pub fn sleep(ms: u64) {
    serial_println!("sleep was called");
    let id = current().expect("sleep should be called after thread initialization.").id().0;
    serial_println!("[thread ({id})] now sleeping for <{ms}> milliseconds");

    scheduler::sleep_current(ms);
}

pub fn init() {
    SCHEDULER.init();
}

pub mod tls;