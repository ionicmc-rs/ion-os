#![allow(missing_docs, clippy::missing_safety_doc, missing_debug_implementations)]
use core::{cell::UnsafeCell, sync::atomic::{AtomicBool, Ordering}};

use alloc::{collections::vec_deque::VecDeque};
use x86_64::instructions::interrupts;

use crate::{hlt_loop, serial_println, thread::{State, Thread, context::Context, stack::ThreadStack}};

/// The global Scheduler
/// 
/// this static handles all the running threads for this CPU (only CPU for now).
/// # Using this static
/// mostly, you will use the safe apis in [`thread`](crate::thread) rather than this static, as threads are currently very unstable.
pub static SCHEDULER: Scheduler = Scheduler::new();

#[derive(Debug, Default)]
pub struct Scheduler {
    queue: UnsafeCell<VecDeque<Thread>>,
    needs_resched: AtomicBool,
    current: UnsafeCell<Option<Thread>>
}

unsafe impl Sync for Scheduler {}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            queue: UnsafeCell::new(VecDeque::new()),
            needs_resched: AtomicBool::new(false),
            current: UnsafeCell::new(None)
        }
    }

    pub fn queue(&self, thread: Thread) {
        // Safety: scheduler may only be accessed synchronously.
        unsafe {
            (*self.queue.get()).push_back(thread);
        }
        serial_println!("queued");
    }

    pub fn request_rescheduling(&self) {
        self.needs_resched.store(true, Ordering::Relaxed);
    }

    pub fn tick(&self) {
        self.request_rescheduling();
    }

    /// Switches contexts
    /// 
    /// this does nothing if only the current thread is set.
    pub fn schedule(&self) {
        serial_println!("rescheduling");
        interrupts::without_interrupts(|| {
            unsafe {
                let queue = self.queue.get().as_mut().unwrap();
                if let Some(next) = self.pick_next() {
                    match self.current.get().as_mut().unwrap().take() { Some(mut cur) => {
                        let cur_ref = if cur.state() == State::Terminated {
                            serial_println!("removing terminated thread.");
                            // dropped after this.
                            &mut cur
                        } else {
                            serial_println!("reinserting thread");
                            queue.push_back(cur);
                            queue.back_mut().unwrap()
                        };
                        let cur_ctx = cur_ref.ctx_mut();

                        let new_ctx = next.ctx();
                        cur_ctx.switch_to(new_ctx);
                    } _ => {
                        serial_println!("switching from nothing");
                        extern "C" fn __unused(_: usize) -> ! {
                            hlt_loop()
                        }
                        let _stack = ThreadStack::of_size(1024);
                        let mut _ctx = Context::new(__unused, 0, _stack.ptr() as usize);
                        let new_ctx = next.ctx();
                        _ctx.switch_to(new_ctx);
                    }}
                }
            }
        })
    }

    fn pick_next(&self) -> Option<Thread> {
        let queue = unsafe { self.queue.get().as_mut().unwrap() };
        let len = queue.len();

        for _ in 0..len {
            let thread = queue.pop_front().unwrap();

            let runnable = match thread.state() {
                State::Ready => true,
                State::Blocked(ptr) => unsafe { *ptr },
                _ => false,
            };

            if runnable {
                serial_println!("found a ready thread.");
                // DO NOT reinsert — it is now the running thread
                return Some(thread);
            }

            // Not runnable → rotate to back
            queue.push_back(thread);
        }

        None
    }

    
    pub fn reschedule_if_needed(&self) {
        if self.needs_resched.swap(false, Ordering::Relaxed) {
            self.schedule();
        }
    }

    pub fn current(&self) -> Option<&Thread> {
        unsafe { 
            self
                .current
                .get()
                .as_ref()?
                // Convert `&Option<T>` to `Option<&T>`
                .as_ref() 
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn current_mut(&self) -> Option<&mut Thread> {
        unsafe  {
            self
                .current
                .get()
                .as_mut()?
                // Convert `&mut Option<T>` to `Option<&mut T>`
                .as_mut()
        }
    }

    pub fn last(&self) -> Option<&Thread> {
        let queue = unsafe { self.queue.get().as_ref().unwrap() };
        queue.back()
    }
}

pub fn yield_now() {
    serial_println!("yielding execution");
    SCHEDULER.schedule();
}

pub fn current() -> Option<&'static Thread> {
    SCHEDULER.current()
}

pub fn exit_current() {
    if let Some(v) = SCHEDULER.current_mut() {
        v.set_state(State::Terminated)
    }
    SCHEDULER.schedule();
}

