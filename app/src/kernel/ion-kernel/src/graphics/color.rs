//! Color utilities for graphics.
//!
//! Colors are stored as 0x00RRGGBB (no alpha for now).

/// A packed RGB color (0x00RRGGBB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Color(pub u32);

impl Color {
    /// Create a color from RGB components.
    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    pub const fn hex(hex: u32) -> Self {
        Self(hex)
    }

    /// Create a grayscale color.
    #[inline]
    pub const fn gray(v: u8) -> Self {
        Self::rgb(v, v, v)
    }

    /// Get the raw packed value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

// Common colors
impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const RED: Color = Color::rgb(255, 0, 0);
    pub const GREEN: Color = Color::rgb(0, 255, 0);
    pub const BLUE: Color = Color::rgb(0, 0, 255);
}
