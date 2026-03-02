use crate::arch::syscall::dispatch::SyscallFrame;

#[derive(Debug, Clone)]
pub enum ThreadCommand {
    ExitCurrent,
    Unknown
}

#[derive(Debug, Clone)]
pub enum ProcessCommand {
    Exit(i32),
    Unknown
}

#[derive(Debug, Clone)]
pub enum IoCommand {
    Write(usize, usize, usize), // fd, buf, len
    Unknown
}


#[derive(Debug, Clone)]
pub enum Syscall {
    /// Used to enter user mode.
    Ack,
    Process(ProcessCommand),
    Thread(ThreadCommand),
    Io(IoCommand),

    Unknown
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
                _ => ProcessCommand::Unknown
            }),
            2 => Self::Io(match frame.rdi {
                0 => IoCommand::Write(frame.rsi as usize, frame.rdx as usize, frame.r10 as usize),
                _ => IoCommand::Unknown
            }),
            3 => Self::Thread(match frame.rdi {
                0 => ThreadCommand::ExitCurrent,
                _ => ThreadCommand::Unknown
            }),
            _ => Self::Unknown
        }
    }
}