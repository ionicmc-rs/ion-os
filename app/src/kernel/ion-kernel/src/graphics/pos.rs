//! Basic position and geometry types for graphics.
//!
//! These types are intentionally small, copyable, and boring.
//! They are used across framebuffer, drawing, UI, and input code.

/// A 2D position in pixels (x, y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub x: u32,
    pub y: u32,
}

impl Pos {
    #[inline]
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// A 2D size in pixels (width, height).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

impl Size {
    #[inline]
    pub const fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }
}

/// A rectangle defined by a position (top-left) and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub pos: Pos,
    pub size: Size,
}

impl Rect {
    #[inline]
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self {
            pos: Pos::new(x, y),
            size: Size::new(w, h),
        }
    }

    #[inline]
    pub const fn from_pos_size(pos: Pos, size: Size) -> Self {
        Self { pos, size }
    }

    #[inline]
    pub const fn x(&self) -> u32 {
        self.pos.x
    }

    #[inline]
    pub const fn y(&self) -> u32 {
        self.pos.y
    }

    #[inline]
    pub const fn width(&self) -> u32 {
        self.size.w
    }

    #[inline]
    pub const fn height(&self) -> u32 {
        self.size.h
    }

    #[inline]
    pub const fn right(&self) -> u32 {
        self.pos.x + self.size.w
    }

    #[inline]
    pub const fn bottom(&self) -> u32 {
        self.pos.y + self.size.h
    }

    /// Returns true if the point lies inside the rectangle.
    #[inline]
    pub const fn contains(&self, p: Pos) -> bool {
        p.x >= self.pos.x
            && p.y >= self.pos.y
            && p.x < self.right()
            && p.y < self.bottom()
    }

    /// Clamp this rectangle to a bounding size.
    #[inline]
    pub fn clamp_to(&self, bounds: Size) -> Rect {
        let x = self.pos.x.min(bounds.w);
        let y = self.pos.y.min(bounds.h);

        let max_w = bounds.w.saturating_sub(x);
        let max_h = bounds.h.saturating_sub(y);

        let w = self.size.w.min(max_w);
        let h = self.size.h.min(max_h);

        Rect::new(x, y, w, h)
    }
}
