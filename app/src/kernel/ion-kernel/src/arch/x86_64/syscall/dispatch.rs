use core::{ffi::CStr, panic};

use alloc::{borrow::ToOwned, boxed::Box, string::ToString, vec};

use crate::{
    arch::{
        VirtAddr,
        syscall::parse::{
            IoCommand, ProcessCommand, ProcessFlags,
            Syscall::{self, Io},
            ThreadCommand,
        },
    },
    asyncio,
    c_lib::libc::{self, OsError},
    event_logger::{Event, EventData, EventLevel, SyscallEvent, push_event},
    io::{ErrorKind, File, OpenOptions},
    lib_alloc::user::{USER_DATA_BASE, USER_DATA_SIZE},
    log::{self, info},
    privilege,
    process::{self, current_process, exit_proc, read_current_fd, write_current_fd},
    resources::OpenFlags,
    thread,
};

/// Represents inputs to a syscall
#[repr(C)]
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct SyscallFrame {
    pub rax: u64, // syscall number
    pub rdi: u64, // sub command
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r8: u64,
    pub r9: u64,
}

const UNKNOWN_MAGIC_VAL: u64 = u64::MAX - 1;
const FAIL_MAGIC_VAL: u64 = u64::MAX - 2;

/// SysV x86_64 returns two integer fields in `rax` and `rdx`.
#[repr(C)]
pub struct SyscallReturn {
    pub value: u64,
    pub metadata: u64,
}

#[doc(hidden)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn syscall_dispatch(frame: *const SyscallFrame) -> SyscallReturn {
    let (value, metadata) = syscall_dispatch_inner(frame);
    SyscallReturn { value, metadata }
}

fn io_error_to_os_error(kind: Option<ErrorKind>) -> OsError {
    match kind.unwrap_or(ErrorKind::Other) {
        ErrorKind::NotFound => OsError::NotFound,
        ErrorKind::PermissionDenied => OsError::PermissionDenied,
        ErrorKind::AlreadyExists => OsError::AlreadyExists,
        ErrorKind::WouldBlock => OsError::Busy,
        ErrorKind::InvalidInput | ErrorKind::InvalidData => OsError::InvalidInput,
        ErrorKind::IsADirectory => OsError::IsADirectory,
        ErrorKind::TimedOut => OsError::Timeout,
        ErrorKind::Interrupted => OsError::Interrupted,
        ErrorKind::ResourceBusy => OsError::Busy,
        ErrorKind::Unsupported => OsError::Unsupported,
        ErrorKind::MemoryError => OsError::MemoryError,
        _ => OsError::ProcessFail,
    }
}

