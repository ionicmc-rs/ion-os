use x86_64::instructions;


/// relaxes the CPU (halts on x86)
#[inline]
pub fn relax() {
    instructions::hlt();
}

/// spins forever (core::hint::spin_loop)
#[inline]
pub fn spin() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// does nothing
/// 
/// useful for bypassing llvm optimizations
#[inline]
pub fn nop() {
    instructions::nop();
}

/// returns the core's id
/// 
/// 0 for now
#[inline]
pub fn core_id() -> usize {
    0 // temporary
}