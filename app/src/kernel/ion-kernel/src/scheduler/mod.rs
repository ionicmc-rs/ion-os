#![allow(
    missing_docs,
    clippy::missing_safety_doc,
    missing_debug_implementations
)]
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::{boxed::Box, collections::vec_deque::VecDeque, vec::Vec};

use crate::{
    hlt_loop,
    interrupts::InterruptGuard,
    serial_println,
    thread::{
        self, State, Thread, ThreadID,
        build::TPriority,
        tls::{ThreadSelf, install_tls},
    },
};

/// The global Scheduler
///
/// this static handles all the running threads for this CPU (only CPU for now).
/// # Using this static
/// mostly, you will use the safe apis in [`thread`](crate::thread) rather than this static, as threads are currently very unstable.
pub static SCHEDULER: Scheduler = Scheduler::new();

type ThreadQueue = VecDeque<Thread>;

#[derive(Default)]
struct ReadyQueue(ThreadQueue);

impl ReadyQueue {
    pub const fn new() -> Self {
        ReadyQueue(VecDeque::new())
    }

    pub fn queue(&mut self, thread: Thread) {
        if let State::Ready = thread.state() {
        } else {
            panic!(
                "Safety invariant violated: `ReadyQueue::queue` must not be called with a thread that is not ready."
            );
        }
        self.0.push_back(thread);
    }

    pub fn queue_ref(&mut self, thread: Thread) -> &mut Thread {
        self.queue(thread);
        self.0.back_mut().unwrap()
    }

