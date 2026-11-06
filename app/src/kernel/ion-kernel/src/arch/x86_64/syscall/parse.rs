use alloc::string::String;

use crate::{arch::syscall::dispatch::SyscallFrame, process::ProcessID};

#[derive(Debug, Clone)]
pub enum ThreadCommand {
    ExitCurrent,
    GetErrno,
    Yield,
    Unknown,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct ProcessFlags: u32 {
        const INHERIT_FDS = 1 >> 0;
        const INHERIT_STDIO = 1 >> 1;
        // rest is reserved for future use
    }
}

#[derive(Debug, Clone)]
pub enum ProcessCommand {
    Exit(i32),
    Spawn(
        *const i8,        /* path */
        *const *const i8, /* argv */
        usize,            /* argc */
        ProcessFlags,
    ),
    Kill(ProcessID),
    ChangeName(*const i8, usize),
    Unknown,
}

#[derive(Debug, Clone)]
pub enum IoCommand {
    Write(usize, usize, usize),  // fd, buf, len
    Read(usize, usize, usize),   // fd, buf, len
    Open(*const i8, usize, u32), // path, path length
    Close(usize),                // fd
    Seek(usize, i64),            // fd, offset
    Unknown,
}

#[derive(Debug, Clone)]
pub enum Syscall {
    /// Used to enter user mode.
    Ack,
    Process(ProcessCommand),
    Thread(ThreadCommand),
    Io(IoCommand),

    Unknown,
}

impl Syscall {
    /// Syscall Policy: (TODO: place this somewhere safe)
    ///
    /// Inputs:
    /// rax - main command
    /// rdi - sub command
    pub fn parse(frame: SyscallFrame) -> Self {
        match frame.rax {
            0 => Self::Ack,
            1 => Self::Process(match frame.rdi {
                0 => ProcessCommand::Exit(frame.rsi as i32),
                _ => ProcessCommand::Unknown,
            }),
            2 => Self::Io(match frame.rdi {
                0 => IoCommand::Write(frame.rsi as usize, frame.rdx as usize, frame.r10 as usize),
                1 => IoCommand::Read(frame.rsi as usize, frame.rdx as usize, frame.r10 as usize),
                _ => IoCommand::Unknown,
            }),
            3 => Self::Thread(match frame.rdi {
                0 => ThreadCommand::ExitCurrent,
                1 => ThreadCommand::GetErrno,
                2 => ThreadCommand::Yield,
                _ => ThreadCommand::Unknown,
            }),
            _ => Self::Unknown,
        }
    }
}
