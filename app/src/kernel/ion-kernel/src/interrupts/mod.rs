use crate::{println, serial_println};
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault.set_handler_fn(double_fault::double_fault)
                .set_stack_index(double_fault::DOUBLE_FAULT_IST_INDEX);
        }
        idt
    };
}

/// inits the idt.
pub fn init_idt() {
    gdt::init();
    IDT.load();
    serial_println!("Initialized IDT properly");
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

/// GDT
pub mod gdt;
mod double_fault;