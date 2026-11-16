use core::fmt::Display;

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemUnit {
    #[default]
    Bytes,
    KB,
    KiB,
    MB,
    MiB,
    GB,
    GiB,
    TB,
    TiB,
    PB,
    PiB,
    EB,
    EiB
    // No higher memory units supported for now
}

impl Display for MemUnit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MemDisplay {
    amount: f64,
    bytes: usize,
    unit: MemUnit
}

impl PartialEq for MemDisplay {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}
impl Eq for MemDisplay {}

impl Ord for MemDisplay {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl PartialOrd for MemDisplay {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub enum MemUnitSystem {
    SI,
    #[default]
    IEC,
}

impl MemDisplay {
    pub fn from_bytes(bytes: usize) -> Self {
        Self::from_bytes_with_system(bytes, MemUnitSystem::IEC)
    }

    pub fn from_bytes_with_system(bytes: usize, system: MemUnitSystem) -> Self {
        let units: &[(MemUnit, u64)] = match system {
            MemUnitSystem::SI => &[
                (MemUnit::Bytes, 1),
                (MemUnit::KB, 1_000),
                (MemUnit::MB, 1_000_000),
                (MemUnit::GB, 1_000_000_000),
                (MemUnit::TB, 1_000_000_000_000),
                (MemUnit::PB, 1_000_000_000_000_000),
                (MemUnit::EB, 1_000_000_000_000_000_000),
            ],
            MemUnitSystem::IEC => &[
                (MemUnit::Bytes, 1),
                (MemUnit::KiB, 1 << 10),
                (MemUnit::MiB, 1 << 20),
                (MemUnit::GiB, 1 << 30),
                (MemUnit::TiB, 1 << 40),
                (MemUnit::PiB, 1 << 50),
                (MemUnit::EiB, 1 << 60),
            ],
        };

        let (unit, divisor) = units
            .iter()
            .rev()
            .find(|(_, divisor)| (bytes as u64) >= *divisor)
            .copied()
            .unwrap_or((MemUnit::Bytes, 1));

        Self {
            amount: bytes as f64 / divisor as f64,
            bytes,
            unit,
        }
    }
}

impl Display for MemDisplay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:.2} {}", self.amount, self.unit)
    }
}