    pub fn next(&mut self) -> Option<Thread> {
        self.0.pop_front()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Priority {
    High,
    Med,
    Low,
    // used for idle and terminated threads
    None,
}

struct NonRunningThread(Priority, Thread);

#[derive(Default)]
struct NonRunning(VecDeque<NonRunningThread>);

impl NonRunning {
    pub const fn new() -> Self {
        NonRunning(VecDeque::new())
    }

    pub fn queue(&mut self, thread: Thread, priority: Priority) {
        if !matches!(
            thread.state(),
            State::Sleeping(..) | State::Blocked(..) | State::Terminated
        ) {
            panic!(
                "Safety invariant violated: `NonRunning::queue` must never be called with a thread that is not sleeping, blocked or terminated."
            )
        }
        self.0.push_back(NonRunningThread(priority, thread));
    }

    pub fn queue_ref(&mut self, thread: Thread, priority: Priority) -> &mut Thread {
        self.queue(thread, priority);
        &mut self.0.back_mut().unwrap().1
    }
}

#[derive(Default)]
struct Queues {
    // running queues
    // all threads in this category must have `State::Running`
    /// High priority threads
    ///
    /// this queue very unsafe to use - it starves all other (lower priority) threads until they are finished. so this should only
    /// really be used for one time `throw-away` threads that handle user-related operations (input, output, etc.)
    high: ReadyQueue,
    /// Medium priority threads
    ///
    /// this queue should only handle internal kernel operations, similar to the high priority queue, but for things users wont use.
    ///
    /// assuming the low priority queue is only for background work that is not always needed, this queue can safely have threads that loop
    /// for long period of time.
    med: ReadyQueue,
    /// Low priority threads
    ///
    /// this queue handles only background work - it runs very rarely and is almost always taken over my medium priority threads.
    ///
    /// this should never handle user operations.
    low: ReadyQueue,

    /// Non running threads
    ///
    /// these threads are not running, they are sleeping or blocking.
    ///
    /// this queue is reserved for threads that are currently awaiting something such as I/O operations, or user input.
    /// # Non running threads VS. Yielding threads
    /// some threads call [`yield_now`] in a loop, because they are awaiting a thread local operation - which we can't stop.
    ///
    /// this prevents optimization - but is necessary.
    non_running: NonRunning,
    /// Idle thread.
    ///
    /// here to prevent having to create a new one each time it is needed.
    ///
    /// this should never be dropped.
    idle: Option<Thread>,

    /// List of threads that have panicked.
    panicked: Vec<Thread>,
}

pub extern "C" fn idle_entry(_: usize) -> ! {
    // thread::set_name("idle");
    loop {
        yield_now();
    }
}

macro_rules! queue_fn {
    ($($name:ident, $name_ref:ident = $q:ident),*) => {
        $(
            fn $name(&mut self, thread: Thread) {
                self.$q.queue(thread);
            }

            fn $name_ref(&mut self, thread: Thread) -> &mut Thread {
                self.$q.queue_ref(thread)
            }
        )*
    };
}
fn idle_thread() -> Thread {
    Thread::new(idle_entry, 0, 16 * 1024)
}

impl Queues {
    pub const fn new() -> Self {
        Self {
            high: ReadyQueue::new(),
            med: ReadyQueue::new(),
            low: ReadyQueue::new(),
            non_running: NonRunning::new(),
            idle: None,
            panicked: Vec::new(),
        }
    }

    pub fn init(&mut self) {
        self.idle = Some(idle_thread());
    }

    // queueing

    queue_fn! {
        queue_high, queue_high_ref = high,
        queue_med, queue_med_ref = med,
        queue_low, queue_low_ref = low
    }

    #[allow(unused)] // only unused fn.
    pub fn queue_norun(&mut self, priority: Priority, thread: Thread) {
        self.non_running.queue(thread, priority);
    }

    pub fn queue_norun_ref(&mut self, priority: Priority, thread: Thread) -> &mut Thread {
        self.non_running.queue_ref(thread, priority)
    }

    pub fn queue(&mut self, priority: Priority, thread: Thread) {
        match priority {
            Priority::High => self.queue_high(thread),
            Priority::Med => self.queue_med(thread),
            Priority::Low => self.queue_low(thread),
            Priority::None => (), // let thread drop
        }
    }

    pub fn place_idle(&mut self, idle: Thread) -> &mut Thread {
        if let State::Ready = idle.state() {
        } else if idle.id() != ThreadID(0) {
            panic!(
                "Safety invariant violation: `Queues::place_idle` should only receive ready, 0 ID threads (idle)"
            );
        }

        if self.idle.is_some() {
            panic!(
                "Idle thread already exists, but the thread was already asserted to be the idle one. this likely indicates data being sent between CPUs"
            );
            // better error message for when we get CPUs
        }

        self.idle = Some(idle);
        self.idle.as_mut().unwrap()
    }

    pub fn set_panic(&mut self, thread: Thread) -> &mut Thread {
        self.panicked.push(thread);
        self.panicked.last_mut().unwrap()
    }

    pub fn remove_panic(&mut self, id: ThreadID) {
        let i = self
            .panicked
            .iter()
            .enumerate()
            .find_map(|(i, v)| (v.id() == id).then_some(i));
        if let Some(i) = i {
            self.panicked.remove(i);
        }
    }

    pub fn has_panicked(&self, id: ThreadID) -> bool {
        self.panicked.iter().any(|v| v.id() == id)
    }

    // waking sleeping/blocked threads

    pub fn restore_done(&mut self) {
        let len = self.non_running.0.len();

        for _ in 0..len {
            let NonRunningThread(priority, mut thread) = self.non_running.0.pop_front().unwrap();

            if thread.should_wake() {
                thread.wake();
                self.queue(priority, thread);
            } else if let State::Blocked(ev) = thread.state() {
                match ev {
                    WakeEvent::EndOf(_) => {
                        // only awoken by wake_waiters_of

                        self.non_running.queue(thread, priority);
                    }
                    WakeEvent::KeyInput => {
                        // only awoken by wake_kbd.

                        self.non_running.queue(thread, priority);
                    }
                    WakeEvent::Raw(ptr) => {
                        if unsafe { **ptr } {
                            thread.set_state(State::Ready);
                            self.queue(priority, thread);
                        } else {
                            self.non_running.queue(thread, priority);
                        }
                    }
                    WakeEvent::FnYieldTrue(func) => {
                        if func() {
                            thread.set_state(State::Ready);
                            self.queue(priority, thread);
                        } else {
                            self.non_running.queue(thread, priority);
                        }
                    }
                    WakeEvent::Atomic(locked) => {
                        let is_locked = locked.load(Ordering::Relaxed);
                        if !is_locked {
                            thread.set_state(State::Ready);
                            self.queue(priority, thread);
                        } else {
                            self.non_running.queue(thread, priority);
                        }
                    }
                }
            } else if priority == Priority::None {
                if let State::Terminated = thread.state() {
                } else {
                    panic!(
                        "Safety invariant violated: `Queues::restore_done` detected a non-terminated thread of `None` Priority"
                    )
                }
                drop(thread);
                continue; // the thread is terminated, we can drop and continue on.
            } else {
                self.non_running.queue(thread, priority);
            }
        }
    }

    // getting values

    pub fn next(&mut self) -> Option<(Thread, Priority)> {
        macro_rules! check_queue {
            ($q:expr_2021, $p:expr_2021) => {
                if let Some(t) = $q.next() {
                    return Some((t, $p));
                }
            };
        }
        check_queue!(self.high, Priority::High);
        check_queue!(self.med, Priority::Med);
        check_queue!(self.low, Priority::Low);
        None
    }

    pub fn take_idle(&mut self) -> Option<Thread> {
        self.idle.take()
    }

}

macro_rules! unsafe_cell_ref {
    ($cell:expr_2021) => {
        unsafe { $cell.get().as_ref().unwrap() }
    };
    (mut $cell:expr_2021) => {
        unsafe { $cell.get().as_mut().unwrap() }
    };
}

#[derive(Debug, Default)]
pub struct Scheduler {
    queues: UnsafeCell<Queues>,
    needs_resched: AtomicBool,
    current: UnsafeCell<Option<(Thread, Priority)>>,
    can_reschedule: AtomicBool,
}

unsafe impl Sync for Scheduler {}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            queues: Queues::new().into(),
            needs_resched: AtomicBool::new(false),
            can_reschedule: AtomicBool::new(false),
            current: None.into(),
        }
    }

    pub fn init(&self) {
        let queues = unsafe_cell_ref!(mut self.queues);
        queues.init();
        let idle = queues
            .take_idle()
            .expect("queues uninit after explicit init - likely a memory error");
        idle.dump_stack();
        *unsafe_cell_ref!(mut self.current) = Some((idle, Priority::None));

        self.can_reschedule.store(true, Ordering::Relaxed);
    }

    /// Queues at medium priority
    pub fn queue(&self, thread: Thread) {
        // Safety: scheduler may only be accessed synchronously.
        unsafe_cell_ref!(mut self.queues).queue_med(thread);
    }

    pub fn queue_at(&self, prio: TPriority, thread: Thread) {
        let prio = match prio {
            TPriority::High => Priority::High,
            TPriority::Low => Priority::Low,
            TPriority::Med => Priority::Med,
        };
        unsafe_cell_ref!(mut self.queues).queue(prio, thread);
    }

    pub fn request_rescheduling(&self) {
        if self.can_reschedule.load(Ordering::Relaxed) {
            self.needs_resched.store(true, Ordering::Relaxed);
        }
    }

    pub fn critical_moment_raw<R>(&self, f: impl FnOnce() -> R, result_buf: &mut MaybeUninit<R>) {
        let cr = self.can_reschedule.swap(false, Ordering::Acquire);
        let nr = self.needs_resched.swap(false, Ordering::Acquire);

        let res = f();
        result_buf.write(res);

        self.can_reschedule.store(cr, Ordering::Release);
        self.needs_resched.store(nr, Ordering::Release);
    }

    pub fn critical_moment<R>(&self, f: impl FnOnce() -> R, result_buf: &mut MaybeUninit<R>) {
        self.critical_moment_raw(f, result_buf);
        // here is a good place to check if we can reschedule, we know threads cant be used from interrupts.
        self.reschedule_if_needed();
    }

    pub fn wake_kbd(&self) {
        let queues = unsafe_cell_ref!(mut self.queues);
        let len = queues.non_running.0.len();
        for _ in 0..len {
            let NonRunningThread(priority, mut thread) = queues.non_running.0.pop_front().unwrap();
            match thread.state() {
                State::Ready | State::Running => panic!(
                    "Safety invariant violated: detected a ready/running thread in the non running queue"
                ),
                State::Blocked(WakeEvent::KeyInput) => {
                    thread.set_state(State::Ready);
                    queues.queue(priority, thread);
                }
                _ => queues.queue_norun(priority, thread),
            }
        }
    }

    pub fn wake_waiters_of(&self, id: ThreadID) {
        let queues = unsafe_cell_ref!(mut self.queues);
        let len = queues.non_running.0.len();
        for _ in 0..len {
            let NonRunningThread(priority, mut thread) = queues.non_running.0.pop_front().unwrap();
            match thread.state() {
                State::Ready | State::Running => {
                    panic!("Safety invariant violated: detected a thread in the non running queue")
                }
                State::Blocked(WakeEvent::EndOf(waited)) if *waited == id => {
                    thread.set_state(State::Ready);
                    queues.queue(priority, thread);
                }
                _ => queues.queue_norun(priority, thread),
            }
        }
    }

    /// Switches contexts
    ///
    /// interrupts are always enabled after this function is called.
    #[track_caller] // very useful for panics
    fn schedule(&self, reason: ScheduleReason) {
        let mut _buf = MaybeUninit::new(());
        if !self.can_reschedule.load(Ordering::Acquire) {
            serial_println!("cannot reschedule");
            return;
        }
        let _guard = InterruptGuard::new();
        let queues = unsafe_cell_ref!(mut self.queues);

        let current = unsafe_cell_ref!(mut self.current);
        if current.is_none() {
            panic!("use of scheduler before initialization");
        }
        // restore ready to run threads before checking anything, just in-case the timer missed anything
        queues.restore_done();

        let (next, next_priority) = match queues.next() {
            Some(t_and_p) => (t_and_p.0, t_and_p.1),
            _ => {
                match reason {
                    ScheduleReason::Yield | ScheduleReason::Preempt => {
                        return; // no other threads - safely continue
                    }
                    ScheduleReason::Exit
                    | ScheduleReason::Block(_)
                    | ScheduleReason::Sleep(_)
                    | ScheduleReason::Panicked => match queues.take_idle() {
                        Some(idle) => (idle, Priority::None),
                        None => panic!("idle thread must never exit."),
                    },
                    ScheduleReason::Start => {
                        panic!("Started scheduler with no available threads");
                    }
                }
            }
        };
        let mut no_store = false;
        let (mut old, old_priority) = current.replace((next, next_priority)).unwrap();
        let old_ref = match reason {
            ScheduleReason::Yield | ScheduleReason::Preempt => {
                match old_priority {
                    Priority::High => queues.queue_high_ref(old),
                    Priority::Med => queues.queue_med_ref(old),
                    Priority::Low => queues.queue_low_ref(old),
                    Priority::None => {
                        // it is an idle thread - we give it back to the queue list
                        queues.place_idle(old)
                    }
                }
            }
            ScheduleReason::Block(ev) => {
                old.set_state(State::Blocked(ev));
                queues.queue_norun_ref(old_priority, old)
            }
            ScheduleReason::Sleep(ticks) => {
                old.set_sleep(ticks);
                queues.queue_norun_ref(old_priority, old)
            }
            ScheduleReason::Exit => {
                old.set_state(State::Terminated);
                queues.queue_norun_ref(Priority::None, old)
            }
            ScheduleReason::Panicked => {
                old.set_state(State::Terminated);
                no_store = true;
                queues.set_panic(old)
            }
            ScheduleReason::Start => {
                if old_priority != Priority::None {
                    panic!("threads must only start when the current thread is the idle thread.")
                }
                no_store = true;
                // this ref should not actually be used.
                queues.place_idle(old)
            }
        };

        let old_ctx = old_ref.ctx_mut();
        let new_ctx = current.as_ref().unwrap().0.ctx();
        let new_tls = current.as_ref().unwrap().0.tls_block();

        self.needs_resched.store(false, Ordering::Release);

        unsafe { install_tls(new_tls as *mut ThreadSelf) };

        current.as_ref().unwrap().0.dump_stack();

        // interrupts enabled here.
        drop(_guard);

        if no_store {
            unsafe { new_ctx.load() };
        } else {
            unsafe { old_ctx.switch_to(new_ctx) };
        }
    }

    pub fn has_panicked(&self, id: ThreadID) -> bool {
        unsafe_cell_ref!(self.queues).has_panicked(id)
    }

    pub fn remove_panic(&self, id: ThreadID) {
        unsafe_cell_ref!(mut self.queues).remove_panic(id);
    }

    pub fn reschedule_if_needed(&self) {
        if unsafe_cell_ref!(self.current).is_none() {
            // Many places call this function before initialization having this return prevents countless checks
            return;
        }

        if self.needs_resched.swap(false, Ordering::Release) {
            self.schedule(ScheduleReason::Preempt);
        }
    }

    pub fn current(&self) -> Option<&Thread> {
        unsafe_cell_ref!(self.current).as_ref().map(|(t, _)| t)
    }

    #[allow(clippy::mut_from_ref)]
    pub fn current_mut(&self) -> Option<&mut Thread> {
        unsafe_cell_ref!(mut self.current).as_mut().map(|(t, _)| t)
    }
}

