use crate::{arch::syscall::parse::{ProcessCommand, Syscall, ThreadCommand}, hlt_loop, log::info, serial_println, thread};

/// Represents inputs to a syscall
#[repr(C)]
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct SyscallFrame {
    pub rax: u64, // syscall number
    pub rdi: u64,
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
pub extern "C" fn syscall_dispatch(
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
                    thread::exit_current();
                }
                ProcessCommand::Unknown => {
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
                set_metadata(UNKNOWN_MAGIC_VAL);
                frame.rax
            }
        }

        // Unknown syscall policy:
        // 0 in rax - main command unknown (rdi irrelevant)
        // 0 in rdi - sub command unknown

        Syscall::Unknown => UNKNOWN_MAGIC_VAL
    }
}
