//! Ion OS libc-like math module

/// Result for a Division
/// 
/// 32-bit integers used.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivT {
    /// Quotient
    pub quot: i32,
    /// Remainder
    pub rem: i32,
}

/// Result for a Division
/// 
/// 64-bit integers used.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LDivT {
    /// Quotient
    pub quot: i64,
    /// Remainder
    pub rem: i64,
}

/// Absolute value of x
/// 
/// will return the value as +ve if it is -ve.
#[unsafe(no_mangle)]
pub extern "C" fn abs(x: i32) -> i32 {
    if x < 0 { -x } else { x }
}

/// Absolute value of x
/// 
/// will return the value as +ve if it is -ve.
#[unsafe(no_mangle)]
pub extern "C" fn labs(x: i64) -> i64 {
    if x < 0 { -x } else { x }
}

/// Division returning struct
#[unsafe(no_mangle)]
pub extern "C" fn div(num: i32, denom: i32) -> DivT {
    DivT {
        quot: num / denom,
        rem: num % denom,
    }
}

/// Division Returning struct
#[unsafe(no_mangle)]
pub extern "C" fn ldiv(num: i64, denom: i64) -> LDivT {
    LDivT {
        quot: num / denom,
        rem: num % denom,
    }
}

// Random number generator (LCG)
static mut NEXT: u32 = 1;

/// Random number using set value as seed.
/// 
/// This means these values are not to be trusted for safety.
#[unsafe(no_mangle)]
pub extern "C" fn rand() -> i32 {
    unsafe {
        NEXT = NEXT.wrapping_mul(1103515245).wrapping_add(12345);
        ((NEXT / 65536) % 32768) as i32
    }
}

/// Set the Random seed.
#[unsafe(no_mangle)]
pub extern "C" fn srand(seed: u32) {
    unsafe {
        NEXT = seed;
    }
}