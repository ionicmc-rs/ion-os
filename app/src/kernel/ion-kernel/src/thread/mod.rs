#![allow(
    missing_docs,
    clippy::missing_safety_doc,
    missing_debug_implementations
)]
#[cfg(debug_assertions)]
use core::ptr::fn_addr_eq;
use core::{fmt::Debug, sync::atomic::{AtomicU64, Ordering}};

#[cfg(debug_assertions)]
use crate::scheduler::idle_entry;
use crate::{
    address_space::{AddressSpace, kernel_address_space}, arch::ctx::Context, lib_alloc::THREAD_STACK_ALLOC, log, process::{ProcessID, elf::{ElfLoadError, load_elf}}, scheduler::{self, SCHEDULER, WakeEvent}, serial_println, thread::{
        stack::ThreadStack,
        tls::{ThreadSelf, TlsAllocation, alloc_thread_self},
    }
};
use crate::arch;

mod join;
pub mod pool;
pub mod stack;
pub mod sync;

use alloc::{alloc::{Allocator, Global}, sync::Arc};
use elf::endian::AnyEndian;
pub use join::{JoinError, JoinErrorKind, JoinHandle, spawn, spawn_at};

#[allow(unused)]
pub(crate) use crate::scheduler::{
    atomic_block_until, block_until, block_until_end_of, block_until_key_input, block_until_true,
    critical_moment, critical_moment_raw, current, exit_current, has_panicked, remove_panic_id,
    request_reschedule, reschedule_if_needed, start, yield_now,
    kill
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadID(pub u64);

pub struct Thread {
    id: ThreadID,
    ctx: arch::ctx::Context,
    state: State,
    tls: usize,

    /// owned memory to ensure [`Drop`] runs on the allocation
    _stack: Option<ThreadStack<Arc<dyn Allocator>>>, // this is a little fishy....
    // guards against forgetting to drop the allocation.
    _tls_guard: Option<TlsAllocation>, // No metadata storage here, we do that with thread local storage.
}

impl Debug for Thread {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Thread")
            .field("id", &self.id)
            .field("context", &self.ctx)
            .field("state", &self.state)
            .field("tls", &self.tls)
            .field_with("stack", |f| {
                f.pad(&alloc::format!("{:?} (size: {:?})", self._stack.as_ref().map(|s| s.ptr()), self._stack.as_ref().map(|s| s.size())))
            })
            .finish()
    }
}

impl Debug for State {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Blocked(ev) => match ev {
                WakeEvent::Atomic(_) | WakeEvent::FnYieldTrue(_) => {
                    f.debug_struct("Blocked").finish()
                }
                WakeEvent::EndOf(id) => {
                    f.debug_tuple("BlockedOnThread")
                        .field(id)
                        .finish()
                }
                WakeEvent::KeyInput => {
                    f.debug_struct("BlockedOnKeyInput")
                        .finish()
                }
                WakeEvent::Raw(ptr) => {
                    f.debug_tuple("BlockedOnPtr")
                        .field(ptr)
                        .finish()
                }
            }
            Self::Ready => f.debug_struct("Ready").finish(),
            Self::Running => f.debug_struct("Running").finish(),
            Self::Sleeping(tick) => f.debug_struct("Sleeping")
                .field("until", &tick)
                .finish(),
            Self::Terminated => f.debug_struct("Terminated").finish()
        }
    }
}

impl Thread {
    pub fn new(entry: extern "C" fn(usize) -> !, arg: usize, stack: usize) -> Self {
        // use critical moment to prevent alloc from rescheduling.
        self::critical_moment_raw(|| {
            let mut stack: ThreadStack<Arc<dyn Allocator>> = ThreadStack::of_size(stack, Arc::new(Global));
            let id = ThreadID(NEXT_ID.fetch_add(1, Ordering::Relaxed));

            #[cfg(debug_assertions)]
            if fn_addr_eq(entry, idle_entry as extern "C" fn(usize) -> !) {
                stack.should_not_drop();
            }

            let (raw_ptr, guard) = unsafe { alloc_thread_self(stack.ptr(), stack.size()) };

            let mut context = Context::initial(entry as usize, arg, stack.ptr() as usize + stack.size() as usize);

            context.set_addr_space(unsafe { kernel_address_space() });

            Self {
                ctx: context,
                id,
                tls: raw_ptr as usize,
                state: State::Ready,
                _stack: Some(stack),
                _tls_guard: Some(guard),
            }
        })
    }

    pub fn for_user(elf: elf::ElfBytes<AnyEndian>, elf_data: &[u8]) -> Result<Self, ElfLoadError> {
        log::debug!("loading ELF file");
        let loaded = load_elf(elf, elf_data)?;

        log::debug!("loaded ELF file, entry point: {:#x}", loaded.context.instruction_pointer());

        let (raw_ptr, guard) = unsafe { alloc_thread_self(loaded.context.stack_pointer() as *mut u8, loaded.stack_size) };

        log::debug!("created new user thread with TLS block at {:#x} (size: {})", raw_ptr as usize, loaded.stack_size);

        Ok(Self {
            ctx: loaded.context,
            id: ThreadID(NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            state: State::Ready,
            tls: raw_ptr as usize,
            _stack: None,
            _tls_guard: Some(guard),
        })
    }

    pub fn set_pid(&mut self, pid: ProcessID) {
        let ptr = self.tls as *mut ThreadSelf;
        unsafe {
            (*ptr).pid = Some(pid);
        }
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
        if let State::Sleeping(_) = self.state {
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

    // #[inline(always)]
    // #[track_caller]
    // pub fn dump_stack(&self) {
    //     log::debug!("Thread (id {}) stack: {:?} (size: {})", self.id.0, self._stack.ptr(), self._stack.size());
    // }
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
    scheduler::sleep_current(ms / 10);
}

pub fn id() -> ThreadID {
    current()
        .expect("should not retrieve thread id before initialization.")
        .id()
}

pub fn init() {
    SCHEDULER.init();
}

pub mod build;
pub mod tls;

pub use build::{ThreadBuilder, name, panicking, kill_safe, set_name, TPriority, set_kill_safe};
