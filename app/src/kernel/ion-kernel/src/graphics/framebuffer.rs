use core::mem::MaybeUninit;

use linked_list_allocator::align_up_size;

use crate::{boot_info, disk::default_drivers::virtio::map_mmio_frame, graphics::Pos, log, thread::{self, sync::Mutex}};

#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    pub phys_addr: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u8,
}

#[derive(Debug)]
pub struct Framebuffer {
    base: *mut u32,
    info: FramebufferInfo,
}

impl Framebuffer {
    /// # Safety
    /// `virt_addr` must be a valid, mapped framebuffer region.
    pub unsafe fn new(virt_addr: u64, info: FramebufferInfo) -> Self {
        Self {
            base: virt_addr as *mut u32,
            info,
        }
    }

    pub fn info(&self) -> &FramebufferInfo {
        &self.info
    }
}

impl Framebuffer {
    pub fn put_pixel(&mut self, x: u32, y: u32, color: u32) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let bytes_per_pixel = (self.info.bpp / 8) as usize;
        let offset = y as usize * self.info.pitch as usize
               + x as usize;

        unsafe {
            let ptr = self.base.add(offset);

            match bytes_per_pixel {
                4 => {
                    // XRGB / ARGB / BGRA — depends on bootloader
                    core::ptr::write_unaligned(ptr, color);
                }
                3 => {
                    // 24-bit RGB
                    let ptr = ptr.cast::<u8>();
                    let c = color.to_le_bytes();
                    core::ptr::write_volatile(ptr,     c[0]);
                    core::ptr::write_volatile(ptr.add(1), c[1]);
                    core::ptr::write_volatile(ptr.add(2), c[2]);
                }
                _ => {
                    // unsupported for now
                }
            }
        }
    }
}

impl Framebuffer {
    pub fn clear(&mut self, color: u32) {
        if self.info.pitch == self.info.width * 4 {
            self.clear_optimized(color);
        } else {
            self.fallback_clear(color);
        }
    }

    fn fallback_clear(&mut self, color: u32) {
        let stride = self.info.pitch as usize;

        for y in 0..self.info.height as usize {
            for x in 0..self.info.width as usize {
                unsafe {
                    *self.base.add(y * stride + x) = color;
                }
            }
        }
    }


    fn clear_optimized(&mut self, color: u32) {
        let pixels = (self.info.height * self.info.pitch / 4) as usize;
        let fb = unsafe {
            core::slice::from_raw_parts_mut(self.base, pixels)
        };
        fb.fill(color);
    }
}

use crate::graphics::pos::Rect;
use crate::graphics::color::Color;

impl Framebuffer {
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        // Ensure stride is in PIXELS if base is *mut u32
        let stride = self.info.pitch as usize; 

        let start_x = rect.x().min(self.info.width);
        let start_y = rect.y().min(self.info.height);
        let max_x = rect.right().min(self.info.width);
        let max_y = rect.bottom().min(self.info.height);

        if start_x >= max_x || start_y >= max_y {
            return;
        }

        let len = (max_x - start_x) as usize;

        for y in start_y..max_y {
            unsafe {
                let row_ptr = self.base.add(y as usize * stride + start_x as usize);
                core::slice::from_raw_parts_mut(row_ptr, len).fill(color.raw());
            }
        }
    }

}


static FRAMEBUFFER: Mutex<MaybeUninit<Framebuffer>> = Mutex::new(MaybeUninit::uninit());

pub fn init() {
    let fb_info = unsafe { boot_info().frame_buffer.read() };
    match fb_info.fb_type {
        crate::c_lib::FBType::Text => return,
        crate::c_lib::FBType::Indexed => todo!(),
        crate::c_lib::FBType::Rgb => {}
    }
    log::info!(
        "FB: {}x{} pitch={} bpp={}",
        fb_info.width,
        fb_info.height,
        fb_info.pitch,
        fb_info.bpp,
    );


    let mut lock = FRAMEBUFFER.lock();

    let pitch_pixels = if fb_info.pitch == fb_info.width {
        fb_info.pitch // already pixels
    } else {
        fb_info.pitch / (fb_info.bpp / 8) as u32 // bytes → pixels
    };

    let res_info = FramebufferInfo {
        bpp: fb_info.bpp,
        phys_addr: fb_info.addr.addr().get() as u64,
        height: fb_info.height,
        pitch: pitch_pixels,
        width: fb_info.width,
    };


    let base = thread::critical_moment(|| {
        const PAGE_2M: usize = 2 * 1024 * 1024;

        let size = res_info.pitch as u64 * res_info.height as u64;
        let aligned_size = align_up_size(size as usize, PAGE_2M);

        let phys_base = res_info.phys_addr & !(PAGE_2M as u64 - 1);

        let base = unsafe { map_mmio_frame(phys_base as usize) };

        let pages = aligned_size / PAGE_2M;
        for n in 1..pages {
            unsafe {
                map_mmio_frame((phys_base + (n * PAGE_2M) as u64) as usize);
            }
        }

        let offset = (res_info.phys_addr - phys_base) as usize;
        unsafe { (base as *mut u8).add(offset).cast::<u32>() }
    });


    lock.write(Framebuffer { base, info: res_info });
}

pub fn clear(color: Color) {
    let mut lock = FRAMEBUFFER.lock();
    unsafe { lock.assume_init_mut() }.clear(color.raw());
}

pub fn put_pixel(pos: Pos, color: Color) {
    let mut lock = FRAMEBUFFER.lock();
    unsafe { lock.assume_init_mut() }.put_pixel(pos.x, pos.y, color.raw());
}

pub fn draw_rect(rect: Rect, color: Color) {
    let mut lock = FRAMEBUFFER.lock();
    unsafe { lock.assume_init_mut() }.fill_rect(rect, color);
}

pub unsafe fn force_unlock() {
    unsafe { FRAMEBUFFER.force_unlock() }
}

pub unsafe fn clear_poison() {
    unsafe { FRAMEBUFFER.clear_poison() }
}