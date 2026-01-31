use crate::{arch::syscall::parse::{ProcessCommand, Syscall, ThreadCommand}, c_lib::libc::{self, OsError}, log::info, process::{CURRENT_PROCESS, exit_proc}, serial_println, thread};

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

    serial_println!("syscall command received: {}", frame.rax);
    serial_println!("full frame: {frame:#?}");

    match Syscall::parse(frame.clone()) {
        Syscall::Ack => {
            set_metadata(0);
            0
        },
        Syscall::Process(v) => {
            match v {
                ProcessCommand::Exit(code) => {
                    info!("process <TID {}> exited with code {code}", thread::id().0);

                    let pid = unsafe { CURRENT_PROCESS.unwrap() };
                    
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
