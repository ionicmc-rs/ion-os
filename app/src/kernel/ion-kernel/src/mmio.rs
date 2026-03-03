use core::sync::atomic::{AtomicUsize, Ordering};

use linked_list_allocator::align_up_size;

use crate::{address_space::{kernel_address_space, map_2mib_into_space, map_4k_into_space}, arch::{PhysAddr, VirtAddr, mmu::PageTableFlags}, log};

const MMIO_TOTAL_SIZE: usize = 1024 * 1024 * 1024; // 1 GiB

const MMIO_2M_REGION_SIZE: usize = MMIO_TOTAL_SIZE / 2;
const MMIO_4K_REGION_SIZE: usize = MMIO_TOTAL_SIZE / 2;

const FRAME_2M: usize = 2 * 1024 * 1024;
const FRAME_4K: usize = 4 * 1024;

const MMIO_2M_MAX: usize = MMIO_2M_REGION_SIZE / FRAME_2M;
const MMIO_4K_MAX: usize = MMIO_4K_REGION_SIZE / FRAME_4K;

static MMIO_BASE: AtomicUsize = AtomicUsize::new(0);

static mut MMIO_2M_BITMAP: [u64; MMIO_2M_MAX / 64] =
    [0; MMIO_2M_MAX / 64];

static mut MMIO_4K_BITMAP: [u64; MMIO_4K_MAX / 64] =
    [0; MMIO_4K_MAX / 64];

fn ensure_mmio_base() -> usize {
    let base = MMIO_BASE.load(Ordering::Acquire);
    if base != 0 {
        return base;
    }

    // choose clean aligned base
    let computed = align_up_size(
        1024 * 1024 * 1024, // 1GiB
        FRAME_2M
    );

    MMIO_BASE.store(computed, Ordering::Release);
    computed
}

pub unsafe fn alloc_2m_contiguous(count: usize) -> Option<usize> {
    let max = MMIO_2M_MAX;

    for start in 0..=(max - count) {
        let mut ok = true;

        for i in 0..count {
            let idx = start + i;
            let word = idx / 64;
            let bit  = idx % 64;

            if (MMIO_2M_BITMAP[word] >> bit) & 1 == 1 {
                ok = false;
                break;
            }
        }

        if ok {
            for i in 0..count {
                let idx = start + i;
                let word = idx / 64;
                let bit  = idx % 64;
                MMIO_2M_BITMAP [word] |= 1u64 << bit;
            }


            log::debug!("mapping: {start}");
            return Some(start);
        }
    }

    None
}

pub unsafe fn alloc_4k_contiguous(count: usize) -> Option<usize> {
    let max = MMIO_4K_MAX;

    for start in 0..=(max - count) {
        let mut ok = true;

        for i in 0..count {
            let idx = start + i;
            let word = idx / 64;
            let bit  = idx % 64;

            if (MMIO_4K_BITMAP[word] >> bit) & 1 == 1 {
                ok = false;
                break;
            }
        }

        if ok {
            for i in 0..count {
                let idx = start + i;
                let word = idx / 64;
                let bit  = idx % 64;
                MMIO_4K_BITMAP[word] |= 1u64 << bit;
            }

            return Some(start);
        }
    }

    None
}

#[track_caller]
pub unsafe fn map_mmio_frame(phys: usize) -> *mut u8 {
    assert!(phys % FRAME_2M == 0);

    let base = ensure_mmio_base();

    let slot = unsafe { alloc_2m_contiguous(1).expect("MMIO 2MiB exhausted") };

    let virt = base + slot * FRAME_2M;

    unsafe { map_2mib_into_space(
        &mut kernel_address_space(),
        VirtAddr::new(virt),
        PhysAddr::new(phys),
        PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::NO_EXECUTE
            | PageTableFlags::NO_CACHE,
    ) };

    virt as *mut u8
}

#[track_caller]
pub unsafe fn map_mmio_4k_frame(phys: usize) -> *mut u8 {
    assert!(phys % FRAME_4K == 0);

    let base = ensure_mmio_base();

    let region_base = base + MMIO_2M_REGION_SIZE;

    let slot = unsafe { alloc_4k_contiguous(1).expect("MMIO 4KiB exhausted") };

    let virt = region_base + slot * FRAME_4K;

    unsafe { map_4k_into_space(
        &mut kernel_address_space(),
        VirtAddr::new(virt),
        PhysAddr::new(phys),
        PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::NO_EXECUTE
            | PageTableFlags::NO_CACHE,
    ) };

    virt as *mut u8
}

pub unsafe fn free_2m(slot: usize) {
    let word = slot / 64;
    let bit = slot % 64;
    MMIO_2M_BITMAP[word] &= !(1u64 << bit);
}

pub unsafe fn free_4k(slot: usize) {
    let word = slot / 64;
    let bit = slot % 64;
    MMIO_4K_BITMAP[word] &= !(1u64 << bit);
}

pub fn mmio_base() -> usize {
    ensure_mmio_base()
}