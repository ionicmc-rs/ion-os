use core::mem::MaybeUninit;

use crate::address_space::{kernel_address_space, map_2mib_into_space};
use crate::arch::mmu::PageTableFlags;
use crate::arch::{PhysAddr, VirtAddr};
use crate::graphics::pixel::PixelFormat;
use crate::log;
use crate::{boot_info, graphics::Surface, thread::sync::Mutex};

use crate::mmio::{alloc_2m_contiguous, map_mmio_frame, mmio_base};

static FRAMEBUFFER: Mutex<MaybeUninit<Surface>> = Mutex::new(MaybeUninit::uninit());

pub fn init() {
    let fb = unsafe { boot_info().frame_buffer.read() };

    assert!(fb.bpp == 32, "Only 32bpp supported currently");

    let pitch_bytes = fb.pitch;
    let width = fb.width;
    let height = fb.height;
    let phys_addr = fb.addr.addr().get();

    let surface = unsafe { map_framebuffer(phys_addr, width, height, pitch_bytes, fb.bpp) };

    FRAMEBUFFER.lock().write(surface);
}

pub fn take() -> Surface {
    let mut lock = FRAMEBUFFER.lock();
    unsafe { lock.assume_init_read() }
}

pub unsafe fn clear_poison() {
    unsafe { FRAMEBUFFER.clear_poison() };
}

unsafe fn map_framebuffer(
    phys_addr: usize,
    width: u32,
    height: u32,
    pitch_bytes: u32,
    bpp: u8,
) -> Surface {
    const FRAME_2M: usize = 2 * 1024 * 1024;

    // ------------------------------------------------------------
    // 1. Determine pixel format
    // ------------------------------------------------------------

    let format = match bpp {
        32 => PixelFormat::Xrgb8888,
        24 => PixelFormat::Rgb888,
        _ => panic!("Unsupported framebuffer bpp: {}", bpp),
    };

    // ------------------------------------------------------------
    // 2. Compute physical base and offset
    // ------------------------------------------------------------

    let phys_base = phys_addr & !(FRAME_2M - 1);

    let offset = (phys_addr - phys_base) as usize;

    // ------------------------------------------------------------
    // 3. Compute total framebuffer size
    // ------------------------------------------------------------

    let total_bytes = pitch_bytes as usize * height as usize;

    // IMPORTANT: include offset
    let required_bytes = offset + total_bytes;

    // Round up to 2MiB pages
    let pages = (required_bytes + FRAME_2M - 1) / FRAME_2M;

    // ------------------------------------------------------------
    // 4. Allocate contiguous virtual MMIO region
    // ------------------------------------------------------------

    let slot = alloc_2m_contiguous(pages).expect("MMIO 2MiB region exhausted");

    let virt_base = mmio_base() + slot as usize * FRAME_2M;

    // ------------------------------------------------------------
    // 5. Map physical pages to contiguous virtual pages
    // ------------------------------------------------------------

    for n in 0..pages {
        unsafe {
            map_2mib_into_space(
                &mut kernel_address_space(),
                VirtAddr::new(virt_base + n * FRAME_2M),
                PhysAddr::new(phys_base + (n * FRAME_2M)),
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::NO_EXECUTE
                    | PageTableFlags::NO_CACHE,
            )
        };
    }

    // ------------------------------------------------------------
    // 6. Final pointer includes original offset
    // ------------------------------------------------------------

    let final_ptr = (virt_base as *mut u8).add(offset);

    // ------------------------------------------------------------
    // 7. Return surface
    // ------------------------------------------------------------

    log::debug!("phys_addr = {:#x}", phys_addr);
    log::debug!("phys_base = {:#x}", phys_base);
    log::debug!("offset = {:#x}", offset);
    log::debug!("pages = {}", pages);

    Surface {
        width,
        height,
        pitch_bytes,
        base: final_ptr,
        format,
    }
}
