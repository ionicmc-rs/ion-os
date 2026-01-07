#![allow(missing_docs, clippy::missing_safety_doc, missing_debug_implementations)]
use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};

use alloc::{collections::vec_deque::VecDeque, vec::Vec};

use crate::thread::{State, Thread, ThreadID, tls::{ThreadSelf, install_tls}};

/// The global Scheduler
/// 
/// this static handles all the running threads for this CPU (only CPU for now).
/// # Using this static
/// mostly, you will use the safe apis in [`thread`](crate::thread) rather than this static, as threads are currently very unstable.
pub static SCHEDULER: Scheduler = Scheduler::new();

type ThreadQueue = VecDeque<Thread>;

#[derive(Debug, Default)]
struct ReadyQueue(ThreadQueue);

impl ReadyQueue {
    pub const fn new() -> Self {
        ReadyQueue(VecDeque::new())
    }

    pub fn queue(&mut self, thread: Thread) {
        if thread.state() != State::Ready {
            panic!("Safety invariant violated: `ReadyQueue::queue` must not be called with a thread that is not ready.");
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
    None
}

#[derive(Debug)]
struct NonRunningThread(Priority, Thread);

#[derive(Debug, Default)]
struct NonRunning(VecDeque<NonRunningThread>);

impl NonRunning {
    pub const fn new() -> Self {
        NonRunning(VecDeque::new())
    }

    pub fn queue(&mut self, thread: Thread, priority: Priority) {
        if !matches!(thread.state(), State::Sleeping(..) | State::Blocked(..) | State::Terminated) {
            panic!("Safety invariant violated: `NonRunning::queue` must never be called with a thread that is not sleeping, blocked or terminated.")
        }
        self.0.push_back(NonRunningThread(priority, thread));
    }

    pub fn queue_ref(&mut self, thread: Thread, priority: Priority) -> &mut Thread {
        self.queue(thread, priority);
        &mut self.0.back_mut().unwrap().1
    }
}

#[derive(Debug, Default)]
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
    panicked: Vec<ThreadID>
}

pub extern "C" fn idle_entry(_: usize) -> ! {
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
    Thread::new(idle_entry, 0, 4096)
}

impl Queues {
    pub const fn new() -> Self {
        Self { 
            high: ReadyQueue::new(), 
            med: ReadyQueue::new(), 
            low: ReadyQueue::new(), 
            non_running: NonRunning::new() ,
            idle: None,
            panicked: Vec::new()
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
            Priority::None => () // let thread drop
        }
    }

    pub fn place_idle(&mut self, idle: Thread) -> &mut Thread {
        if idle.state() != State::Ready && idle.id() != ThreadID(0) {
            panic!("Safety invariant violation: `Queues::place_idle` should only receive ready, 0 ID threads (idle)");
        }
        
        if self.idle.is_some() {
            panic!("Idle thread already exists, but the thread was already asserted to be the idle one. this likely indicates data being sent between CPUs"); // better error message for when we get CPUs
        }

        self.idle = Some(idle);
        self.idle.as_mut().unwrap()
    }

    pub fn set_panic(&mut self, id: ThreadID) {
        self.panicked.push(id);
    }

    pub fn has_panicked(&self, id: ThreadID) -> bool {
        self.panicked.contains(&id)
    }

    // waking sleeping/blocked threads

    pub fn restore_done(&mut self) {
        let len = self.non_running.0.len();
        
        for _ in 0..len {
            let NonRunningThread(priority, mut thread) = self.non_running.0.pop_front().unwrap();

            if priority == Priority::None {
                if thread.state() != State::Terminated {
                    panic!("Safety invariant violated: `Queues::restore_done` detected a non-terminated thread of `None` Priority")
                }
                drop(thread);
                continue; // the thread is terminated, we can drop and continue on.
            }

            if thread.should_wake() {
                thread.wake();
                self.queue(priority, thread);
            } else if let State::Blocked(ptr) = thread.state() {
                if unsafe { *ptr } {
                    self.queue(priority, thread);
                } 
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
                    return Some((t, $p))
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
    }
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
            current: None.into()
        }
    }

