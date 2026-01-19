use x86_64::{registers::model_specific::{
    Efer, EferFlags, LStar, Star,
}};

unsafe extern "C" { 
    // note: it is not actually of the "C" abi, and takes args.
    // note: however, this is (likely) the best way to do this.
    fn syscall_entry();
}

/// inits syscalls
pub unsafe fn init_syscalls() {
    // Enable SYSCALL/SYSRET
    unsafe { Efer::update(|e| e.insert(EferFlags::SYSTEM_CALL_EXTENSIONS)) };

    // Set entry point
    LStar::write(x86_64::VirtAddr::new(syscall_entry as *const () as u64));

    let selectors = super::interrupts::gdt_selectors();

    // Configure segment selectors for syscall/sysret
    Star::write(
        selectors.user_code,
        selectors.user_data,
        selectors.kernel_code,
        selectors.kernel_data,
    ).unwrap();
}

/// syscall impl
pub mod dispatch;
mod parse;