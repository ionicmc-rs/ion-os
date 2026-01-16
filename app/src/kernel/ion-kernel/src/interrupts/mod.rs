//! Tools for both Software and Hardware Interrupts.
//!
//! We use this module for setting the IDT, GDT and TSS tables, and handling keyboard inputs.
//!
//! These things are handled in Ion OS's initialization, so you wouldn't typically have to see them.

use crate::{c_lib::libc, interrupts::pic8259::InterruptIndex, serial_println, text::println};
use lazy_static::lazy_static;
use x86_64::{
    instructions::interrupts,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame},
};

macro set_index($idt:expr_2021, $($index:ident$((unsafe $stack:literal))? => $handler:expr_2021),*) {
    $({
        $(_ = $stack; let options = )?
        $idt[InterruptIndex::$index.as_u8()]
           .set_handler_fn($handler);
        $(unsafe { options.set_stack_index($stack) };)?
    })*
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.page_fault.set_handler_fn(page_fault::page_fault)
                .set_stack_index(page_fault::PAGE_FAULT_IST_INDEX);
            idt.double_fault.set_handler_fn(double_fault::double_fault)
                .set_stack_index(double_fault::DOUBLE_FAULT_IST_INDEX);
        }
        idt.general_protection_fault.set_handler_fn(gp::protection_fault);
        // Hardware Interrupts.


        set_index!(
            idt,
            Timer => timer::timer_isr,
            Keyboard => keyboard::keyboard_interrupt_handler
        );

        idt
    };
}

/// inits the IDT, GDT, TSS, PIC8259, and interrupts.
pub fn init_interrupt_operations() {
    gdt::init();
    IDT.load();
    pic8259::init();
    // no interrupts here
    serial_println!("Initialized interrupt operations properly");
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    libc::error::set_errno(3); // note: reads TLS
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

#[cfg(feature = "test")]
/// Tests
pub mod test {
    use crate::{c_lib::libc, test::TestInfo};

    /// breakpoint test
    pub fn test_breakpoint(_inf: TestInfo) {
        x86_64::instructions::interrupts::int3();
        // // always passes, which may be a problem...
        // now can check the error code number
        assert_eq!(libc::get_errno().0, 3)
    }
}

/// Disables interrupts and reenables them upon drop
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterruptGuard(());

impl Default for InterruptGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl InterruptGuard {
    /// disables interrupts, reenabling them upon drop
    pub fn new() -> Self {
        interrupts::disable();
        Self(())
    }

    /// disables this guard, keeping interrupts off.
    pub fn disable(self) {
        core::mem::forget(self);
    }
}

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        interrupts::enable();
    }
}

mod double_fault;
/// GDT
pub mod gdt;
/// Keyboard Interrupt Handling.
pub mod keyboard;
mod page_fault;
pub mod pic8259;
mod timer;
pub use timer::ticks;
mod gp;
