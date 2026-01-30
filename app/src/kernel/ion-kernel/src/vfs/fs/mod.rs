//! Contains IonFS impl

pub mod parse;
pub mod alloc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IonFSVersion(u8, u8, u16);

impl PartialOrd for IonFSVersion {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IonFSVersion {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.2.cmp(&other.2)
            .then(self.1.cmp(&other.1))
            .then(self.0.cmp(&other.0))
    }
}

pub const ALLOWED_VERSIONS: &[[u8; 4]] = &[[0, 1, 0, 0]];

impl IonFSVersion {
    pub fn new(total: u8, major: u8, minor: u16) -> Option<Self> {
        let minor_bytes = minor.to_le_bytes();
        if !ALLOWED_VERSIONS.contains(
            &[total, major, minor_bytes[0], minor_bytes[1]]
        ) {
            return None;
        }
        Some(Self(total, major, minor))
    }

    pub fn from_array(array: [u8; 4]) -> Option<Self> {
        if !ALLOWED_VERSIONS.contains(&array) {
            return None;
        }
        Some(Self(array[0], array[1], u16::from_le_bytes([array[2], array[3]])))
    }

    pub fn to_tuple(self) -> (u8, u8, u16) {
        (self.0, self.1, self.2)
    }

    pub fn to_array(self) -> [u8; 4] {
        let minor_bytes = self.2.to_le_bytes();
        [self.0, self.1, minor_bytes[0], minor_bytes[1]]
    }
}