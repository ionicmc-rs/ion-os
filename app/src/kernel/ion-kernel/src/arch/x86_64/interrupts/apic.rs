//! Local APIC support
//!
//! Minimal LAPIC implementation for early kernel use.
//! Assumes LAPIC is mapped at its physical address.

use core::mem::MaybeUninit;
use core::ptr::{read_volatile, write_volatile};

use x86_64::instructions::port::Port;
use x86_64::registers::model_specific::{ApicBase as IA32_APIC_BASE, ApicBaseFlags};

const APIC_ENABLE: u64 = 1 << 11;

const APIC_EOI: usize = 0xB0;
const APIC_SVR: usize = 0xF0;

const APIC_LVT_TIMER: usize = 0x320;
const APIC_TIMER_INIT: usize = 0x380;
const APIC_TIMER_DIV: usize = 0x3E0;

const APIC_LVT_LINT0: usize = 0x350;
const APIC_LVT_LINT1: usize = 0x360;
const APIC_LVT_ERROR: usize = 0x370;

const APIC_MASK: u32 = 1 << 16;


/// Local APIC interface
pub struct LocalApic {
    base: *mut u32,
}

impl LocalApic {
    /// Create LAPIC instance from base address
    pub const unsafe fn new(base: u64) -> Self {
        Self {
            base: base as *mut u32,
        }
    }
    
    #[inline(always)]
    unsafe fn read(&self, offset: usize) -> u32 {
        read_volatile(self.base.add(offset / 4))
    }
    
    #[inline(always)]
    unsafe fn write(&self, offset: usize, value: u32) {
        write_volatile(self.base.add(offset / 4), value);
    }
    
    /// Enable the LAPIC
    pub unsafe fn enable(&self, vector: u8) {
        let value = (vector as u32) | (1 << 8);
        self.mask_all_lvt();
        self.write(APIC_SVR, value);
    }
    
    /// Send End Of Interrupt
    pub unsafe fn eoi(&self) {
        self.write(APIC_EOI, 0);
    }
    
    pub unsafe fn mask_all_lvt(&self) {
        self.write(APIC_LVT_TIMER, APIC_MASK);
        self.write(APIC_LVT_LINT0, APIC_MASK);
        self.write(APIC_LVT_LINT1, APIC_MASK);
        self.write(APIC_LVT_ERROR, APIC_MASK);
    }

    /// Setup LAPIC timer
    pub unsafe fn setup_timer(&self, vector: u8, initial: u32) {
        // divide by 1
        self.write(APIC_TIMER_DIV, 0b1011);

        // vector
        self.write(APIC_LVT_TIMER, (vector as u32) | (1 << 17));

        // initial count
        self.write(APIC_TIMER_INIT, initial);
    }
}

/// Disable legacy PIC
pub unsafe fn disable_pic() {
    let mut pic1 = Port::<u8>::new(0x21);
    let mut pic2 = Port::<u8>::new(0xA1);

    pic1.write(0xFF);
    pic2.write(0xFF);
}

/// Enable APIC via MSR
pub unsafe fn enable_apic_msr() {
    let mut base = IA32_APIC_BASE::read();
    base.1 |= ApicBaseFlags::LAPIC_ENABLE;
    IA32_APIC_BASE::write(base.0, base.1);
}

pub fn lapic_base() -> u64 {
    let (frame, _) = IA32_APIC_BASE::read();
    frame.start_address().as_u64()
}

/// Initialize Local APIC
pub unsafe fn init() -> LocalApic {
    disable_pic();
    enable_apic_msr();

    let base = lapic_base();

    let lapic = LocalApic::new(base);

    // enable with spurious vector
    lapic.enable(0xFF);

    lapic
}

/// # Safety
/// must be called from CPU0
pub unsafe fn init_cpu0_lapic() {
    CPU0_LAPIC.write(LocalApic::new(lapic_base()));
}

static mut CPU0_LAPIC: MaybeUninit<LocalApic> = MaybeUninit::uninit();

pub fn cpu_lapic() -> &'static LocalApic {
    unsafe { CPU0_LAPIC.assume_init_ref() }
}

/// Contains the basic handlers for Hardware Interrupts.
pub mod handlers {
    /// Notifies that the interrupt handler has ended.
    ///
    /// Requires an explicit `unsafe` keyword.
    pub macro notify(unsafe $name:ident) {
        unsafe {
            super::cpu_lapic().eoi();
        }
    }
}
