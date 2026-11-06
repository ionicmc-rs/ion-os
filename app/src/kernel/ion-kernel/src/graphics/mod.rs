pub mod color;
pub mod font;
pub mod framebuffer;
pub mod geom;
pub mod pixel;
pub mod renderer;
pub mod surface;

pub use color::Color;
pub use framebuffer::{init as init_from_boot, take as framebuffer};
pub use geom::{Pos, Rect};
pub use renderer::{init as init_renderer, with_renderer};
pub use surface::Surface;
