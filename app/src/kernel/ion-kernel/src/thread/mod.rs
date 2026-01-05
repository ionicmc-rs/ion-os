#![allow(missing_docs, clippy::missing_safety_doc, missing_debug_implementations)]
use core::sync::atomic::{AtomicU64, Ordering};

use crate::{serial_println, thread::{context::Context, stack::ThreadStack}};

pub mod stack;
pub mod context;
mod join;

pub use join::{JoinHandle, spawn};

pub use crate::scheduler::{exit_current, current, yield_now};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadID(u64);

pub struct Thread {
    id: ThreadID,
    /// owned memory to ensure [`Drop`] runs on the allocation
    _stack: ThreadStack,
    ctx: context::Context,
    state: State,
    // No metadata storage here, we do that with thread local storage.
}

impl Thread {
    pub fn new(
        entry: extern "C" fn(usize) -> !,
        arg: usize,
        stack: usize
    ) -> Self {
        serial_println!("creating stack.");
        let stack = ThreadStack::of_size(stack);
        serial_println!("created stack.");
        let id = ThreadID(NEXT_ID.fetch_add(1, Ordering::Relaxed));

        Self { 
            ctx: Context::new(entry, arg, (stack.ptr() as usize) + stack.size()),
            id,
            _stack: stack,
            state: State::Ready
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    Terminated,
    Blocked(*const bool),
    Running
}