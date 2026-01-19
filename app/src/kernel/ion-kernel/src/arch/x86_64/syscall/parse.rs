use crate::arch::syscall::dispatch::SyscallFrame;

pub enum Syscall {
    /// Used to enter user mode.
    Ack,
    Exit(i32),

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
            1 => Self::Exit(frame.rsi as i32),
            _ => Self::Unknown
        }
    }
}