use core::{cell::Cell, fmt::Debug};

use alloc::{boxed::Box, format, string::String, vec, vec::Vec};

use crate::{
    asyncio, io::{self, ErrorKind, OpenOptions}, log, privilege::UserID, resources::{self, FdDispatch, OpenFlags, ResourceTable, channel, fd}, scheduler::current_scheduler, serial_println, thread::{self, Thread, ThreadID, sync::Mutex, tls::tcb}
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
    Zombie(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildStreamDirection {
    /// Parent gets readable endpoint, child gets writable endpoint.
    ParentReads,
    /// Parent gets writable endpoint, child gets readable endpoint.
    ParentWrites,
}

#[derive(Debug, Clone, Copy)]
struct ChildFdTemplate {
    child_fd: u32,
    global_fd: u32,
}

#[derive(Debug)]
pub struct Resources {
    local_fd_mappings: ResourceTable<FdHandle>,
    inheritable_fds: Vec<u32>,
    child_fd_templates: Vec<ChildFdTemplate>,
}

#[derive(Debug)]
pub struct FdHandle(pub u32);

impl FdDispatch for FdHandle {
    fn read(&mut self, buf: &mut [u8]) -> crate::io::Result<u64> {
        fd::read(self.0, buf)
    }

    fn write(&mut self, data: &[u8]) -> crate::io::Result<u64> {
        fd::write(self.0, data)
    }

    fn seek(&mut self, pos: u64) -> crate::io::Result<u64> {
        fd::seek(self.0, pos)
    }

    fn seek_relative(&mut self, delta: i64) -> crate::io::Result<u64> {
        fd::seek_relative(self.0, delta)
    }

    fn rewind(&mut self) {
        _ = fd::rewind(self.0);
    }

    fn tell(&self) -> crate::io::Result<u64> {
        fd::tell(self.0)
    }

    fn flags(&self) -> crate::resources::OpenFlags {
        fd::info(self.0).expect("could not get open flags").2
    }

    fn is_readable(&self) -> bool {
        self.flags().contains(OpenFlags::READ)
    }

    fn is_writable(&self) -> bool {
        self.flags().contains(OpenFlags::WRITE)
    }

    fn truncate(&mut self, size: u64) -> crate::io::Result<()> {
        fd::truncate(self.0, size)
    }

    fn close(self: alloc::boxed::Box<Self>) -> crate::io::Result<()> {
        fd::close(self.0)
    }

    fn metadata(&self) -> crate::io::Result<(u64, u64, crate::resources::OpenFlags)> {
        fd::info(self.0)
    }
}

impl Resources {
    pub fn new() -> Self {
        Self {
            local_fd_mappings: ResourceTable::new(),
            inheritable_fds: Vec::new(),
            child_fd_templates: Vec::new(),
        }
    }

    pub fn alloc_fd(&mut self, fd: u32) -> io::Result<u32> {
        self.local_fd_mappings.alloc(FdHandle(fd))
    }

    pub fn close_local(&mut self, local_fd: u32) -> io::Result<()> {
        self.inheritable_fds.retain(|&fd| fd != local_fd);
        self.local_fd_mappings.close(local_fd)
    }

    pub fn close_all(&mut self) {
        self.inheritable_fds.clear();
        self.local_fd_mappings.close_all();
        for template in self.child_fd_templates.drain(..) {
            let _ = resources::fd::close(template.global_fd);
        }
    }

    pub fn read_local(&mut self, local_fd: u32, buf: &mut [u8]) -> io::Result<u64> {
        self.local_fd_mappings.read(local_fd, buf)
    }

    pub fn write_local(&mut self, local_fd: u32, data: &[u8]) -> io::Result<u64> {
        self.local_fd_mappings.write(local_fd, data)
    }

    fn global_for_local(&self, local_fd: u32) -> io::Result<u32> {
        let global_fd = self
            .local_fd_mappings
            .get(local_fd)?
            .with(|handle| handle.0);
        Ok(global_fd)
    }

    fn install_at_local_fd(&mut self, local_fd: u32, global_fd: u32) -> io::Result<()> {
        let tmp = self.alloc_fd(global_fd)?;
        if let Err(e) = self.local_fd_mappings.dup2(tmp, local_fd) {
            let _ = self.local_fd_mappings.close(tmp);
            return Err(e);
        }
        if tmp != local_fd {
            self.local_fd_mappings.close(tmp)?;
        }
        Ok(())
    }

    pub fn set_inheritable(&mut self, local_fd: u32, inheritable: bool) -> io::Result<()> {
        if inheritable {
            if !self.local_fd_mappings.is_valid(local_fd) {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "attempted to mark an invalid local file descriptor as inheritable",
                ));
            }
            if !self.inheritable_fds.iter().any(|&fd| fd == local_fd) {
                self.inheritable_fds.push(local_fd);
            }
        } else {
            self.inheritable_fds.retain(|&fd| fd != local_fd);
        }

        Ok(())
    }

    fn clone_inheritable_globals(&self) -> io::Result<Vec<(u32, u32)>> {
        let mut cloned = Vec::new();

        let result = (|| -> io::Result<()> {
            for &local_fd in &self.inheritable_fds {
                if !self.local_fd_mappings.is_valid(local_fd) {
                    continue;
                }

                let global_fd = self.global_for_local(local_fd)?;
                let duplicated_global = resources::fd::dup(global_fd)?;
                cloned.push((local_fd, duplicated_global));
            }
            Ok(())
        })();

        if let Err(e) = result {
            for &(_, global_fd) in &cloned {
                let _ = resources::fd::close(global_fd);
            }
            return Err(e);
        }

        Ok(cloned)
    }

    /// Build a child resource table containing this process's inheritable FDs.
    /// Each entry gets its own global descriptor reference while keeping the
    /// same local descriptor number in the child.
    pub fn inherit_fds(&self) -> io::Result<ResourceTable<FdHandle>> {
        let inherited = self.clone_inheritable_globals()?;
        let mut child = Resources::new();

        if let Err(error) = child.inherit_globals(inherited) {
            child.close_all();
            return Err(error);
        }

        Ok(child.local_fd_mappings)
    }

    fn clone_child_templates(&self) -> io::Result<Vec<(u32, u32)>> {
        let mut cloned = Vec::new();

        let result = (|| -> io::Result<()> {
            for template in &self.child_fd_templates {
                let duplicated_global = resources::fd::dup(template.global_fd)?;
                cloned.push((template.child_fd, duplicated_global));
            }
            Ok(())
        })();

        if let Err(e) = result {
            for &(_, global_fd) in &cloned {
                let _ = resources::fd::close(global_fd);
            }
            return Err(e);
        }

        Ok(cloned)
    }

    fn inherit_globals(&mut self, inherited: Vec<(u32, u32)>) -> io::Result<()> {
        let mut iter = inherited.into_iter();

        while let Some((local_fd, global_fd)) = iter.next() {
            if let Err(e) = self.install_at_local_fd(local_fd, global_fd) {
                let _ = resources::fd::close(global_fd);
                for (_, remaining_global_fd) in iter {
                    let _ = resources::fd::close(remaining_global_fd);
                }
                return Err(e);
            }
            self.set_inheritable(local_fd, true)?;
        }

        Ok(())
    }

    fn set_child_template_global(&mut self, child_fd: u32, global_fd: u32) {
        if let Some(existing) = self
            .child_fd_templates
            .iter_mut()
            .find(|template| template.child_fd == child_fd)
        {
            let old_global = existing.global_fd;
            existing.global_fd = global_fd;
            let _ = resources::fd::close(old_global);
            return;
        }

        self.child_fd_templates.push(ChildFdTemplate {
            child_fd,
            global_fd,
        });
    }

    fn remove_child_template(&mut self, child_fd: u32) {
        if let Some(idx) = self
            .child_fd_templates
            .iter()
            .position(|template| template.child_fd == child_fd)
        {
            let removed = self.child_fd_templates.remove(idx);
            let _ = resources::fd::close(removed.global_fd);
        }
    }
}

