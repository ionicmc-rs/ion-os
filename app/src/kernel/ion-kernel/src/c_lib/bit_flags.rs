//! Tools for reading Bit Flags.
//! 
//! This is likely better places outside of c_lib, but we place it here for the boot info bit flags (CPUID(1), `edx` and `ecx`)

#![allow(private_bounds)]
use core::{fmt::{Binary, Debug, Display}, ops::{Bound, RangeBounds}};

use crate::c_lib::bit::{IntoBit, read_bit, set_bit};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
/// A Collection of flags
/// 
/// the type may be stored as any positive integer (except for u128 due to performance concerns), u32 by default
pub struct BitFlags<Int: Uint = u32> {
    int: Int
}

const trait Uint: Copy {
    const U_MAX: Self;
    const ZEROED: Self;
    fn into_usize(self) -> usize;
    fn from_usize(uint: usize) -> Self;
}

macro impl_uint($($T:ty)*) {
    $(
        impl const Uint for $T {
            const U_MAX: Self = Self::MAX; 
            const ZEROED: Self = 0;
            fn into_usize(self) -> usize {
                self as usize
            }
            fn from_usize(uint: usize) -> Self {
                uint as Self
            }
        }
    )*
}

impl_uint!(u8 u16 u32 u64 usize);

impl<T: Uint + Binary> Debug for BitFlags<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f
            .debug_struct("BitFlags")
            .field_with("int", |f| write!(f, "{:#b}", self.int))
            .finish()
    }
}

impl<T: Uint + Binary> Display for BitFlags<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#b}", self.int)
    }
}

/// An Error for Setting a Region.
/// 
/// includes:
/// - the upper bound,
/// - the lower bound,
/// - the length of the slice
/// 
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct SetRegionError {
    upper_bound: u8,
    lower_bound: u8,
    slice_len: usize
}

impl Display for SetRegionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Expected a slice for the range {l}->{h} ({r}), but instead got a slice of len {sl}", l=self.lower_bound, h=self.upper_bound, r=self.upper_bound-self.lower_bound, sl=self.slice_len)
    }
}

impl core::error::Error for SetRegionError {}

impl<Int: Uint> BitFlags<Int> {
    /// Creates a new `BitFlags` using the passed in integer.
    pub const fn new(i: Int) -> Self {
        Self { int: i }
    }

    /// Creates a new unset `BitFlags`
    pub const fn new_unset() -> Self {
        Self { int: Int::ZEROED }
    }

    /// Reads the `n`th flag.
    pub const fn read_flag(&self, n: usize) -> bool 
    where  
        Int: [const] Uint
    {
        read_bit(self.int.into_usize(), n)
    }

    /// Sets the `n`th flag
    pub const fn set_flag(&mut self, n: usize, flag: bool)
    where
        Int: [const] Uint + [const] IntoBit
    {
        let mut res = self.int.into_usize(); // result will not be much bigger
        set_bit(&mut res, n, flag);
        self.int = Int::from_usize(res);
    }

    /// Set a region from a range.
    /// # Errors
    /// This returns an error if the range's length is not exactly that of `vals`
    pub fn set_region<R: RangeBounds<u8>>(&mut self, region: R, vals: &[bool]) -> Result<(), SetRegionError> 
    where 
        Int: IntoBit
    {
        let upper = match region.end_bound() {
            core::ops::Bound::Unbounded => Self::flag_count(),
            core::ops::Bound::Excluded(e) => (*e - 1) as usize,
            core::ops::Bound::Included(i) => *i as usize
        };
        let lower = match region.start_bound() {
            core::ops::Bound::Excluded(e) => (*e + 1) as usize,
            core::ops::Bound::Included(e) => *e as usize,
            core::ops::Bound::Unbounded => 0
        };
        if vals.len() != upper - lower {
            return Err(SetRegionError { upper_bound: upper as u8, lower_bound: lower as u8, slice_len: vals.len() });
        }
        for (n, item) in vals.iter().enumerate().take(upper + 1).skip(lower) {
            self.set_flag(n, *item);
        }
        Ok(())
    }

    /// reads the region into the buffer, returning a slice of the region.
    pub fn read_region_into<'a, R: RangeBounds<u8>>(&'a self, region: R, buf: &'a mut [bool]) -> &'a [bool] {
        let upper = match region.end_bound() {
            Bound::Unbounded => Self::flag_count(),
            Bound::Excluded(e) => (*e - 1) as usize,
            Bound::Included(i) => *i as usize,
        };

        let lower = match region.start_bound() {
            Bound::Excluded(e) => (*e + 1) as usize,
            Bound::Included(e) => *e as usize,
            Bound::Unbounded => 0,
        };

        for (n, item) in buf.iter_mut().enumerate().take(upper + 1).skip(lower) {
            *item = self.read_flag(n);
        }

        &buf[lower..=upper]
    }

    /// returns the maximum count of flags.
    /// 
    /// less may be used, it is up to the caller.
    pub const fn flag_count() -> usize {
        // size is in bytes, each byte contains 8 bits.
        size_of::<Int>() * 8
    }

    /// Sets all flags to 0.
    pub const fn unset_all(&mut self) {
        // we will use mem zeroed to avoid having to come up with a type-independent solution

        // Safety: we insure the `Uint` trait is only ever implemented to numbers, which allow zeroing
        *self = unsafe { core::mem::zeroed() }
    }

    /// Sets all flags to 1.
    pub fn set_all(&mut self) {
        self.int = Int::U_MAX;
    }
}