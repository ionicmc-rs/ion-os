use crate::{
    arch::{
        self, interrupts,
        smp::{self, discover_cpus, get_madt},
    },
    cls, hlt_loop, log,
};

pub struct CpuInfo(smp::CpuInfo);

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

#[unsafe(no_mangle)]
pub extern "C" fn ap_entry() -> ! {
    log::debug!("AP started!");

    let apic_id = unsafe { interrupts::LocalApic::new(interrupts::lapic_base()).id() };

    if let Some(index) = smp::cpu_index_from_apic_id(apic_id) {
        // Initialize the per-CPU LAPIC for this core
        unsafe { arch::interrupts::init_cpu_lapic(index) };

        // Initialize CLS for this AP (using its index in the CPU array)
        let cls = cls::cpu_cls(index);
        if let Some(cls) = cls {
            cls.scheduler.init();
        }

        log::debug!("AP online: cpu index {index}, apic id {apic_id}");
    }

    hlt_loop()
}