fn syscall_dispatch_inner(frame: *const SyscallFrame) -> (u64, u64) {
    let frame = unsafe { &*frame };

    let syscall = Syscall::parse(frame.clone());

    push_event(Event {
        cpu: super::super::cpu::core_id() as u16,
        ticks: super::super::interrupts::ticks(),
        level: EventLevel::Trace,
        data: EventData::Syscall(SyscallEvent::Enter {
            group: frame.rax as u32,
            command: frame.rdi as u32,
        }),
    });

    match syscall {
        Syscall::Ack => (0, 0),
        Syscall::Process(v) => {
            match v {
                ProcessCommand::Exit(code) => {
                    info!("process <TID {}> exited with code {code}", thread::id().0);

                    let pid = current_process().unwrap();

                    exit_proc(pid, code);

                    // if we are still here somehow, then we return an error

                    libc::set_os_error(OsError::ProcessFail);

                    (FAIL_MAGIC_VAL, FAIL_MAGIC_VAL)
                }
                ProcessCommand::Spawn(path, args, argc, flags) => {
                    let path = unsafe { CStr::from_ptr(path) };
                    let path_str = match path.to_str() {
                        Ok(s) => s,
                        Err(_) => {
                            libc::set_os_error(OsError::InvalidInput);
                            return (frame.rax, UNKNOWN_MAGIC_VAL);
                        }
                    };
                    let offset = 0;
                    let mut arg_vec = vec![];
                    arg_vec.reserve_exact(argc + 1);
                    arg_vec.push(path_str);
                    for i in 0..argc {
                        let arg_ptr = unsafe { *(args.add(i)) };
                        let arg = unsafe { CStr::from_ptr(arg_ptr) };
                        let arg = match arg.to_str() {
                            Ok(s) => s,
                            Err(_) => {
                                libc::set_os_error(OsError::InvalidInput);
                                return (frame.rax, UNKNOWN_MAGIC_VAL);
                            }
                        };
                        arg_vec.push(arg);
                    }

                    let current_proc = current_process().unwrap();
                    let owner = process::with_proc(current_proc, |proc| proc.owner).unwrap();

                    let local_fds = if flags.contains(ProcessFlags::INHERIT_FDS) {
                        Some(
                            process::with_proc(current_proc, |proc| proc.resources.inherit_fds())
                                .unwrap(),
                        )
                    } else if flags.contains(ProcessFlags::INHERIT_STDIO) {
                        log::warn!("inheriting stdio is not yet implemented, returning error code");
                        return (frame.rax, UNKNOWN_MAGIC_VAL);
                    } else {
                        None
                    }
                    .transpose();

                    let local_fds = match local_fds {
                        Ok(fds) => fds.unwrap_or_default(),
                        Err(e) => {
                            libc::set_os_error(io_error_to_os_error(e.kind()));
                            return (frame.rax, UNKNOWN_MAGIC_VAL);
                        }
                    };

                    let proc_f =
                        process::exec(owner, &path_str, &arg_vec, Some(current_proc), local_fds);
                    match asyncio::run_future(proc_f) {
                        Ok(pid) => (0, pid.0 as u64),
                        Err(e) => {
                            libc::set_os_error(io_error_to_os_error(e.kind()));
                            return (frame.rax, UNKNOWN_MAGIC_VAL);
                        }
                    }
                }
                ProcessCommand::ChangeName(str, len) => {
                    if len > 512 {
                        libc::set_os_error(OsError::InvalidInput);
                        return (frame.rax, UNKNOWN_MAGIC_VAL);
                    } else {
                        let mut kernel_buf = vec![0u8; len];

                        if let Err(e) =
                            copy_from_user(kernel_buf.as_mut_ptr(), str as *const u8, len)
                        {
                            libc::set_os_error(e.to_os_error());
                            return (frame.rax, UNKNOWN_MAGIC_VAL);
                        }

                        if let Ok(s) = core::str::from_utf8(&kernel_buf) {
                            let pid = current_process().unwrap();
                            process::with_proc_mut(pid, |proc| {
                                proc.name = s.to_string();
                            });
                            (0, 0)
                        } else {
                            libc::set_os_error(OsError::InvalidInput);
                            (frame.rax, UNKNOWN_MAGIC_VAL)
                        }
                    }
                }
                ProcessCommand::Kill(p) => {
                    if let Some(curr_proc) = process::current_process() {
                        if curr_proc == p {
                            libc::set_os_error(OsError::InvalidInput);
                            return (frame.rax, UNKNOWN_MAGIC_VAL);
                        } else {
                            // check this process has permission to kill the other process
                            let uid = process::with_proc(p, |proc| proc.owner).unwrap();
                            let privilege = privilege::get_user(uid).unwrap().privilege;

                            let curr_uid =
                                process::with_proc(curr_proc, |proc| proc.owner).unwrap();
                            let curr_privilege = privilege::get_user(uid).unwrap().privilege;

                            if privilege > curr_privilege {
                                libc::set_os_error(OsError::PermissionDenied);
                                (frame.rax, UNKNOWN_MAGIC_VAL)
                            } else {
                                process::exit_proc(p, 1);
                                (0, 0)
                            }
                        }
                    } else {
                        libc::set_os_error(OsError::ProcessFail);
                        (frame.rax, UNKNOWN_MAGIC_VAL)
                    }
                }
                ProcessCommand::Unknown => {
                    libc::set_os_error(OsError::InvalidInput);
                    (frame.rax, UNKNOWN_MAGIC_VAL)
                }
            }
        }

        Syscall::Io(io) => match io {
            IoCommand::Write(fd, buf, len) => {
                if len > 64 * 1024 {
                    libc::set_os_error(OsError::InvalidInput);
                    return (u64::MAX, 0);
                }

                let mut kernel_buf = vec![0u8; len];

                if let Err(e) = copy_from_user(kernel_buf.as_mut_ptr(), buf as *const u8, len) {
                    libc::set_os_error(e.to_os_error());
                    return (u64::MAX, 0);
                }

                match write_current_fd(fd as u32, &kernel_buf) {
                    Ok(written) => (written, 0),
                    Err(e) => {
                        libc::set_os_error(io_error_to_os_error(e.kind()));

                        // Keep a debug fallback path so misconfigured fd routing is visible.
                        if let Ok(s) = str::from_utf8(&kernel_buf) {
                            log::error!("{e}");
                            log::debug!("fd write fallback fd={fd}: {s}");
                        } else {
                            log::error!("{e}");
                            log::debug!("fd write fallback fd={fd}: {kernel_buf:?}");
                        }

                        (u64::MAX, 0)
                    }
                }
            }
            IoCommand::Read(fd, buf, len) => {
                if len > 64 * 1024 {
                    libc::set_os_error(OsError::InvalidInput);
                    return (0, 0);
                }

                let mut kernel_buf = vec![0; len];

                match read_current_fd(fd as u32, &mut kernel_buf) {
                    Ok(read) => {
                        let x = copy_to_user(buf as *mut u8, kernel_buf.as_ptr(), read as usize)
                            .map_err(|e| e.to_os_error());

                        if read != 0 {
                            log::debug!("read {read} bytes from fd {fd}: {kernel_buf:?}");
                        }

                        match x {
                            Ok(()) => (read, 0),
                            Err(e) => {
                                libc::set_os_error(e);
                                (u64::MAX, 0)
                            }
                        }
                    }
                    Err(e) => {
                        libc::set_os_error(io_error_to_os_error(e.kind()));
                        (u64::MAX, 0)
                    }
                }
            }
            IoCommand::Close(fd) => {
                process::with_current_proc_mut(|p| match p.close_resource(fd as u32) {
                    Ok(()) => (0, 0),
                    Err(e) => {
                        libc::set_os_error(io_error_to_os_error(e.kind()));
                        (FAIL_MAGIC_VAL, fd as u64)
                    }
                })
                .unwrap()
            }
            IoCommand::Open(path, path_len, flags) => {
                if path_len > 512 {
                    libc::set_os_error(OsError::InvalidInput);
                    return (FAIL_MAGIC_VAL, 0);
                }

                let mut kernel_buf = vec![0u8; path_len];

                if let Err(e) = copy_from_user(kernel_buf.as_mut_ptr(), path as *const u8, path_len)
                {
                    libc::set_os_error(e.to_os_error());
                    return (FAIL_MAGIC_VAL, 0);
                }

                let path_str = match core::str::from_utf8(&kernel_buf) {
                    Ok(s) => s,
                    Err(_) => {
                        libc::set_os_error(OsError::InvalidInput);
                        return (FAIL_MAGIC_VAL, 0);
                    }
                };

                let flags = match OpenFlags::from_bits(flags) {
                    Some(f) => f,
                    None => {
                        libc::set_os_error(OsError::InvalidInput);
                        return (FAIL_MAGIC_VAL, flags as u64);
                    }
                };

                let mut opts = OpenOptions::new();
                let fut = opts
                    .read(flags.contains(OpenFlags::READ))
                    .write(flags.contains(OpenFlags::WRITE))
                    .create(flags.contains(OpenFlags::CREATE))
                    .truncate(flags.contains(OpenFlags::TRUNCATE))
                    .open(path_str);

                let fd = match asyncio::run_future(fut) {
                    Ok(file) => file.fd(),
                    Err(e) => {
                        libc::set_os_error(io_error_to_os_error(e.kind()));
                        return (FAIL_MAGIC_VAL, 0);
                    }
                };
                (0, fd as u64)
            }
            IoCommand::Seek(fd, offset) => {
                let offset: u64 = match offset.try_into() {
                    Ok(n) => n,
                    Err(_) => {
                        libc::set_os_error(OsError::InvalidInput);
                        return (FAIL_MAGIC_VAL, fd as u64);
                    }
                };
                process::with_current_proc_mut(|p| match p.seek_resource(fd as u32, offset) {
                    Ok(new_offset) => (new_offset as u64, 0),
                    Err(e) => {
                        libc::set_os_error(io_error_to_os_error(e.kind()));
                        (FAIL_MAGIC_VAL, fd as u64)
                    }
                })
                .unwrap()
            }
            IoCommand::Unknown => {
                libc::set_os_error(OsError::InvalidInput);
                (frame.rax, UNKNOWN_MAGIC_VAL)
            }
        },

        Syscall::Thread(v) => match v {
            ThreadCommand::ExitCurrent => {
                thread::exit_current();
            }
            ThreadCommand::GetErrno => (*libc::get_errno() as u64, 0),
            ThreadCommand::Yield => {
                crate::scheduler::yield_now();
                (0, 0)
            }
            ThreadCommand::Unknown => {
                libc::set_os_error(OsError::InvalidInput);
                (frame.rax, UNKNOWN_MAGIC_VAL)
            }
        },

        // Unknown syscall policy:
        // 0 in rax - main command unknown (rdi irrelevant)
        // 0 in rdi - sub command unknown
        Syscall::Unknown => {
            libc::set_os_error(OsError::InvalidInput);
            (UNKNOWN_MAGIC_VAL, 0)
        }
    }
}

