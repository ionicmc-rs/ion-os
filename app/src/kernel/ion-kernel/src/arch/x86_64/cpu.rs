use x86_64::instructions;


#[inline]
pub fn relax() {
    instructions::hlt();
}

#[inline]
pub fn spin() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[inline]
pub fn nop() {
    instructions::nop();
}
#[inline]
pub fn core_id() -> usize {
    0 // temporary
}