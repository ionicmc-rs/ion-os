//! Tools for reading bits.
//! 
//! use [`bit_flags`](super::bit_flags) for reading bit flags.
// /// Create a bool from a signed integer
// /// # Panics
// /// panics if i is not 1 or 0
// /// 
// /// if you want to check instead, use [`try_from_int`]
// pub const fn from_int(i: isize) -> bool {
//     if i == 0 {
//         false
//     } else if i == 1 {
//         true
//     } else {
//         panic!("integer is not 1 or 0 for bit::from_int")
//     }
// }

// /// Creates a bool from a signed integer, returning [`None`] if it is not 1 or 0.
// pub const fn try_from_int(i: isize) -> Option<bool> {
//     if i == 1 || i == 0 {
//         Some(from_int(i))
//     } else {
//         None
//     }
// }

// /// Create a bool from an un-singed integer
// /// # Panics
// /// panics if i is not 1 or 0
// /// 
// /// if you want to check instead, use [`try_from_int`]
// pub const fn from_uint(i: usize) -> bool {
//     if i == 0 {
//         false
//     } else if i == 1 {
//         true
//     } else {
//         panic!("integer is not 1 or 0 for bit::from_int")
//     }
// }

// /// Creates a bool from an un-signed integer, returning [`None`] if it is not 1 or 0.
// pub const fn try_from_uint(i: usize) -> Option<bool> {
//     if i == 1 || i == 0 {
//         Some(from_uint(i))
//     } else {
//         None
//     }
// }

// reading and writing

/// Trait for Converting into bits, represented as bool 
pub const trait IntoBit {
    /// converts the 1 or 0 into a bit,
    /// returning [`None`] if the value falls outside that range
    fn into_bit(self) -> Option<bool>;
}

macro impl_into_bit($($T:ty)*) {
    $(
        impl const IntoBit for $T {
            fn into_bit(self) -> Option<bool> {
                if self == 0 {
                    Some(false)
                } else if self == 1 {
                    Some(true)
                } else {
                    None
                }
            }
        }
    )*
}

impl_into_bit!(
    u8 u16 u32 u64 usize
    i8 i16 i32 i64 isize
);

impl const IntoBit for bool {
    fn into_bit(self) -> Option<bool> {
        Some(self)
    }
}

/// reads the nth bit
pub const fn read_bit(val: usize, bitn: usize) -> bool {
    (val & (1 << bitn)) != 0
}

/// sets the nth bit.
pub const fn set_bit(val: &mut usize, bitn: usize, bit: impl [const] IntoBit) {
    if bit.into_bit().unwrap() {
        *val |= 1 << bitn;
    } else {
        *val &= !(1 << bitn);
    }
}