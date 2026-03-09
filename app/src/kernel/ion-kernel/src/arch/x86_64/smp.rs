use core::{ptr::write_volatile, sync::atomic::{self, AtomicUsize, Ordering}};

use alloc::vec::Vec;
use linked_list_allocator::{align_down_size, align_up_size};

use crate::{address_space, arch::{self, interrupts::apic::{self, AcpiSdtHeader, Xsdt, rsdp}}, log};

const MAX_CPUS: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct CpuInfo {
    pub apic_id: u32,
}

const STACK_SIZE: usize = 4096 * 4;

static CPU_STACKS: [[u8; STACK_SIZE]; MAX_CPUS] = [[0; STACK_SIZE]; MAX_CPUS];
static mut CPUS: [Option<CpuInfo>; MAX_CPUS] = [None; MAX_CPUS];
static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn cpu_count() -> usize {
    CPU_COUNT.load(Ordering::Acquire)
}

pub fn cpu(index: usize) -> CpuInfo {
    unsafe { CPUS[index].unwrap() }
}

fn register_cpu(apic_id: u32) {
    let idx = CPU_COUNT.fetch_add(1, Ordering::SeqCst);

    if idx >= MAX_CPUS {
        panic!("Too many CPUs");
    }

    unsafe {
        CPUS[idx] = Some(CpuInfo { apic_id });
    }
}

#[repr(C, packed)]
struct MadtHeader {
    acpi: AcpiSdtHeader,
    lapic_addr: u32,
    flags: u32,
}

#[repr(C, packed)]
struct MadtEntryHeader {
    typ: u8,
    length: u8,
}

#[repr(C, packed)]
struct MadtLocalApic {
    typ: u8,
    length: u8,
    processor_id: u8,
    apic_id: u8,
    flags: u32,
}

const MADT_LOCAL_APIC: u8 = 0;

pub unsafe fn discover_cpus(madt_ptr: *const u8, madt_len: usize) {
    let mut offset = core::mem::size_of::<MadtHeader>();

    log::debug!("madt_len: {madt_len}");

    while offset < madt_len {
        log::debug!("loop");
        let entry = madt_ptr.add(offset) as *const MadtEntryHeader;
        let typ = (*entry).typ;
        let len = (*entry).length as usize;

        log::debug!(
            "entry = {entry:?}
typ = {typ}
len = {len}"
        );

        if len == 0 {
            break;
        }

        match typ {
            MADT_LOCAL_APIC => {
                let lapic = &*(entry as *const MadtLocalApic);

                log::debug!("candidate cpu: {}", lapic.apic_id);

                // CPU usable if enabled flag set
                if lapic.flags & 0b11 != 0 {
                    register_cpu(lapic.apic_id as u32);
                }
            }

            _ => {}
        }

        offset += len;
    }
}

// pub unsafe fn find_madt(xsdt_addr: u64) -> Option<(*const u8, usize)> {
//     log::debug!("finding madt");
//     let xsdt = &*(xsdt_addr as *const Xsdt);

//     let entries =
//         (xsdt.header.length as usize - core::mem::size_of::<AcpiSdtHeader>()) / 8;

//     let entries_base =
//         (xsdt_addr as *const u8).add(core::mem::size_of::<AcpiSdtHeader>()) as *const u64;

//     log::debug!("entries: {entries}");

//     for i in 0..entries {
//         let table_addr = core::ptr::read_unaligned(entries_base.add(i));
//         let table = table_addr as *const AcpiSdtHeader;

//         if (*table).signature == *b"APIC" {
//             log::debug!("MADT found: {table:?}");
//             return Some((table as *const u8, (*table).length as usize));
//         }
//     }

//     None
// }

unsafe fn find_madt_table(addr: u64, entry_size: usize) -> Option<(*const u8, usize)> {
    let sdt = &*(addr as *const AcpiSdtHeader);

    let entries =
        (sdt.length as usize - core::mem::size_of::<AcpiSdtHeader>()) / entry_size;

    let base =
        (addr as *const u8).add(core::mem::size_of::<AcpiSdtHeader>());

    for i in 0..entries {
        let table_addr = match entry_size {
            8 => core::ptr::read_unaligned(base.add(i * 8) as *const u64),
            4 => core::ptr::read_unaligned(base.add(i * 4) as *const u32) as u64,
            _ => unreachable!(),
        };

        let table = table_addr as *const AcpiSdtHeader;

        if (*table).signature == *b"APIC" {
            return Some((table as *const u8, (*table).length as usize));
        }
    }

    None
}

pub fn get_madt() -> (*const u8, usize) {
    let rsdp = rsdp().expect("could not get rsdp");
    let xsdt = rsdp.xsdt_address;

    log::debug!("rsdp: {rsdp:#?}\nxsdt: {xsdt:#x}");

    if rsdp.revision >= 2 && rsdp.xsdt_address != 0 {
        unsafe { find_madt_table(rsdp.xsdt_address, 8) }
    } else {
        unsafe { find_madt_table(rsdp.rsdt_address as u64, 4) }
    }.expect("could not parse XSDT/RSDT for MADT table")
}

unsafe extern "C" {
    static mut ap_pml4_ptr: u64;
    static mut ap_stack: u64;
}

pub unsafe fn start_aps(trampoline: u64) {
    let cpus = CPUS
        .clone()
        .into_iter()
        .flatten();

    let lapic = apic::cpu_lapic();
    let vector = (trampoline >> 12) as u8;

    let bsp = lapic.id();

    for (i, cpu) in cpus.enumerate() {
        if cpu.apic_id == bsp {
            continue;
        }

        unsafe { 
            write_volatile(unsafe { &mut ap_stack }, 
            align_up_size(
                (&CPU_STACKS[i] as *const [u8; STACK_SIZE])
                    .wrapping_add(1) as usize, 
                    16
                ) as u64
            ) 
        };
        unsafe { write_volatile(unsafe { &mut ap_pml4_ptr }, arch::mmu::active_pml4().as_usize() as u64) };

        atomic::fence(Ordering::SeqCst);

        lapic.send_init_ipi(cpu.apic_id);

        spin_delay(100000);

        lapic.send_init_deassert();

        spin_delay(100000);

        lapic.send_startup_ipi(cpu.apic_id, vector);

        spin_delay(10000);

        lapic.send_startup_ipi(cpu.apic_id, vector);
    }
}

#[inline]
pub fn spin_delay(mut cycles: u64) {
    while cycles > 0 {
        core::hint::spin_loop();
        cycles -= 1;
    }
}