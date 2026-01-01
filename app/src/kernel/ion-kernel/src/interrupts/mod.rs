use crate::{interrupts::pic8259::InterruptIndex, println, serial_println};
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

macro set_index($idt:expr, $($index:ident => $handler:expr),*) {
    $(
        $idt[InterruptIndex::$index.as_u8()]
            .set_handler_fn($handler);
    )*
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault.set_handler_fn(double_fault::double_fault)
                .set_stack_index(double_fault::DOUBLE_FAULT_IST_INDEX);
        }
        // Hardware Interrupts.
        set_index!(
            idt,
            Timer => pic8259::handlers::timer,
            Keyboard => keyboard::keyboard_interrupt_handler
        );

        idt
    };
}

/// inits the idt.
pub fn init_interrupt_operations() {
    gdt::init();
    IDT.load();
    pic8259::init();
    x86_64::instructions::interrupts::enable();
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
/// PIC 8259 Compatibility.
pub mod pic8259;
/// Keyboard Interrupt Handling.
pub mod keyboard;
mod double_fault;