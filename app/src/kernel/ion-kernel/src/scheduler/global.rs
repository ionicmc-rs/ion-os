use alloc::{boxed::Box, vec::Vec};

use crate::{
    arch, privilege::UserID, smp::{self, CpuID}, thread::{TPriority, Thread},
};

pub struct CpuData {
    id: CpuID,
    workload: CpuWorkload,
}

pub struct ThreadController {
    division: DivisionType,
    cpus: Vec<CpuData>,
}

pub enum CpuWorkload {
    Normal,
    Kernel,
    User,
    UserPrivileged,
    Priority(TPriority),
    Emergency,
}

pub enum DivisionType {
    /// Used exclusively on single-core systems.
    Single,

    /// All CPUs share the same workload.
    Equal,

    /// Separates kernel and userspace workloads.
    KernelUsers {
        kernel_cores: Vec<CpuID>,
        user_cores: Vec<CpuID>,
    },

    /// Separates kernel, regular userspace and privileged userspace.
    KernelUserPrivileged {
        kernel_cores: Vec<CpuID>,
        reg_user_cores: Vec<CpuID>,
        privileged_user_cores: Vec<CpuID>,
    },

    /// Assigns dedicated CPUs to high and low priority threads.
    /// Medium priority threads can use multiple CPUs.
    Priority {
        low: CpuID,
        high: CpuID,
        med_cores: Vec<CpuID>,
    },

    /// Reserves one CPU exclusively for emergency processes.
    /// All other workloads are delegated to `remaining`.
    EmergencyCore {
        emergency_core: CpuID,
        remaining: Box<DivisionType>,
    },

    /// Distributes work according to per-CPU workload weights.
    ///
    /// The weights must sum to 100.
    /// A weight of 0 excludes a CPU from normal scheduling.
    ///
    /// The vector follows the ordering of `ThreadController::cpus`.
    PerCpuWorkload {
        workloads: Vec<u8>,
    },
}

fn cpu_active_threads(id: CpuID) -> usize {
    id.storage().scheduler.active_thread_count()
}

fn queue_thread_to_cpu(id: CpuID, priority: TPriority, thread: Thread) {
    id.storage().scheduler.queue_at(priority, thread);
}

impl ThreadController {

    pub const fn new() -> Self {
        Self {
            cpus: Vec::new(),
            division: DivisionType::Equal
        }
    }

    /// Queues a regular runnable thread.
    pub fn queue(&self, thread: Thread, priority: TPriority) {
        let cpu = self.choose_cpu(priority, thread.user());

        queue_thread_to_cpu(cpu, priority, thread);
    }

    /// Queues an emergency thread.
    ///
    /// Emergency threads use the reserved emergency CPU when one exists.
    pub fn queue_emergency(&self, thread: Thread, priority: TPriority) {
        let cpu = self.choose_cpu_inner(
            &self.division,
            priority,
            thread.user(),
            true,
        );

        queue_thread_to_cpu(cpu, priority, thread);
    }

    /// Chooses a CPU for a regular runnable thread.
    pub fn choose_cpu(
        &self,
        priority: TPriority,
        user: Option<UserID>,
    ) -> CpuID {
        self.choose_cpu_inner(
            &self.division,
            priority,
            user,
            false,
        )
    }

