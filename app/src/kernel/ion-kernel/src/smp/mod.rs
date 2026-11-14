use core::sync::atomic::{AtomicBool, Ordering};

use crate::{
    arch::{
        self, interrupts, smp::{self, cpu, discover_cpus, get_madt},
    }, cls::{self, CpuLocalStorage}, hlt_loop, log, thread,
};

pub struct CpuInfo(smp::CpuInfo);

static AP_SCHEDULERS_READY: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    static smp_start16: u8;
}

pub fn start() {
    let madt = get_madt();

    unsafe { discover_cpus(madt.0, madt.1) };

    for x in 0..smp::cpu_count() {
        let cpu = smp::cpu(x);

        log::debug!("discovered CPU{x}: {cpu:#?}");
    }

    if smp::cpu_count() <= 1 {
        log::debug!("no other cores found!");
    }

    // Initialize CPU local storage for BSP
    cls::init();

    // actual start

    unsafe { smp::start_aps(&smp_start16 as *const u8 as u64) };
}

pub fn init_cpu0_cls() {
    let cls = cls::cpu_cls(0);
    if let Some(cls) = cls {
        cls.scheduler.init();
    }
}

/// Allow application processors to begin scheduling after the shared
/// scheduler's CPU table has been initialized by the BSP.
pub fn release_ap_schedulers() {
    AP_SCHEDULERS_READY.store(true, Ordering::Release);
}

#[unsafe(no_mangle)]
pub extern "C" fn ap_entry() -> ! {
    // The AP's LAPIC MMIO mapping is not guaranteed to be usable until its
    // interrupt setup has run. CPUID leaf 1 reports the initial APIC ID
    // without touching that mapping.
    let apic_id = unsafe { core::arch::x86_64::__cpuid(1).ebx >> 24 };
    if let Some(index) = smp::cpu_index_from_apic_id(apic_id) {
        arch::interrupts::init_interrupt_operations_ap(index);

        // Initialize CLS for this AP (using its index in the CPU array)
        let cls = cls::cpu_cls(index);
        if let Some(cls) = cls {
            cls.scheduler.init();
        }

        log::debug!("AP online: cpu index {index}, apic id {apic_id}");

        while !AP_SCHEDULERS_READY.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }

        // Give this CPU its scheduler after its idle thread and LAPIC are
        // ready. The AP will then run queued local work or its idle thread.
        unsafe { thread::start() };
    } else {
        log::error!("AP started with unrecognized APIC id {apic_id}");
    }

    hlt_loop()
}

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CpuID(pub usize);

impl CpuID {
    pub fn new(id: usize) -> Option<Self> {
        if id > smp::cpu_count() {
            None
        } else {
            Some(Self(id))
        }
    }

    pub const fn get(&self) -> usize {
        self.0
    }

    pub fn info(&self) -> CpuInfo {
        CpuInfo(cpu(self.0))
    }

    pub fn storage(&self) -> &'static CpuLocalStorage {
        cls::cpu_cls(self.get()).unwrap() // we know this CpuID exists
    }
}
