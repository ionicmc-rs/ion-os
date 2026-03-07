//! Minimal IOAPIC support

use core::ptr::{read_volatile, write_volatile};

pub struct IoApic {
    base: *mut u8,
}

impl IoApic {
    pub const unsafe fn new(base: u64) -> Self {
        Self {
            base: base as *mut u8,
        }
    }

    unsafe fn write_reg(&self, reg: u32) {
        write_volatile(self.base.cast(), reg);
    }

    unsafe fn write_data(&self, value: u32) {
        write_volatile(self.base.add(0x10).cast(), value);
    }

    unsafe fn read_data(&self) -> u32 {
        read_volatile(self.base.add(0x10).cast())
    }

    unsafe fn read(&self, reg: u32) -> u32 {
        self.write_reg(reg);
        self.read_data()
    }

    unsafe fn write(&self, reg: u32, value: u32) {
        self.write_reg(reg);
        self.write_data(value);
    }

    pub unsafe fn route_irq(&self, irq: u8, vector: u8) {
        let redir = 0x10 + (irq as u32) * 2;

        let low = vector as u32; // fixed delivery, edge, active high, unmasked
        let high = 0 << 24;      // destination APIC ID (CPU0)

        self.write(redir, low);
        self.write(redir + 1, high);
    }
}