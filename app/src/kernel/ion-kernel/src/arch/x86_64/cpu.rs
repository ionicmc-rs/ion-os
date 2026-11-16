use core::{arch::x86_64::{__cpuid_count, CpuidResult}, sync::atomic::{Ordering, compiler_fence, fence as cfence}};

use alloc::string::String;
use conquer_once::spin::OnceCell;
use x86_64::instructions;

use crate::cls::current;

use super::smp;

/// Spin Loop Hint to the CPU
#[inline]
pub fn relax() {
    core::arch::x86_64::_mm_pause();
}

#[inline]
pub fn halt() {
    instructions::hlt();
}

/// spins forever, [`relax`]ing the CPU each iteration
///
/// This is not usually what you want - this does not halt the CPU. see [`hlt_loop`] for that.
#[inline]
pub fn spin() -> ! {
    loop {
        relax();
    }
}

/// Halts the CPU forever
///
/// use [`spin`] to spin without halting
#[inline]
pub fn halt_loop() -> ! {
    loop {
        halt();
    }
}

/// does nothing
///
/// useful for bypassing llvm optimizations
///
/// On `x86_64` this emits a `nop` instruction
#[inline]
pub fn nop() {
    instructions::nop();
}

/// Jumps to the given address
///
/// This function *may* return if the jumped-to code is exactly after this function call.
///
/// However, the behavior of this function is unspecified/unknown if the jumped-to code
/// ever returns to the caller of this function, as Rust may have made optimizations based on the assumption
/// (in some cases, adding a `ud2` instruction after this function call) that this function never returns.
/// # Safety
/// Jumping to an invalid address is undefined behavior.
///
/// Calling this with a function pointer is safe, as long as the function does not return.
#[inline]
pub unsafe fn jump(addr: usize) -> ! {
    debug_assert!(addr != 0);
    debug_assert!((addr as *const usize).is_aligned());

    unsafe {
        core::arch::asm!(
            "jmp {0}",
            in(reg) addr,
            options(noreturn)
        );
    }
}

/// Creates a CPU memory fence
///
/// This does **not** create a compiler fence - only a CPU fence.
#[inline(always)]
pub fn fence() {
    unsafe {
        core::arch::asm!("mfence", options(nostack, preserves_flags));
    }
}

/// Creates both a memory and CPU fence
#[inline(always)]
pub fn full_fence(order: Ordering) {
    cfence(order);
}

/// returns the core's id
#[inline]
pub unsafe fn core_id() -> usize {
    (unsafe { current().unwrap_unchecked().id }) as usize
}

use alloc::borrow::ToOwned;

pub fn x86_64_cpuid(leaf: u32, sub_leaf: u32) -> CpuidResult {
    unsafe { __cpuid_count(leaf, sub_leaf) }
}

// Cached information
static VENDOR_NAME: OnceCell<String> = OnceCell::uninit();
static BRAND_NAME: OnceCell<String> = OnceCell::uninit();

static FAMILY: OnceCell<u32> = OnceCell::uninit();
static MODEL: OnceCell<u32> = OnceCell::uninit();
static STEPPING: OnceCell<u32> = OnceCell::uninit();

/// Returns the CPU vendor string.
pub fn vendor_name() -> String {
    VENDOR_NAME.get_or_init(|| {
        let result = x86_64_cpuid(0, 0);

        let mut vendor = [0u8; 12];

        vendor[0..4].copy_from_slice(&result.ebx.to_le_bytes());
        vendor[4..8].copy_from_slice(&result.edx.to_le_bytes());
        vendor[8..12].copy_from_slice(&result.ecx.to_le_bytes());

        String::from_utf8_lossy(&vendor).into_owned()
    }).clone()
}

/// Returns the CPU brand string.
pub fn brand_name() -> String {
    BRAND_NAME.get_or_init(|| {
        let max_extended = x86_64_cpuid(0x80000000, 0).eax;

        if max_extended < 0x80000004 {
            return String::from("Unknown CPU");
        }

        let mut brand = [0u8; 48];

        for i in 0..3 {
            let result = x86_64_cpuid(0x80000002 + i, 0);
            let offset = i as usize * 16;

            brand[offset..offset + 4].copy_from_slice(&result.eax.to_le_bytes());
            brand[offset + 4..offset + 8].copy_from_slice(&result.ebx.to_le_bytes());
            brand[offset + 8..offset + 12].copy_from_slice(&result.ecx.to_le_bytes());
            brand[offset + 12..offset + 16].copy_from_slice(&result.edx.to_le_bytes());
        }

        String::from_utf8_lossy(&brand).trim().to_owned()
    }).clone()
}

/// Returns the CPU family.
pub fn family() -> u32 {
    *FAMILY.get_or_init(|| {
        let eax = x86_64_cpuid(1, 0).eax;

        let base_family = (eax >> 8) & 0xF;
        let extended_family = (eax >> 20) & 0xFF;

        if base_family == 0xF {
            base_family + extended_family
        } else {
            base_family
        }
    })
}

/// Returns the CPU model.
pub fn model() -> u32 {
    *MODEL.get_or_init(|| {
        let eax = x86_64_cpuid(1, 0).eax;

        let base_family = (eax >> 8) & 0xF;
        let base_model = (eax >> 4) & 0xF;
        let extended_model = (eax >> 16) & 0xF;

        if base_family == 0x6 || base_family == 0xF {
            (extended_model << 4) | base_model
        } else {
            base_model
        }
    })
}

/// Returns the CPU stepping.
pub fn stepping() -> u32 {
    *STEPPING.get_or_init(|| {
        x86_64_cpuid(1, 0).eax & 0xF
    })
}