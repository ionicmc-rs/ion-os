use core::sync::atomic::{AtomicUsize, Ordering};

use crate::process::ProcessID;
use crate::thread::ThreadID;

const EVENT_CAP: usize = 1024;

static HEAD: AtomicUsize = AtomicUsize::new(0);

static mut EVENTS: [Option<Event>; EVENT_CAP] = [None; EVENT_CAP];

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum EventLevel {
    Info,
    Trace,
    Warning,
    Error,
    Fatal,
}

#[derive(Clone, Copy)]
pub enum EventData {
    Process(ProcessEvent),
    Thread(ThreadEvent),
    Syscall(SyscallEvent),
    Memory(MemoryEvent),
}

#[derive(Clone, Copy)]
pub enum ProcessEvent {
    Created(ProcessID),
    Exited(ProcessID),
}

#[derive(Clone, Copy)]
pub enum ThreadEvent {
    Created(ThreadID),
    Exited(ThreadID),
    Blocked(ThreadID),
    Unblocked(ThreadID),
}

#[derive(Clone, Copy)]
pub enum SyscallEvent {
    Enter {
        group: u32,
        command: u32,
    },
    Exit {
        result: u64,
    },
}

#[derive(Clone, Copy)]
pub enum MemoryEvent {
    PageFault {
        addr: u64,
    },
}

#[derive(Clone, Copy)]
pub struct Event {
    pub ticks: u64,
    pub cpu: u16,
    pub level: EventLevel,
    pub data: EventData,
}

/// Push an event into the ring buffer.
/// Old events will be overwritten when the buffer is full.
#[inline]
pub fn push_event(ev: Event) {
    let idx = HEAD.fetch_add(1, Ordering::Relaxed) % EVENT_CAP;

    unsafe {
        EVENTS[idx] = Some(ev);
    }
}

/// Pop the next available event.
///
/// This is intended for a consumer like a userspace logger.
/// Not strictly FIFO if producers race, but sufficient for logging.
pub fn pop_event() -> Option<Event> {
    static TAIL: AtomicUsize = AtomicUsize::new(0);

    let tail = TAIL.load(Ordering::Relaxed);
    let head = HEAD.load(Ordering::Acquire);

    if tail == head {
        return None;
    }

    let idx = tail % EVENT_CAP;

    let ev = unsafe { EVENTS[idx].take() };

    TAIL.store(tail + 1, Ordering::Release);

    ev
}