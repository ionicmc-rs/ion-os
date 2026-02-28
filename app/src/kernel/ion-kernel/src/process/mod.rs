use core::{cell::Cell, fmt::Debug};
use alloc::{string::String, vec::Vec, vec};

use crate::{
    privilege::UserId, serial_println, thread::{self, ThreadID, sync::Mutex, tls::tcb}
};

pub mod elf;

pub fn current_process() -> Option<ProcessID> {
    tcb().pid
}

/// Unique identifier for a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct ProcessID(pub u64);

/// State of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Zombie,
}

/// Represents a kernel-side process.
#[derive(Debug)]
pub struct Process {
    pub pid: ProcessID,
    pub parent: Option<ProcessID>,
    pub threads: Vec<ThreadID>,
    pub owner: UserId,
    pub name: String,
    pub children: Vec<ProcessID>,
    pub state: ProcessState,
}

/// Global process manager.
pub struct ProcManager {
    processes: Cell<Vec<Process>>,
    highest: Cell<ProcessID>,
    free_ids: Cell<Vec<ProcessID>>,
}

impl Debug for ProcManager {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let taken = self.processes.take();
        let mut dstruct = f.debug_struct("ProcessManager");
        
        for x in &taken {
            dstruct.field(&x.name, x);
        }

        dstruct.finish()
    }
}

/// Global process table.
pub static PROCESSES: Mutex<ProcManager> = Mutex::new(ProcManager {
    processes: Cell::new(Vec::new()),
    highest: Cell::new(ProcessID(0)),
    free_ids: Cell::new(Vec::new()),
});

impl ProcManager {
    /// Allocate a new PID (reusing freed ones if possible).
    fn alloc_pid(&self) -> ProcessID {
        let mut free = self.free_ids.take();
        let pid = if let Some(pid) = free.pop() {
            pid
        } else {
            let pid = self.highest.get();
            self.highest.set(ProcessID(pid.0 + 1));
            pid
        };
        self.free_ids.set(free);
        pid
    }

    /// Free a PID for reuse.
    fn free_pid(&self, pid: ProcessID) {
        let mut free = self.free_ids.take();
        free.push(pid);
        self.free_ids.set(free);
    }

    pub fn with<R>(&self, pid: ProcessID, f: impl FnOnce(&Process) -> R) -> Option<R> {
        let processes = self.processes.take();
        let res = processes.iter().find(|p| p.pid == pid).map(f);
        self.processes.set(processes);
        res
    }

    pub fn with_mut<R>(&self, pid: ProcessID, f: impl FnOnce(&mut Process) -> R) -> Option<R> {
        let mut processes = self.processes.take();
        let res = processes.iter_mut().find(|p| p.pid == pid).map(f);
        self.processes.set(processes);
        res
    }
}

/// Spawn a new kernel-side process.
///
/// User-mode execution is blocked until ELF loading exists.
pub fn spawn_proc(
    name: impl Into<String>,
    owner: UserId,
    main_thread: ThreadID,
    parent: Option<ProcessID>,
) -> ProcessID {
    let lock = PROCESSES.lock();

    let pid = thread::critical_moment(|| lock.alloc_pid());

    thread::critical_moment(|| {
        let mut processes = lock.processes.take();

        // Link parent → child
        if let Some(ppid) = parent {
            if let Some(p) = processes.iter_mut().find(|p| p.pid == ppid) {
                p.children.push(pid);
            }
        }

        processes.push(Process {
            pid,
            parent,
            threads: vec![main_thread],
            owner,
            name: name.into(),
            children: Vec::new(),
            state: ProcessState::Running,
        });

        lock.processes.set(processes);
    });

    pid
}

/// Exit a process.
///
/// - Terminates all threads
/// - Reparents children
/// - Marks zombie
/// - Frees PID
/// 
/// This fails if the current thread is part of this process.
pub fn exit_proc(pid: ProcessID) {
    let lock = PROCESSES.lock();

    if thread::critical_moment(|| {
        let mut processes = lock.processes.take();

        let Some(idx) = processes.iter().position(|p| p.pid == pid) else {
            lock.processes.set(processes);
            return false;
        };

        let parent = processes[idx].parent;
        let children = core::mem::take(&mut processes[idx].children);
        let threads = core::mem::take(&mut processes[idx].threads);

        let mut should_exit = false;
        // Kill threads
        for tid in threads {
            if tid == thread::id() {
                should_exit = true;
                continue;
            }
            unsafe { thread::kill(tid) };
        }
        
        // Reparent children
        if let Some(ppid) = parent {
            if let Some(p) = processes.iter_mut().find(|p| p.pid == ppid) {
                p.children.extend(children);
            }
        }

        processes[idx].state = ProcessState::Zombie;

        let pid = processes[idx].pid;
        processes.remove(idx);
        lock.free_pid(pid);

        lock.processes.set(processes);
        should_exit
    }) {
        thread::exit_current()
    }
}

pub fn dump() {
    serial_println!("{:#?}", PROCESSES);
}