    fn choose_cpu_inner(
        &self,
        division: &DivisionType,
        priority: TPriority,
        user: Option<UserID>,
        emergency: bool,
    ) -> CpuID {
        match division {
            DivisionType::Single => {
                assert_eq!(
                    self.cpus.len(),
                    1,
                    "Single division requires exactly one CPU"
                );

                self.cpus[0].id
            }

            DivisionType::Equal => {
                Self::choose_from(
                    self.cpus.iter().map(|cpu| cpu.id)
                )
            }

            DivisionType::KernelUsers {
                kernel_cores,
                user_cores,
            } => {
                match user {
                    None => Self::choose_from(kernel_cores.iter().copied()),
                    Some(_) => Self::choose_from(user_cores.iter().copied()),
                }
            }

            DivisionType::KernelUserPrivileged {
                kernel_cores,
                reg_user_cores,
                privileged_user_cores,
            } => {
                match user {
                    None => Self::choose_from(kernel_cores.iter().copied()),

                    Some(id) if id == UserID::os() => {
                        Self::choose_from(privileged_user_cores.iter().copied())
                    }

                    Some(_) => {
                        Self::choose_from(reg_user_cores.iter().copied())
                    }
                }
            }

            DivisionType::Priority {
                low,
                high,
                med_cores,
            } => {
                match priority {
                    TPriority::High => *high,
                    TPriority::Low => *low,
                    TPriority::Med => {
                        Self::choose_from(med_cores.iter().copied())
                    }
                }
            }

            DivisionType::EmergencyCore {
                emergency_core,
                remaining,
            } => {
                if emergency {
                    *emergency_core
                } else {
                    self.choose_cpu_inner(
                        remaining,
                        priority,
                        user,
                        false,
                    )
                }
            }

            DivisionType::PerCpuWorkload { workloads } => {
                Self::choose_weighted(
                    self.cpus.iter().map(|cpu| cpu.id).zip(workloads.iter().copied())
                )
            }
        }
    }

    /// Selects the CPU with the lowest active thread count.
    fn choose_from<I>(cpus: I) -> CpuID
    where
        I: IntoIterator<Item = CpuID>,
    {
        cpus.into_iter()
            .min_by_key(|id| cpu_active_threads(*id))
            .expect("CPU list must not be empty")
    }

    /// Selects a CPU according to its assigned workload weight.
    ///
    /// Compares active_threads / workload_weight.
    fn choose_weighted<I>(cpus: I) -> CpuID
    where
        I: IntoIterator<Item = (CpuID, u8)>,
    {
        cpus.into_iter()
            .filter(|(_, weight)| *weight > 0)
            .min_by(|(cpu_a, weight_a), (cpu_b, weight_b)| {
                let load_a =
                    cpu_active_threads(*cpu_a) as u128 * (*weight_b as u128);

                let load_b =
                    cpu_active_threads(*cpu_b) as u128 * (*weight_a as u128);

                load_a.cmp(&load_b)
            })
            .map(|(id, _)| id)
            .expect("No CPUs available for workload")
    }
}

static THREAD_CONTROLLER: spin::Mutex<ThreadController> = spin::Mutex::new(ThreadController::new());

pub fn init() {
    let count = arch::smp::cpu_count();
    let div = default_division(count);
    let ids = (0..count).map(|id| CpuData { id: CpuID(id), workload: CpuWorkload::Kernel /* TODO: proper workload info */ }).collect();
    let mut l = THREAD_CONTROLLER.lock();
    l.division = div;
    l.cpus = ids;
}

pub fn default_division(cpu_count: usize) -> DivisionType {
    use alloc::vec;
    match cpu_count {
        0 => panic!("No CPUs available"),

        1 => DivisionType::Single,

        2..=3 => DivisionType::Equal,

        4 => DivisionType::KernelUsers {
            kernel_cores: vec![CpuID(0)],
            user_cores: (1..4).map(CpuID).collect(),
        },

        5 => DivisionType::KernelUserPrivileged {
            kernel_cores: vec![CpuID(0)],
            privileged_user_cores: vec![CpuID(1)],
            reg_user_cores: (2..5).map(CpuID).collect(),
        },

        6.. => {
            let med_cores = (1..cpu_count - 1)
                .map(CpuID)
                .collect();

            DivisionType::Priority {
                low: CpuID(0),
                high: CpuID(cpu_count - 1),
                med_cores,
            }
        }
    }
}

pub fn queue(thread: Thread, priority: TPriority) {
    THREAD_CONTROLLER.lock().queue(thread, priority);
}

/// Wake keyboard-blocked threads regardless of which CPU owns them.
/// Keyboard interrupts are routed to the BSP, while the global scheduler may
/// place input consumers on any CPU.
pub fn wake_kbd() {
    let cpus: Vec<CpuID> = THREAD_CONTROLLER
        .lock()
        .cpus
        .iter()
        .map(|cpu| cpu.id)
        .collect();

    for cpu in cpus {
        cpu.storage().scheduler.wake_kbd();
    }
}
