use core::sync::atomic::{Ordering, compiler_fence};

use x86_64::instructions;


/// Spin Loop Hint to the CPU
#[inline]
pub fn relax() {
    core::arch::x86_64::_mm_pause();
}

/// spins forever, [`relax`]ing the CPU each iteration
/// 
/// This is not usually what you want - this does not halt the CPU. see [`hlt_loop`] for that.
#[inline]
pub fn spin() -> ! {
    loop {
        relax();
    }
}

/// Halts the CPU forever
/// 
/// use [`spin`] to spin without halting
#[inline]
pub fn hlt_loop() -> ! {
    loop {
        instructions::hlt();
    }
}

/// does nothing
/// 
/// useful for bypassing llvm optimizations
/// 
/// On `x86_64` this emits a `nop` instruction
#[inline]
pub fn nop() {
    instructions::nop();
}

/// Jumps to the given address
/// 
/// This function *may* return if the jumped-to code is exactly after this function call.
/// 
/// However, the behavior of this function is unspecified/unknown if the jumped-to code
/// ever returns to the caller of this function, as Rust may have made optimizations based on the assumption
/// (in some cases, adding a `ud2` instruction after this function call) that this function never returns.
/// # Safety
/// Jumping to an invalid address is undefined behavior.
/// 
/// Calling this with a function pointer is safe, as long as the function does not return.
#[inline]
pub unsafe fn jump(addr: usize) -> ! {

    debug_assert!(addr != 0);
    debug_assert!((addr as *const usize).is_aligned());

    unsafe {
        core::arch::asm!(
            "jmp {0}",
            in(reg) addr,
            options(noreturn)
        );
    }
}

/// Creates a CPU memory fence
/// 
/// This does **not** create a compiler fence - only a CPU fence.
#[inline(always)]
pub fn fence() {
    unsafe {
        core::arch::asm!(
            "mfence",
            options(nostack, preserves_flags)
        );
    }
}

/// Creates both a memory and CPU fence
#[inline(always)]
pub fn full_fence(compiler_order: Ordering) {
    compiler_fence(compiler_order);
    fence();
}

/// returns the core's id
/// 
/// 0 for now
#[inline]
pub fn core_id() -> usize {
    0 // temporary
}