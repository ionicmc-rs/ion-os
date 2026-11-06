use core::ptr;

use crate::graphics::pixel::PixelFormat;
use crate::graphics::{Color, Rect};

pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub pitch_bytes: u32,
    pub format: PixelFormat,
    pub base: *mut u8,
}

impl Surface {
    #[inline(always)]
    pub fn draw_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        unsafe {
            match self.format {
                PixelFormat::Xrgb8888 => {
                    let pitch = self.pitch_bytes as usize >> 2;
                    let fb = self.base as *mut u32;

                    fb.add(y as usize * pitch + x as usize)
                        .write_unaligned(color.raw());
                }

                PixelFormat::Rgb888 => {
                    let pitch = self.pitch_bytes as usize;
                    let off = y as usize * pitch + x as usize * 3;

                    let ptr = self.base.add(off);
                    let raw = color.raw().to_le_bytes();

                    ptr.write(raw[0]);
                    ptr.add(1).write(raw[1]);
                    ptr.add(2).write(raw[2]);
                }
            }
        }
    }

    pub fn clear(&mut self, color: Color) {
        self.fill_rect(Rect::new(0, 0, self.width, self.height), color);
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let start_x = rect.x().min(self.width);
        let start_y = rect.y().min(self.height);
        let end_x = rect.right().min(self.width);
        let end_y = rect.bottom().min(self.height);

        if start_x >= end_x || start_y >= end_y {
            return;
        }

        unsafe {
            match self.format {
                /* ----------------------------
                XRGB8888 FAST PATH
                ---------------------------- */
                PixelFormat::Xrgb8888 => {
                    let pitch = self.pitch_bytes as usize >> 2;
                    let fb = self.base as *mut u32;
                    let color = color.raw();

                    let width = (end_x - start_x) as usize;

                    for y in start_y..end_y {
                        let mut row = fb.add(y as usize * pitch + start_x as usize);

                        for _ in 0..width {
                            row.write_unaligned(color);
                            row = row.add(1);
                        }
                    }
                }

                /* ----------------------------
                RGB888 PATH
                ---------------------------- */
                PixelFormat::Rgb888 => {
                    let pitch = self.pitch_bytes as usize;
                    let raw = color.raw().to_le_bytes();
                    let width = (end_x - start_x) as usize;

                    for y in start_y..end_y {
                        let mut row = self.base.add(y as usize * pitch + start_x as usize * 3);

                        for _ in 0..width {
                            row.write(raw[0]);
                            row.add(1).write(raw[1]);
                            row.add(2).write(raw[2]);
                            row = row.add(3);
                        }
                    }
                }
            }
        }
    }

    pub fn blit(&mut self, src: &Surface, x: u32, y: u32) {
        if self.format != src.format {
            panic!("blit format mismatch");
        }

        if x >= self.width || y >= self.height {
            return;
        }

        let width = src.width.min(self.width - x);
        let height = src.height.min(self.height - y);
        let bytes_per_pixel = match self.format {
            PixelFormat::Xrgb8888 => 4,
            PixelFormat::Rgb888 => 3,
        };
        let copy_bytes = width as usize * bytes_per_pixel;

        unsafe {
            for row in 0..height {
                let src_ptr = src.base.add(row as usize * src.pitch_bytes as usize);
                let dst_ptr = self.base.add(
                    (y + row) as usize * self.pitch_bytes as usize + x as usize * bytes_per_pixel,
                );
                ptr::copy_nonoverlapping(src_ptr, dst_ptr, copy_bytes);
            }
        }
    }

    /// Copy a clipped rectangle from another surface.
    pub fn blit_region(&mut self, src: &Surface, rect: Rect) {
        if self.format != src.format {
            panic!("blit format mismatch");
        }

        let start_x = rect.x().min(self.width).min(src.width);
        let start_y = rect.y().min(self.height).min(src.height);
        let end_x = rect.right().min(self.width).min(src.width);
        let end_y = rect.bottom().min(self.height).min(src.height);

        if start_x >= end_x || start_y >= end_y {
            return;
        }

        let bytes_per_pixel = match self.format {
            PixelFormat::Xrgb8888 => 4,
            PixelFormat::Rgb888 => 3,
        };
        let copy_bytes = (end_x - start_x) as usize * bytes_per_pixel;

        unsafe {
            for row in start_y..end_y {
                let src_ptr = src.base.add(
                    row as usize * src.pitch_bytes as usize + start_x as usize * bytes_per_pixel,
                );
                let dst_ptr = self.base.add(
                    row as usize * self.pitch_bytes as usize + start_x as usize * bytes_per_pixel,
                );

                ptr::copy_nonoverlapping(src_ptr, dst_ptr, copy_bytes);
            }
        }
    }
}