// copy from/to

#[inline]
fn validate_user_range(ptr: usize, len: usize) -> bool {
    if len == 0 {
        return true;
    }

    let end = match ptr.checked_add(len - 1) {
        Some(v) => v,
        None => return false,
    };

    let base = USER_DATA_BASE;
    let top = USER_DATA_BASE + USER_DATA_SIZE;

    ptr >= base && end < top
}

#[derive(Debug, Clone, Copy)]
pub enum UserCopyError {
    InvalidPointer,
    NotUserRange,
    NotMapped,
}

impl UserCopyError {
    pub fn to_os_error(self) -> OsError {
        match self {
            UserCopyError::InvalidPointer => OsError::InvalidInput,
            UserCopyError::NotUserRange => OsError::PermissionDenied,
            UserCopyError::NotMapped => OsError::PermissionDenied,
        }
    }
}

use crate::arch::mmu::is_user_accessible;

const PAGE_SIZE: usize = 4096;

fn validate_user_pages(ptr: usize, len: usize) -> bool {
    if len == 0 {
        return true;
    }

    let start = ptr & !(PAGE_SIZE - 1);
    let end = (ptr + len - 1) & !(PAGE_SIZE - 1);

    let mut page = start;

    while page <= end {
        if !is_user_accessible(VirtAddr::new(page)) {
            return false;
        }

        page += PAGE_SIZE;
    }

    true
}

pub fn copy_from_user(dst: *mut u8, src_user: *const u8, len: usize) -> Result<(), UserCopyError> {
    let src = src_user as usize;

    if !validate_user_range(src, len) {
        return Err(UserCopyError::NotUserRange);
    }

    if !validate_user_pages(src, len) {
        return Err(UserCopyError::NotMapped);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(src_user, dst, len);
    }

    Ok(())
}

pub fn copy_to_user(dst_user: *mut u8, src: *const u8, len: usize) -> Result<(), UserCopyError> {
    let dst = dst_user as usize;

    if !validate_user_range(dst, len) {
        return Err(UserCopyError::NotUserRange);
    }

    if !validate_user_pages(dst, len) {
        return Err(UserCopyError::NotMapped);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(src, dst_user, len);
    }

    Ok(())
}
