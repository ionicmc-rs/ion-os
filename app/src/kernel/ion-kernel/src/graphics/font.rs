//! PSF1 font parsing + rendering
//!
//! - Typed error handling
//! - Zero allocations
//! - Safe for kernel panic paths

use core::{mem::size_of, sync::atomic::{AtomicBool, Ordering}};

use crate::{graphics::{self, Color, Pos, Surface, framebuffer}, log};

/// PSF1 on-disk header
#[repr(C)]
#[derive(Clone, Copy)]
struct Psf1Header {
    magic: [u8; 2],
    mode: u8,
    charsize: u8,
}

const PSF1_MAGIC: [u8; 2] = [0x36, 0x04];

/// Errors that can occur while parsing a PSF1 font
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontError {
    /// File is smaller than a PSF1 header
    FileTooSmall,

    /// Magic bytes are not 0x36, 0x04
    InvalidMagic {
        found: [u8; 2],
    },

    /// Glyph height is zero
    InvalidCharSize {
        charsize: u8,
    },

    /// File does not contain all glyph data
    Truncated {
        expected: usize,
        found: usize,
    },
}

/// Parsed bitmap font (graphics-facing)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BitmapFont<'a> {
    pub glyph_width: u8,   // always 8 for PSF1
    pub glyph_height: u8,
    pub glyph_count: u16,
    glyphs: &'a [u8],
}

static mut FALLBACK_FONT: Option<BitmapFont> = None;
static IS_FALLBACK_SET: AtomicBool = AtomicBool::new(false);

pub fn set_fallback_font(font: BitmapFont<'static>) {
    if IS_FALLBACK_SET.swap(true, Ordering::AcqRel) {
        panic!("set fallback font twice");
    }

    unsafe { FALLBACK_FONT = Some(font) }
}

pub fn fallback_font() -> BitmapFont<'static> {
    try_get_fallback_font().expect("should not access fallback font before creation")
}

pub fn try_get_fallback_font() -> Option<BitmapFont<'static>> {
    unsafe { FALLBACK_FONT }
}

impl<'a> BitmapFont<'a> {
    /// Parse a PSF1 font from raw bytes
    pub fn parse_psf1(data: &'a [u8]) -> Result<Self, FontError> {
        if data.len() < size_of::<Psf1Header>() {
            return Err(FontError::FileTooSmall);
        }

        let header = unsafe {
            &*(data.as_ptr() as *const Psf1Header)
        };

        if header.magic != PSF1_MAGIC {
            return Err(FontError::InvalidMagic {
                found: header.magic,
            });
        }

        let glyph_count = if header.mode & 0x01 != 0 {
            512usize
        } else {
            256usize
        };

        if header.charsize == 0 {
            return Err(FontError::InvalidCharSize {
                charsize: header.charsize,
            });
        }

        let glyph_height = header.charsize as usize;
        let header_size = size_of::<Psf1Header>();
        let glyph_bytes = glyph_count * glyph_height;
        let expected_size = header_size + glyph_bytes;

        if data.len() < expected_size {
            return Err(FontError::Truncated {
                expected: expected_size,
                found: data.len(),
            });
        }

        let glyphs = &data[header_size..expected_size];

        Ok(Self {
            glyph_width: 8,
            glyph_height: header.charsize,
            glyph_count: glyph_count as u16,
            glyphs,
        })
    }

    /// Get a glyph bitmap (one byte per row)
    pub fn glyph(&self, ch: u8) -> Option<&[u8]> {
        let idx = ch as usize;
        if idx >= self.glyph_count as usize {
            return None;
        }

        let h = self.glyph_height as usize;
        let start = idx * h;
        let end = start + h;

        Some(&self.glyphs[start..end])
    }
}

/* ============================
 * Rendering
 * ============================
 */

pub fn draw_glyph(
    surface: &mut Surface,
    x: u32,
    y: u32,
    font: &BitmapFont,
    glyph: &[u8],
    color: Color,
) {
    unsafe {
        match surface.format {

            /* =========================
               XRGB8888 FAST PATH
               ========================= */

            crate::graphics::pixel::PixelFormat::Xrgb8888 => {

                let pitch = surface.pitch_bytes as usize >> 2;
                let fb = surface.base as *mut u32;
                let color = color.raw();

                for (row, byte) in glyph.iter().enumerate() {

                    let mut dst =
                        fb.add((y as usize + row) * pitch + x as usize);

                    let mut bits = *byte;

                    for _ in 0..8 {
                        if bits & 0x80 != 0 {
                            dst.write_unaligned(color);
                        }
                        dst = dst.add(1);
                        bits <<= 1;
                    }
                }
            }

            /* =========================
               RGB888 PATH
               ========================= */

            crate::graphics::pixel::PixelFormat::Rgb888 => {

                let pitch = surface.pitch_bytes as usize;
                let raw = color.raw().to_le_bytes();

                for (row, byte) in glyph.iter().enumerate() {

                    let mut dst =
                        surface.base.add(
                            (y as usize + row) * pitch +
                            x as usize * 3
                        );

                    let mut bits = *byte;

                    for _ in 0..8 {

                        if bits & 0x80 != 0 {
                            dst.write(raw[0]);
                            dst.add(1).write(raw[1]);
                            dst.add(2).write(raw[2]);
                        }

                        dst = dst.add(3);
                        bits <<= 1;
                    }
                }
            }
        }
    }
}

pub fn draw_glyph_from_panic(
    x: u32,
    y: u32,
    font: &BitmapFont,
    glyph: &[u8],
    color: Color,
) {
    unsafe { framebuffer::clear_poison() };

    let mut surface = super::framebuffer();

    draw_glyph(&mut surface, x, y, font, glyph, color);
}

/// Draw ASCII text using a PSF1 font
pub fn draw_text(
    surface: &mut Surface,
    mut pos: Pos,
    font: &BitmapFont,
    text: &str,
    color: Color,
) {
    let start_x = pos.x;

    for ch in text.bytes() {
        match ch {
            b'\n' => {
                pos.x = start_x;
                pos.y += font.glyph_height as u32;
            }
            _ => {
                if let Some(glyph) = font.glyph(ch) {
                    draw_glyph(surface, pos.x, pos.y, font, glyph, color);
                }
                pos.x += font.glyph_width as u32;
            }
        }
    }
}

pub fn draw_text_from_panic(
    mut pos: Pos,
    font: &BitmapFont,
    text: &str,
    color: Color
) {
    let start_x = pos.x;

    for ch in text.bytes() {
        match ch {
            b'\n' => {
                pos.x = start_x;
                pos.y += font.glyph_height as u32;
            }
            _ => {
                if let Some(glyph) = font.glyph(ch) {
                    draw_glyph_from_panic(pos.x, pos.y, font, glyph, color);
                }
                pos.x += font.glyph_width as u32;
            }
        }
    }
}
