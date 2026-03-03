pub mod color;
pub mod geom;
pub mod surface;
pub mod framebuffer;
pub mod renderer;
pub mod font;
pub mod pixel;

pub use color::Color;
pub use geom::{Pos, Rect};
pub use surface::Surface;
pub use framebuffer::{init as init_from_boot, take as framebuffer};
pub use renderer::{init as init_renderer, with_renderer};