/// Represents a kernel-side process.
#[derive(Debug)]
pub struct Process {
    pub pid: ProcessID,
    pub parent: Option<ProcessID>,
    pub threads: Vec<ThreadID>,
    pub owner: UserID,
    pub name: String,
    pub children: Vec<ProcessID>,
    pub state: ProcessState,
    pub resources: Resources,
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

/// Spawn a new process - assuming its components already exist
pub fn spawn_proc(
    name: impl Into<String>,
    owner: UserID,
    main_thread: ThreadID,
    parent: Option<ProcessID>,
) -> ProcessID {
    let lock = PROCESSES.lock();

    let pid = thread::critical_moment(|| lock.alloc_pid());

    thread::critical_moment(|| {
        let mut processes = lock.processes.take();
        let mut inherited_globals = Vec::new();

        // Link parent -> child and clone inheritable parent FDs.
        if let Some(ppid) = parent {
            if let Some(p) = processes.iter_mut().find(|p| p.pid == ppid) {
                p.children.push(pid);
                match p.resources.clone_inheritable_globals() {
                    Ok(cloned) => inherited_globals = cloned,
                    Err(e) => {
                        log::error!("failed to clone inheritable FDs from parent {ppid:?}: {e}")
                    }
                }

                match p.resources.clone_child_templates() {
                    Ok(cloned) => inherited_globals.extend(cloned),
                    Err(e) => {
                        log::error!("failed to clone child FD templates from parent {ppid:?}: {e}")
                    }
                }
            }
        }

        let mut resources = Resources::new();
        if let Err(e) = resources.inherit_globals(inherited_globals) {
            log::error!("failed to install inherited FDs for child {pid:?}: {e}");
        }

        processes.push(Process {
            pid,
            parent,
            threads: vec![main_thread],
            owner,
            name: name.into(),
            children: Vec::new(),
            state: ProcessState::Running,
            resources,
        });

        lock.processes.set(processes);
    });

    pid
}

pub async fn exec(owner: UserID, path: &str, args: &[&str], parent: Option<ProcessID>, local_fds: ResourceTable<FdHandle>) -> io::Result<ProcessID> {
    // get the ELF first

    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .await?;

    let meta = file.metadata().await?;

    if meta.is_dir() {
        return Err(io::Error::new(
            ErrorKind::IsADirectory,
            "cannot execute a directory"
        ));
    }

    if meta.is_empty() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "cannot execute an empty file"
        ));
    }

    let file_bytes = file.read_all().await?;

    let parsed = ::elf::ElfBytes::minimal_parse(&file_bytes).map_err(
        |e| io::Error::new(ErrorKind::InvalidData, e)
    )?;

    let mut main = Thread::for_user(parsed, &file_bytes, args)
        .map_err(io::Error::other)?;

    // actual spawn

    let pid = spawn_proc(path, owner, main.id(), parent);

    PROCESSES
        .lock()
        .with_mut(pid, |proc| {
            proc.resources = Resources {
                local_fd_mappings: local_fds,
                inheritable_fds: Vec::new(),
                child_fd_templates: Vec::new(),
            };
        });

    if pid.0 != 0 && parent.is_none() {
        panic!("only the init process can be an orphan process")
    }

    main.set_pid(pid);
    main.set_name(&format!("{owner:?} <{path}> {args:?} - child of {parent:?}"));

    current_scheduler().queue(main);

    Ok(pid)
}

