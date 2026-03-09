//! Local APIC support
//!
//! Minimal LAPIC implementation for early kernel use.
//! Assumes LAPIC is mapped at its physical address.
#![allow(unsafe_op_in_unsafe_fn)]

use core::mem::MaybeUninit;
use core::ptr::{read_volatile, write_volatile};

use x86_64::instructions::port::Port;
use x86_64::registers::model_specific::{ApicBase as IA32_APIC_BASE, ApicBaseFlags};

use crate::{boot_info, log};
use crate::c_lib::{MultibootAcpi, MultibootTag};

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

const APIC_ID: usize = 0x20;

const APIC_ICR_LOW: usize = 0x300;
const APIC_ICR_HIGH: usize = 0x310;

const APIC_DELIVERY_PENDING: u32 = 1 << 12;


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

    pub unsafe fn id(&self) -> u32 {
        self.read(APIC_ID) >> 24
    }
    
    unsafe fn send_ipi_raw(&self, apic_id: u32, value: u32) {
        self.write(APIC_ICR_HIGH, apic_id << 24);
        self.write(APIC_ICR_LOW, value);

        while self.read(APIC_ICR_LOW) & APIC_DELIVERY_PENDING != 0 {}
    }
    
    pub unsafe fn send_init_ipi(&self, apic_id: u32) {
        const INIT: u32 = 0b101 << 8;
        const LEVEL: u32 = 1 << 14;
        const ASSERT: u32 = 1 << 15;

        self.send_ipi_raw(apic_id, INIT | LEVEL | ASSERT);
    }

    pub unsafe fn send_startup_ipi(&self, apic_id: u32, vector: u8) {
        const STARTUP: u32 = 0b110 << 8;

        self.send_ipi_raw(apic_id, STARTUP | vector as u32);
    }

    pub fn send_init_deassert(&self) {
        let icr = (0b101 << 8) | (1 << 15); // INIT, level trigger, level=0 (deassert)

        unsafe {
            self.write_icr(0, icr);
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

impl LocalApic {
    unsafe fn write_icr(&self, apic_id: u32, icr_low: u32) {
        let icr_high = apic_id << 24;

        // wait until previous IPI finished
        while core::ptr::read_volatile(self.base.add(0x300 / 4)) & (1 << 12) != 0 {
            core::hint::spin_loop();
        }

        // write destination
        core::ptr::write_volatile(self.base.add(0x310 / 4), icr_high);

        // send command
        core::ptr::write_volatile(self.base.add(0x300 / 4), icr_low);

        // wait for completion
        while core::ptr::read_volatile(self.base.add(0x300 / 4)) & (1 << 12) != 0 {
            core::hint::spin_loop();
        }
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

#[repr(C, packed)]
pub struct AcpiSdtHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}


#[repr(C, packed)]
pub struct Xsdt {
    pub header: AcpiSdtHeader,
    pub tables: [u64; 0],
}

#[repr(C, packed)]
#[derive(Debug)]
pub struct Rsdp {
    pub signature: [u8; 8],
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_address: u32,

    // ACPI 2+
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

pub fn rsdp() -> Option<&'static Rsdp> {
    let mut tag = boot_info().multiboot_info.into_inner().wrapping_byte_add(8);

    loop {
        let t = unsafe { &*tag };

        match t.typ {
            14 | 15 => {
                let acpi = tag as *const MultibootAcpi;
                return Some(unsafe { &*((*acpi).rsdp.as_ptr() as *const Rsdp) });
            }

            0 => return None,
            _ => {
                log::debug!("tag of type: {t:?}");
            }
        }

        tag = ((tag as usize + t.size as usize + 7) & !7) as *const MultibootTag;
    }
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
