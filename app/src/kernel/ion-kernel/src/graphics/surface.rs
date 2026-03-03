use core::ptr;

use crate::graphics::{Color, Rect};
use crate::graphics::pixel::PixelFormat;

pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub pitch_bytes: u32,
    pub format: PixelFormat,
    pub base: *mut u8,
}

impl Surface {
    #[inline]
    pub fn draw_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        let bpp = match self.format {
            PixelFormat::Xrgb8888 => 4,
            PixelFormat::Rgb888   => 3,
        };

        let byte_offset =
            y as usize * self.pitch_bytes as usize +
            x as usize * bpp;

        unsafe {
            let ptr = self.base.add(byte_offset);

            match self.format {
                PixelFormat::Xrgb8888 => {
                    ptr::write_unaligned(
                        ptr as *mut u32,
                        color.raw(),
                    );
                }
                PixelFormat::Rgb888 => {
                    let raw = color.raw().to_le_bytes();
                    ptr::write(ptr,     raw[0]);
                    ptr::write(ptr.add(1), raw[1]);
                    ptr::write(ptr.add(2), raw[2]);
                }
            }
        }
    }

    pub fn clear(&mut self, color: Color) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.draw_pixel(x, y, color);
            }
        }
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let start_x = rect.x().min(self.width);
        let start_y = rect.y().min(self.height);
        let end_x   = rect.right().min(self.width);
        let end_y   = rect.bottom().min(self.height);

        for y in start_y..end_y {
            for x in start_x..end_x {
                self.draw_pixel(x, y, color);
            }
        }
    }

    pub fn blit(&mut self, src: &Surface, x: u32, y: u32) {
        if self.format != src.format {
            panic!("blit format mismatch");
        }

        let bpp = match self.format {
            PixelFormat::Xrgb8888 => 4,
            PixelFormat::Rgb888   => 3,
        };

        for row in 0..src.height {
            if y + row >= self.height {
                break;
            }

            for col in 0..src.width {
                if x + col >= self.width {
                    break;
                }

                unsafe {
                    let src_off =
                        row as usize * src.pitch_bytes as usize +
                        col as usize * bpp;

                    let dst_off =
                        (y + row) as usize * self.pitch_bytes as usize +
                        (x + col) as usize * bpp;

                    ptr::copy_nonoverlapping(
                        src.base.add(src_off),
                        self.base.add(dst_off),
                        bpp,
                    );
                }
            }
        }
    }
}