/// Exit a process.
///
/// - Terminates all threads
/// - Reparents children
/// - Marks zombie
/// - Frees PID
///
/// This fails if the current thread is part of this process.
pub fn exit_proc(pid: ProcessID, exit_code: i32) {
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

        processes[idx].resources.close_all();
        processes[idx].state = ProcessState::Zombie(exit_code);
        lock.free_pid(pid);

        lock.processes.set(processes);
        should_exit
    }) {
        thread::exit_current()
    }
}

pub fn with_proc<R>(pid: ProcessID, f: impl FnOnce(&Process) -> R) -> Option<R> {
    PROCESSES.lock().with(pid, f)
}

pub fn with_proc_mut<R>(pid: ProcessID, f: impl FnOnce(&mut Process) -> R) -> Option<R> {
    PROCESSES.lock().with_mut(pid, f)
}

pub fn with_current_proc<R>(f: impl FnOnce(&Process) -> R) -> Option<R> {
    let pid = current_process()?;
    with_proc(pid, f)
}

pub fn with_current_proc_mut<R>(f: impl FnOnce(&mut Process) -> R) -> Option<R> {
    let pid = current_process()?;
    with_proc_mut(pid, f)
}

pub fn dump() {
    serial_println!("{:#?}", PROCESSES);
}