pub fn current() -> Option<&'static Thread> {
    SCHEDULER.current()
}

pub fn exit_current() {
    if let Some(v) = SCHEDULER.current_mut() {
        v.set_state(State::Terminated)
    }
    wake_waiters_of(thread::id());
    SCHEDULER.schedule(ScheduleReason::Exit);
}

pub fn sleep_current(ticks: u64) {
    SCHEDULER.schedule(ScheduleReason::Sleep(ticks));
}

pub fn block_until(ptr: *const bool) {
    SCHEDULER.schedule(ScheduleReason::Block(WakeEvent::Raw(ptr)));
}

pub fn block_until_end_of(id: ThreadID) {
    SCHEDULER.schedule(ScheduleReason::Block(WakeEvent::EndOf(id)));
}

pub fn block_until_key_input() {
    SCHEDULER.schedule(ScheduleReason::Block(WakeEvent::KeyInput));
}

pub fn block_until_true(f: impl Fn() -> bool + 'static) {
    SCHEDULER.schedule(ScheduleReason::Block(WakeEvent::FnYieldTrue(Box::new(f))));
}

pub fn atomic_block_until(atomic: &'static AtomicBool) {
    SCHEDULER.schedule(ScheduleReason::Block(WakeEvent::Atomic(atomic)));
}

