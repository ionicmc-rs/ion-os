use crate::{hlt_loop, lib_alloc::is_heap_addr, scheduler, serial_println};

#[repr(C)]
pub struct Context {
    rsp: usize,
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
}


impl Context {
    pub fn new(
        entry: extern "C" fn(usize) -> !,
        arg: usize,
        stack_top: usize
    ) -> Self {
        if !is_heap_addr(stack_top) {
            serial_println!("failed to create context: {stack_top} is not a heap address");
        }
        serial_println!("creating and aligning.");
        let mut rsp = stack_top;
        rsp &= !0xF;

        serial_println!("pushing fake stack frame.");
        unsafe {
            // We push a fake stack frame to trick the CPU to return to thread_exit if we ever return.
            rsp -= core::mem::size_of::<usize>();
            *(rsp as *mut usize) = thread_exit as *const () as usize;
            
            // ... we then push the thread_trampoline to start executing there.
            rsp -= core::mem::size_of::<usize>();
            *(rsp as *mut usize) = thread_trampoline as *const () as usize;
        }
        serial_println!("done - now constructing context");

        Self {
            // stack pointer
            rsp,

            // use r12 to pass the arg
            r12: arg as u64,

            // use r15 to store the entry point.
            r15: entry as *const () as u64,

            // rest are 0
            rbx: 0,
            rbp: 0,
            r13: 0,
            r14: 0,
        }
        
    }

    /// Switch to a new context
    /// 
    /// This may never return.
    /// # Examples
    /// Examples are not provided - this a private API.
    /// # Safety
    /// The context switch must only be called with valid [`Context`]s
    pub unsafe fn switch_to(&mut self, new: &Context) {
        serial_println!("switching to a new context");
        unsafe {
            context_switch(self, new);
        }
    }

    pub unsafe fn switch_raw(this: *mut Self, new: *const Context) {
        unsafe {
            context_switch(this, new);
        }
    }
}

unsafe extern "C" {
    /// raw assembly context switch.
    /// 
    /// this changes from `o` to `n`
    /// # Examples
    /// Examples are not provided - this a private API.
    unsafe fn context_switch(o: *mut Context, n: *const Context);
    /// moves the `r12` register into the proper `rdi` register.
    /// 
    /// this allows the argument to be used instead of manual register reading.
    /// # Example
    /// examples are not provided as this is a private API.
    unsafe fn thread_trampoline() -> !;
}

extern "C" fn thread_exit() -> ! {
    scheduler::exit_current();
    hlt_loop()
}