impl Process {
    pub fn open_resource(&mut self, resource: Box<dyn FdDispatch + 'static>) -> io::Result<u32> {
        let global = fd::alloc(resource)?;
        match self.resources.alloc_fd(global) {
            Ok(local) => Ok(local),
            Err(e) => {
                let _ = fd::close(global);
                Err(e)
            }
        }
    }
    pub fn close_resource(&mut self, local_fd: u32) -> io::Result<()> {
        let global_fd = self.resources.global_for_local(local_fd)?;
        self.resources.close_local(local_fd)?;
        fd::close(global_fd)
    }
    pub fn seek_resource(&mut self, local_fd: u32, pos: u64) -> io::Result<u64> {
        self.resources.local_fd_mappings.seek(local_fd, pos)
    }
}

pub fn write_proc_fd(pid: ProcessID, fd: u32, data: &[u8]) -> io::Result<u64> {
    let lock = PROCESSES.lock();
    thread::critical_moment(|| {
        let mut processes = lock.processes.take();
        let res = match processes.iter_mut().find(|p| p.pid == pid) {
            Some(process) => process.resources.write_local(fd, data),
            None => Err(io::Error::new(ErrorKind::NotFound, "process not found")),
        };
        lock.processes.set(processes);
        res
    })
}

pub fn read_proc_fd(pid: ProcessID, fd: u32, buf: &mut [u8]) -> io::Result<u64> {
    let lock = PROCESSES.lock();
    thread::critical_moment(|| {
        let mut processes = lock.processes.take();
        let res = match processes.iter_mut().find(|p| p.pid == pid) {
            Some(process) => process.resources.read_local(fd, buf),
            None => Err(io::Error::new(ErrorKind::NotFound, "process not found")),
        };
        lock.processes.set(processes);
        res
    })
}

pub fn write_current_fd(fd: u32, data: &[u8]) -> io::Result<u64> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    write_proc_fd(pid, fd, data)
}

pub fn read_current_fd(fd: u32, buf: &mut [u8]) -> io::Result<u64> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    read_proc_fd(pid, fd, buf)
}

/// Open a ring-buffer stream pair in a process as regular local FDs.
///
/// Returns `(read_fd, write_fd)`.
pub fn open_stream_pair_for_proc(pid: ProcessID, capacity: usize) -> io::Result<(u32, u32)> {
    let (reader, writer) = channel::ring_channel_boxed(capacity)?;

    let lock = PROCESSES.lock();
    thread::critical_moment(|| {
        let mut processes = lock.processes.take();

        let res = match processes.iter_mut().find(|p| p.pid == pid) {
            Some(process) => {
                let read_fd = process.open_resource(reader)?;
                let write_fd = match process.open_resource(writer) {
                    Ok(fd) => fd,
                    Err(e) => {
                        let _ = process.resources.close_local(read_fd);
                        return Err(e);
                    }
                };
                Ok((read_fd, write_fd))
            }
            None => Err(io::Error::new(ErrorKind::NotFound, "process not found")),
        };

        lock.processes.set(processes);
        res
    })
}

pub fn open_stream_pair_current(capacity: usize) -> io::Result<(u32, u32)> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    open_stream_pair_for_proc(pid, capacity)
}

/// Mark or unmark an FD to be inherited by future children of this process.
pub fn set_proc_fd_inheritable(pid: ProcessID, fd: u32, inheritable: bool) -> io::Result<()> {
    let lock = PROCESSES.lock();
    thread::critical_moment(|| {
        let mut processes = lock.processes.take();
        let res = match processes.iter_mut().find(|p| p.pid == pid) {
            Some(process) => process.resources.set_inheritable(fd, inheritable),
            None => Err(io::Error::new(ErrorKind::NotFound, "process not found")),
        };
        lock.processes.set(processes);
        res
    })
}

pub fn set_current_fd_inheritable(fd: u32, inheritable: bool) -> io::Result<()> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    set_proc_fd_inheritable(pid, fd, inheritable)
}

/// Install a child-only FD template for future children.
///
/// The parent does not get a local FD for `resource`; it is only installed
/// into spawned children at `child_fd`.
pub fn install_child_resource_for_proc(
    parent_pid: ProcessID,
    child_fd: u32,
    resource: Box<dyn FdDispatch + 'static>,
) -> io::Result<()> {
    let global_fd = fd::alloc(resource)?;
    let lock = PROCESSES.lock();
    thread::critical_moment(|| {
        let mut processes = lock.processes.take();
        let res = match processes.iter_mut().find(|p| p.pid == parent_pid) {
            Some(parent) => {
                parent.resources.set_child_template_global(child_fd, global_fd);
                Ok(())
            }
            None => {
                let _ = resources::fd::close(global_fd);
                Err(io::Error::new(ErrorKind::NotFound, "parent process not found"))
            }
        };
        lock.processes.set(processes);
        res
    })
}

