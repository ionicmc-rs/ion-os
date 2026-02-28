
/// Represents inputs to a syscall
#[repr(C)]
#[derive(Debug, Clone, Default, Copy, Hash, PartialEq, Eq)]
pub struct SyscallFrame {
    pub rax: u64, // syscall number
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r8:  u64,
    pub r9:  u64,
}

/// Calls the syscall for this frame
/// 
/// this is not recommended to be used directly, helpers for the specific syscall are provided.
/// 
/// # Safety
/// You must be using Ion OS, after syscall initialization, and the syscall must be valid
pub unsafe fn syscall(frame: SyscallFrame) -> (u64, u64) {
    let mut result = frame.rax;
    let mut meta = frame.rdi;

    // Keep syscall in one asm block so register constraints/clobbers are explicit.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") result,
            inlateout("rdi") meta,
            in("rsi") frame.rsi,
            in("rdx") frame.rdx,
            in("r10") frame.r10,
            in("r8") frame.r8,
            in("r9") frame.r9,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }

    (result, meta)
}

macro_rules! syscall_fns {
    ($(
        $(#[$meta:meta])*
        $sys_name:ident
            $(<$($Generic:ident),* $(,)?>)?
        ($($arg:ident: $ArgTy:ty),* $(,)?)
        $(-> $Ret:ty)?
        {
            $($reg:ident: $val:expr),*
            $(,)?
        }
        $(($ret_get:pat, $ret_meta:pat) => $ret_parse:expr)?
    );* $(;)?) => {
        $(
            $(#[$meta])*
            #[allow(unused_variables)]
            pub fn $sys_name$($($Generic),*)?($($arg: $ArgTy),*) $(-> $Ret)? {
                let frame = SyscallFrame {
                    $($reg: $val,)*
                    ..Default::default()
                };
                $(let ($ret_get, $ret_meta) =)? unsafe { syscall(frame) };
                $($ret_parse)?
            }
        )*
    };
}

syscall_fns!(
    /// Does nothing but return.
    /// 
    /// This is used internally in Ion OS to enter user mode.
    ack() {};

    // process

    /// Exits the process.
    /// 
    /// the code is an i32, but is stored across the syscall-boundary as a u64, which may cause problems (it shouldn't)
    pexit(code: i32) {
        rax: 1,
        rdi: 0,
        rsi: code as u64
    };

    /// returns the id of the currently running process.
    pid() -> u64 {
        rax: 1,
        rdi: 1,
    }
    (ret, _) => ret;

    // thread

    /// Exits the currently running thread.
    texit() {
        rax: 2,
        rdi: 0
    };

    // io

    /// Writes `count` bytes from `buffer` to `fd`.
    write(fd: u64, buffer: *const u8, count: u64) -> u64 {
        rax: 3,
        rdi: 0,
        rsi: fd,
        rdx: buffer as u64,
        r10: count
    }
    (ret, _) => ret
);
