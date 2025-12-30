use crate::{println, serial_println};
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        serial_println!("ANYTHING???");
        let mut idt = InterruptDescriptorTable::new();
        serial_println!("Setting Breakpoint");
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        serial_println!("Done");
        idt
    };
}

/// inits the idt.
pub fn init_idt() {
    IDT.load();
    serial_println!("Well, it loads");
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

#[cfg(feature = "test")]
/// Tests
pub mod test {
    use crate::test::{TestInfo, TestResult};

    /// breakpoint test
    pub fn test_breakpoint(_inf: TestInfo) -> TestResult {
        x86_64::instructions::interrupts::int3();
        // always passes, which may be a problem...
        TestResult::Ok
    }
}