pub fn yield_now() {
    SCHEDULER.schedule(ScheduleReason::Yield);
}

pub fn request_reschedule() {
    SCHEDULER.request_rescheduling();
}

pub fn wake_done() {
    unsafe_cell_ref!(mut SCHEDULER.queues).restore_done();
}

pub fn reschedule_if_needed() {
    SCHEDULER.reschedule_if_needed();
}

pub fn critical_moment<R>(f: impl Fn() -> R) -> R {
    let mut buf = MaybeUninit::uninit();
    SCHEDULER.critical_moment(f, &mut buf);

    // Safety: scheduler fills it
    unsafe { buf.assume_init() }
}

pub fn critical_moment_raw<R>(f: impl Fn() -> R) -> R {
    let mut buf = MaybeUninit::uninit();
    SCHEDULER.critical_moment_raw(f, &mut buf);

    // Safety: scheduler fills it
    unsafe { buf.assume_init() }
}

pub fn has_panicked(id: ThreadID) -> bool {
    SCHEDULER.has_panicked(id)
}

pub unsafe fn start() {
    SCHEDULER.schedule(ScheduleReason::Start);
}

pub unsafe fn remove_panic_id(id: ThreadID) {
    SCHEDULER.remove_panic(id);
}

pub unsafe fn panic_current() -> ! {
    wake_waiters_of(thread::id());
    SCHEDULER.schedule(ScheduleReason::Panicked);
    hlt_loop()
}

pub unsafe fn wake_kbd() {
    SCHEDULER.wake_kbd();
}

fn wake_waiters_of(id: ThreadID) {
    serial_println!("waking waiters of id {id:?}");
    SCHEDULER.wake_waiters_of(id);
}
#[allow(unused)]
enum ScheduleReason {
    Yield,
    Preempt,
    Exit,
    Block(WakeEvent),
    Sleep(u64), // wake tick
    Panicked,
    Start,
}

pub enum WakeEvent {
    Raw(*const bool),
    EndOf(ThreadID),
    KeyInput,
    FnYieldTrue(Box<dyn Fn() -> bool>),
    // only supports static lifetimes for now.
    Atomic(&'static AtomicBool),
}

// ASM linkage.
