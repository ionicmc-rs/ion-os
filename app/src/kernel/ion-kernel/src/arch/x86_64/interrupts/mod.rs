//! Tools for both Software and Hardware Interrupts.
//!
//! We use this module for setting the IDT, GDT and TSS tables, and handling keyboard inputs.
//!
//! These things are handled in Ion OS's initialization, so you wouldn't typically have to see them.

use crate::{
    address_space::{self, kernel_address_space},
    arch::{
        PhysAddr, VirtAddr,
        interrupts::{ioapic::IoApic, thread::YIELD_INTERRUPT_NUMBER},
        mmu::{self, PageTable, PageTableFlags},
    },
    c_lib::libc,
    serial_println,
};
use lazy_static::lazy_static;
use x86_64::{
    instructions::interrupts,
    set_general_handler,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame},
};

// tools

/// enables interrupts
pub fn enable() {
    interrupts::enable();
}

pub fn enable_and_halt() {
    interrupts::enable_and_hlt();
}

/// disables interrupts
pub fn disable() {
    interrupts::disable();
}

/// returns wether interrupts are enabled
pub fn are_enabled() -> bool {
    interrupts::are_enabled()
}

/// runs a function without interrupts.
///
/// this keeps them disabled if they were already
///
/// see [`interrupts::without_interrupts`]
pub fn without_interrupts<R>(f: impl FnOnce() -> R) -> R {
    interrupts::without_interrupts(f)
}

/// triggers the nth software interrupt
/// # Safety
/// see [`interrupts::software_interrupt`]
pub unsafe fn trigger<const N: u8>() {
    unsafe { interrupts::software_interrupt::<N>() };
}

// IDT
lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        fn general_handler(
            stack_frame: InterruptStackFrame,
            index: u8,
            error_code: Option<u64>,
        ) {
            todo!("handle irq {}\n\nerr code: {error_code:?}\n\n{stack_frame:#?}", index);
        }

        set_general_handler!(&mut idt, general_handler);

        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.invalid_opcode.set_handler_fn(ud::invalid_opcode);
        unsafe {
            idt.page_fault.set_handler_fn(page_fault::page_fault)
                .set_stack_index(page_fault::PAGE_FAULT_IST_INDEX);
            idt.double_fault.set_handler_fn(double_fault::double_fault)
                .set_stack_index(double_fault::DOUBLE_FAULT_IST_INDEX);
        }
        idt.general_protection_fault.set_handler_fn(gp::protection_fault);

        // Hardware Interrupts.

        // APIC

        unsafe {
            idt[TIMER_VEC]
                .set_handler_fn(thread::timer_trampoline)
                .set_stack_index(thread::TIMER_IST);
            idt[YIELD_INTERRUPT_NUMBER]
                .set_handler_fn(thread::yield_trampoline)
                .set_stack_index(thread::YIELD_IST);
        }

        idt[KEYBOARD_VEC]
            .set_handler_fn(keyboard::keyboard_interrupt_handler);

        idt
    };
}

const TIMER_VEC: u8 = 48;
const KEYBOARD_VEC: u8 = 49;

static mut IOAPIC: IoApic = unsafe { IoApic::new(0xFEC00000) };

/// inits the IDT, GDT, TSS, PIC8259, and interrupts.
pub fn init_interrupt_operations() {
    gdt::init_cpu(0);
    IDT.load();
    unsafe {
        address_space::map_4k_into_space(
            &mut kernel_address_space(),
            VirtAddr::new(apic::lapic_base() as usize),
            PhysAddr::new(apic::lapic_base() as usize),
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_CACHE
                | PageTableFlags::NO_EXECUTE,
        );

        address_space::map_4k_into_space(
            &mut kernel_address_space(),
            VirtAddr::new(0xFEC00000),
            PhysAddr::new(0xFEC00000),
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_CACHE
                | PageTableFlags::NO_EXECUTE,
        );

        apic::init_cpu0_lapic();
        // Initialize BSP's per-CPU LAPIC entry (CPU0 is always at index 0)
        apic::init_bsp_cpu_lapic();
        apic::bsp_lapic().enable(0xFF);

        apic::bsp_lapic().setup_timer(TIMER_VEC, 10_000_000);

        IOAPIC.route_irq(1, KEYBOARD_VEC);
    };
}

pub fn init_interrupt_operations_ap(id: usize) {
    IDT.load();
    gdt::init_cpu(id);
    unsafe {
        // Register this CPU before looking up its LAPIC. The BSP setup path
        // owns the shared IOAPIC and must not be run on application processors.
        apic::enable_apic_msr();
        let lapic = apic::init_cpu_lapic(id);
        lapic.enable(0xFF);
        lapic.setup_timer(TIMER_VEC, 10_000_000);
    }
}

pub fn ioapic() -> &'static IoApic {
    unsafe { &IOAPIC }
}

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    libc::error::set_errno(3); // note: reads TLS
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
        assert_eq!(*libc::get_errno(), 3)
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

    pub fn execute<R>(f: impl FnOnce() -> R) -> R {
        let guard = Self::new();

        let res = f();

        drop(guard);

        res
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
mod gdt;

pub use gdt::selectors as gdt_selectors;

mod ioapic;
/// Keyboard Interrupt Handling.
pub mod keyboard;
mod page_fault;
mod thread;
pub use thread::{YieldReason, set_yield_reason, ticks, trigger_yield};

pub use apic::LocalApic;
pub use apic::cpu_lapic;
pub use apic::current_cpu_lapic;
pub use apic::init_bsp_cpu_lapic;
pub use apic::init_cpu_lapic;
pub use apic::init_cpu0_lapic;
pub use apic::lapic_base;
pub(crate) use thread::InterruptRegs;
mod gp;
mod ud;

pub(in crate::arch::x86_64) mod apic;