    pub fn init(&self) {
        let queues = unsafe_cell_ref!(mut self.queues);
        queues.init();
        *unsafe_cell_ref!(mut self.current) = Some((queues.take_idle().expect("queues should be initialized"), Priority::None));
        self.can_reschedule.store(true, Ordering::Relaxed);
    }

    /// Queues at medium priority
    pub fn queue(&self, thread: Thread) {
        // Safety: scheduler may only be accessed synchronously.
        unsafe_cell_ref!(mut self.queues).queue_med(thread);
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

    pub fn tick(&self) {
        self.request_rescheduling();
    }

    /// Switches contexts
    /// 
    /// interrupts are always enabled after this function is called.
    fn schedule(&self, reason: ScheduleReason) {
        let mut _buf = MaybeUninit::new(());
        if !self.can_reschedule.load(Ordering::Acquire) {
            return;
        }
        x86_64::instructions::interrupts::disable();
        let queues = unsafe_cell_ref!(mut self.queues);

        let current = unsafe_cell_ref!(mut self.current);
        if current.is_none() {
            panic!("use of scheduler before initialization");
        }
        // restore ready to run threads before checking anything, just in-case the timer missed anything
        queues.restore_done();

        let (next, next_priority) = match queues.next() { Some(t_and_p) => {
            (t_and_p.0, t_and_p.1)
        } _ => {
            match queues.take_idle() { Some(idle) => {
                (idle, Priority::None)
            } _ => {
                // we are already in the idle thread - we can simply return after re-enabling interrupts
                x86_64::instructions::interrupts::enable();
                return;
            }}
        }};

        let (mut old, old_priority) = current.replace((next, next_priority)).unwrap();
        let old_ref = match reason {
            ScheduleReason::Yield | ScheduleReason::Preempt => {
                match old_priority {
                    Priority::High => queues.queue_high_ref(old),
                    Priority::Med  => queues.queue_med_ref(old),
                    Priority::Low  => queues.queue_low_ref(old),
                    Priority::None => {
                        // it is an idle thread - we give it back to the queue list
                        queues.place_idle(old)
                    }
                }
            }
            ScheduleReason::Block(ptr) => {
                old.set_state(State::Blocked(ptr));
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
                queues.set_panic(old.id());
                queues.queue_norun_ref(Priority::None, old)
            }
        };
        
        let old_ctx = old_ref.ctx_mut();
        let new_ctx = current.as_ref().unwrap().0.ctx();
        let new_tls = current.as_ref().unwrap().0.tls_block();

        self.needs_resched.store(false, Ordering::Release);
        
        unsafe { install_tls(new_tls as *mut ThreadSelf) };

        // interrupts enabled here.
        x86_64::instructions::interrupts::enable();
        unsafe { old_ctx.switch_to(new_ctx) };
    }

    pub fn has_panicked(&self, id: ThreadID) -> bool {
        unsafe_cell_ref!(self.queues).has_panicked(id)
    }
    
    pub fn reschedule_if_needed(&self) {
        if unsafe_cell_ref!(self.current).is_none() {
            // Many places call this function before initialization having this return prevents countless checks
            return;
        }

        if self.needs_resched.swap(false, Ordering::Relaxed) {
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
    SCHEDULER.schedule(ScheduleReason::Exit);
}

pub fn sleep_current(ticks: u64) {
    SCHEDULER.schedule(ScheduleReason::Sleep(ticks));
}

pub fn block_until(ptr: *const bool) {
    SCHEDULER.schedule(ScheduleReason::Block(ptr));
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

#[allow(unused)]
enum ScheduleReason {
    Yield,
    Preempt,
    Exit,
    Block(*const bool),
    Sleep(u64), // wake tick
    Panicked
}

// ASM linkage.