pub fn install_child_resource_current(
    child_fd: u32,
    resource: Box<dyn FdDispatch + 'static>,
) -> io::Result<()> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    install_child_resource_for_proc(pid, child_fd, resource)
}

pub fn clear_child_fd_template_for_proc(parent_pid: ProcessID, child_fd: u32) -> io::Result<()> {
    let lock = PROCESSES.lock();
    thread::critical_moment(|| {
        let mut processes = lock.processes.take();
        let res = match processes.iter_mut().find(|p| p.pid == parent_pid) {
            Some(parent) => {
                parent.resources.remove_child_template(child_fd);
                Ok(())
            }
            None => Err(io::Error::new(ErrorKind::NotFound, "parent process not found")),
        };
        lock.processes.set(processes);
        res
    })
}

pub fn clear_child_fd_template_current(child_fd: u32) -> io::Result<()> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    clear_child_fd_template_for_proc(pid, child_fd)
}

/// Create a stream where one endpoint is installed into future children at `child_fd`,
/// and the other endpoint is returned as a parent local FD.
pub fn open_child_stream_for_proc(
    parent_pid: ProcessID,
    child_fd: u32,
    capacity: usize,
    direction: ChildStreamDirection,
) -> io::Result<u32> {
    let (reader, writer) = channel::ring_channel_boxed(capacity)?;
    let (parent_resource, child_resource): (Box<dyn FdDispatch>, Box<dyn FdDispatch>) =
        match direction {
            ChildStreamDirection::ParentReads => (reader, writer),
            ChildStreamDirection::ParentWrites => (writer, reader),
        };

    let child_global_fd = fd::alloc(child_resource)?;
    let lock = PROCESSES.lock();

    thread::critical_moment(|| {
        let mut processes = lock.processes.take();
        let res = match processes.iter_mut().find(|p| p.pid == parent_pid) {
            Some(parent) => {
                let parent_local_fd = match parent.open_resource(parent_resource) {
                    Ok(fd) => fd,
                    Err(e) => {
                        let _ = resources::fd::close(child_global_fd);
                        return Err(e);
                    }
                };

                parent
                    .resources
                    .set_child_template_global(child_fd, child_global_fd);

                Ok(parent_local_fd)
            }
            None => {
                let _ = resources::fd::close(child_global_fd);
                Err(io::Error::new(ErrorKind::NotFound, "parent process not found"))
            }
        };

        lock.processes.set(processes);
        res
    })
}

pub fn open_child_stream_current(
    child_fd: u32,
    capacity: usize,
    direction: ChildStreamDirection,
) -> io::Result<u32> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    open_child_stream_for_proc(pid, child_fd, capacity, direction)
}

pub fn open_child_stdout_stream_for_proc(parent_pid: ProcessID, capacity: usize) -> io::Result<u32> {
    open_child_stream_for_proc(parent_pid, 1, capacity, ChildStreamDirection::ParentReads)
}

pub fn open_child_stderr_stream_for_proc(parent_pid: ProcessID, capacity: usize) -> io::Result<u32> {
    open_child_stream_for_proc(parent_pid, 2, capacity, ChildStreamDirection::ParentReads)
}

pub fn open_child_stdin_stream_for_proc(parent_pid: ProcessID, capacity: usize) -> io::Result<u32> {
    open_child_stream_for_proc(parent_pid, 0, capacity, ChildStreamDirection::ParentWrites)
}

pub fn open_child_stdout_stream_current(capacity: usize) -> io::Result<u32> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    open_child_stdout_stream_for_proc(pid, capacity)
}

pub fn open_child_stderr_stream_current(capacity: usize) -> io::Result<u32> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    open_child_stderr_stream_for_proc(pid, capacity)
}

pub fn open_child_stdin_stream_current(capacity: usize) -> io::Result<u32> {
    let pid = current_process()
        .ok_or_else(|| io::Error::new(ErrorKind::PermissionDenied, "thread is not in a process"))?;
    open_child_stdin_stream_for_proc(pid, capacity)
}
