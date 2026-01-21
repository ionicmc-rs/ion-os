use crate::arch::syscall::dispatch::SyscallFrame;

pub enum ThreadCommand {
    ExitCurrent,
    Unknown
}

pub enum ProcessCommand {
    Exit(i32),
    Unknown
}

pub enum Syscall {
    /// Used to enter user mode.
    Ack,
    Process(ProcessCommand),
    Thread(ThreadCommand),

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
            2 => Self::Thread(match frame.rdi {
                0 => ThreadCommand::ExitCurrent,
                _ => ThreadCommand::Unknown
            }),
            _ => Self::Unknown
        }
    }
}