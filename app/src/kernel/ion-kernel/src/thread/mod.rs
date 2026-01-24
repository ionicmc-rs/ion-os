#![allow(
    missing_docs,
    clippy::missing_safety_doc,
    missing_debug_implementations
)]
#[cfg(debug_assertions)]
use core::ptr::fn_addr_eq;
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(debug_assertions)]
use crate::scheduler::idle_entry;
use crate::{
    address_space::AddressSpace, scheduler::{self, SCHEDULER, WakeEvent}, serial_println, thread::{
        context::Context,
        stack::ThreadStack,
        tls::{TlsAllocation, alloc_thread_self},
    }
};
use crate::arch;

pub mod context;
mod join;
pub mod pool;
pub mod stack;
pub mod sync;

use alloc::{alloc::{Allocator, Global}, sync::Arc};
pub use join::{JoinError, JoinErrorKind, JoinHandle, spawn, spawn_at};

pub use crate::scheduler::{
    atomic_block_until, block_until, block_until_end_of, block_until_key_input, block_until_true,
    critical_moment, critical_moment_raw, current, exit_current, has_panicked, remove_panic_id,
    request_reschedule, reschedule_if_needed, start, yield_now,
    kill
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadID(pub u64);

pub struct Thread {
    id: ThreadID,
    ctx: context::Context,
    state: State,
    tls: usize,

    /// owned memory to ensure [`Drop`] runs on the allocation
    _stack: ThreadStack<Arc<dyn Allocator>>, // this is a little fishy....
    // guards against forgetting to drop the allocation.
    _tls_guard: TlsAllocation, // No metadata storage here, we do that with thread local storage.
}

impl Drop for Thread {
    fn drop(&mut self) {
        serial_println!("Thread <id {}> dropped here", self.id.0)
    }
}

impl Thread {
    pub fn new(entry: extern "C" fn(usize) -> !, arg: usize, stack: usize, addr_space: AddressSpace) -> Self {
        // use critical moment to prevent alloc from rescheduling.
        self::critical_moment_raw(|| {
            serial_println!("creating stack.");
            let alloc: Arc<dyn Allocator> = if let Some(v) = addr_space.allocator() {
                v
            } else {
                Arc::new(Global)
            };
            serial_println!("retrieved alloc");
            let mut stack = ThreadStack::of_size(stack, alloc);
            serial_println!("created stack.");
            let id = ThreadID(NEXT_ID.fetch_add(1, Ordering::Relaxed));

            #[cfg(debug_assertions)]
            if fn_addr_eq(entry, idle_entry as extern "C" fn(usize) -> !) {
                stack.should_not_drop();
            }

            let (raw_ptr, guard) = unsafe { alloc_thread_self(stack.ptr(), stack.size()) };

            Self {
                ctx: Context::new(entry, arg, (stack.ptr() as usize) + stack.size(), addr_space),
                id,
                tls: raw_ptr as usize,
                _stack: stack,
                state: State::Ready,
                _tls_guard: guard,
            }
        })
    }
    pub fn tls_block(&self) -> usize {
        self.tls
    }

    pub fn set_sleep(&mut self, ticks: u64) {
        let wake_tick = arch::interrupts::ticks().wrapping_add(ticks);
        self.state = State::Sleeping(wake_tick);
    }

    pub fn should_wake(&self) -> bool {
        if let State::Sleeping(wake_tick) = self.state {
            arch::interrupts::ticks() == wake_tick
        } else {
            false
        }
    }

    pub fn wake(&mut self) {
        if let State::Sleeping(tick) = self.state {
            serial_println!(
                "[thread ({})] thread has awoken on tick {}.",
                self.id.0,
                tick
            );
            self.state = State::Ready;
        }
    }

    pub fn state(&self) -> &State {
        &self.state
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

    pub fn dump_stack(&self) {
        serial_println!(
            "thread <id {:?}> stack ranges from {:p}->{:p}",
            self.id,
            self._stack.ptr(),
            self._stack.ptr().wrapping_add(self._stack.size())
        );
    }
}

/// The next available ID
/// # Safety
/// must never be mutated outside [`Thread::new`]
pub static NEXT_ID: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
pub enum State {
    #[default]
    Ready,
    Terminated,
    Blocked(WakeEvent),
    Running,
    Sleeping(u64),
}

// TOOLS

#[track_caller] // for panics.
pub fn sleep(ms: u64) {
    serial_println!("sleep was called");
    let id = current()
        .expect("sleep should be called after thread initialization.")
        .id()
        .0;
    serial_println!("[thread ({id})] now sleeping for <{ms}> milliseconds");

    scheduler::sleep_current(ms);
}

pub fn id() -> ThreadID {
    current()
        .expect("cannot retrieve thread id before initialization.")
        .id()
}

pub fn init() {
    SCHEDULER.init();
}

pub mod build;
pub mod tls;

pub use build::{ThreadBuilder, name, panicking, preemption_safe, set_name};
