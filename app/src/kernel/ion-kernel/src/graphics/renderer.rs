use alloc::alloc::{Layout, alloc, alloc_zeroed};
use core::mem::MaybeUninit;

use crate::graphics::{Color, Rect, Surface};
use crate::thread::sync::Mutex;

pub struct Renderer {
    pub front: Surface,
    pub back: Surface,
}

static RENDERER: Mutex<MaybeUninit<Renderer>> = Mutex::new(MaybeUninit::uninit());

pub fn init(front: Surface) {
    let size = (front.pitch_bytes as usize) * (front.height as usize);

    let layout = Layout::from_size_align(size, 4096).unwrap();

    let ptr = unsafe { alloc_zeroed(layout) };

    assert!(!ptr.is_null(), "Back buffer allocation failed");

    let back = Surface {
        width: front.width,
        height: front.height,
        pitch_bytes: front.width * 4, // tightly packed
        base: ptr,
        format: front.format,
    };

    RENDERER.lock().write(Renderer { front, back });
}

pub fn with_renderer<F: FnOnce(&mut Renderer)>(f: F) {
    let mut lock = RENDERER.lock();
    let r = unsafe { lock.assume_init_mut() };
    f(r);
}

impl Renderer {
    pub fn clear(&mut self, color: Color) {
        self.back.clear(color);
    }

    pub fn present(&mut self) {
        self.front.blit(&self.back, 0, 0);
    }

    pub fn present_region(&mut self, region: Rect) {
        self.front.blit_region(&self.back, region);
    }
}
