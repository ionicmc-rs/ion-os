use alloc::{string::String, vec};

use crate::{arch::{VirtAddr, syscall::parse::{IoCommand, ProcessCommand, Syscall, ThreadCommand}}, c_lib::libc::{self, OsError}, lib_alloc::user::{USER_DATA_BASE, USER_DATA_SIZE}, log::{self, info}, process::{current_process, exit_proc}, serial_println, thread};

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
    pub r8:  u64,
    pub r9:  u64,
}

const UNKNOWN_MAGIC_VAL: u64 = 0xab07df;

fn set_metadata(val: u64) {
    unsafe {
        core::arch::asm!(
            "mov rdi, {}",
            in(reg) val
        )
    }
}

#[doc(hidden)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn syscall_dispatch(
    frame: *const SyscallFrame,
) -> u64 {
    let frame = unsafe { &*frame };

    let syscall = Syscall::parse(frame.clone());

    log::debug!("syscall received: {syscall:?}");

    match syscall {
        Syscall::Ack => {
            set_metadata(0);
            0
        },
        Syscall::Process(v) => {
            match v {
                ProcessCommand::Exit(code) => {
                    info!("process <TID {}> exited with code {code}", thread::id().0);

                    let pid = unsafe { current_process().unwrap() };
                    
                    exit_proc(pid);

                    // if we are still here somehow, then we return an error

                    libc::set_os_error(OsError::ProcessFail);

                    frame.rax
                }
                ProcessCommand::Unknown => {
                    libc::set_os_error(OsError::InvalidInput);
                    set_metadata(UNKNOWN_MAGIC_VAL);
                    frame.rax
                }
            }
        }

        Syscall::Io(io) => match io {
            IoCommand::Write(fd, buf, len) => {
                // if it is a reserved fd (0, 1, 2), print for now

                if len > 64 * 1024 {
                    libc::set_os_error(OsError::InvalidInput);
                    return 0;
                }

                let mut kernel_buf = vec![0u8; len];

                if let Err(e) = copy_from_user(kernel_buf.as_mut_ptr(), buf as *const u8, len) {
                    libc::set_os_error(e.to_os_error());
                    return 0;
                }

                match fd {
                    fd @ 0..=2 => {
                        match String::from_utf8(kernel_buf) {
                            Ok(s) => {
                                log::debug!("got a write on fd `{fd}`: {s}");
                            }
                            Err(e) => {
                                log::debug!("got a write on fd `{fd}` (non-UTF8): {s:?}");
                                log::debug!("UTF8 error: {e:?}");
                            }
                        }
                    }
                    _ => {
                        panic!("non reserved fds are unsupported for now")
                    }
                }

                len as u64
            }
            IoCommand::Unknown => {
                libc::set_os_error(OsError::InvalidInput);
                set_metadata(UNKNOWN_MAGIC_VAL);
                frame.rax
            }
        }

        Syscall::Thread(v) => match v {
            ThreadCommand::ExitCurrent => {
                thread::exit_current();
            }
            ThreadCommand::Unknown => {
                libc::set_os_error(OsError::InvalidInput);
                set_metadata(UNKNOWN_MAGIC_VAL);
                frame.rax
            }
        }

        // Unknown syscall policy:
        // 0 in rax - main command unknown (rdi irrelevant)
        // 0 in rdi - sub command unknown

        Syscall::Unknown => {
            libc::set_os_error(OsError::InvalidInput);
            UNKNOWN_MAGIC_VAL
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
    let top  = USER_DATA_BASE + USER_DATA_SIZE;

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
    let end   = (ptr + len - 1) & !(PAGE_SIZE - 1);

    let mut page = start;

    while page <= end {
        if !is_user_accessible(VirtAddr::new(page)) {
            return false;
        }

        page += PAGE_SIZE;
    }

    true
}

pub fn copy_from_user(
    dst: *mut u8,
    src_user: *const u8,
    len: usize,
) -> Result<(), UserCopyError> {

    let src = src_user as usize;

    if !validate_user_range(src, len) {
        return Err(UserCopyError::NotUserRange);
    }

    if !validate_user_pages(src, len) {
        return Err(UserCopyError::NotMapped);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(
            src_user,
            dst,
            len,
        );
    }

    Ok(())
}

pub fn copy_to_user(
    dst_user: *mut u8,
    src: *const u8,
    len: usize,
) -> Result<(), UserCopyError> {

    let dst = dst_user as usize;

    if !validate_user_range(dst, len) {
        return Err(UserCopyError::NotUserRange);
    }

    if !validate_user_pages(dst, len) {
        return Err(UserCopyError::NotMapped);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(
            src,
            dst_user,
            len,
        );
    }

    Ok(())
}
