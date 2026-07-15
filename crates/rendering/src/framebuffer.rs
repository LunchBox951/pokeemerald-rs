//! An owned 240x160 RGB888 pixel buffer (S-2 slice 1).
//!
//! Pure `std`, headless-testable, with no dependency on `platform` or any
//! windowing crate `(minimal-deps)`. Sized and pixel-formatted to be handed
//! directly to `platform`'s `softbuffer` presentation surface once that
//! wiring lands — a future integration issue, out of scope here.

use crate::palette::Rgb888;

/// An owned 240x160 RGB888 pixel buffer — the GBA's visible display area.
#[derive(Debug, Clone)]
pub struct Framebuffer {
    pixels: Vec<Rgb888>,
}

impl Framebuffer {
    /// Visible display width in pixels — fixed GBA hardware resolution.
    pub const WIDTH: usize = 240;

    /// Visible display height in pixels — fixed GBA hardware resolution.
    pub const HEIGHT: usize = 160;

    /// Create a framebuffer filled with [`Rgb888::BLACK`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            pixels: vec![Rgb888::BLACK; Self::WIDTH * Self::HEIGHT],
        }
    }

    /// Framebuffer width in pixels (always [`Self::WIDTH`]).
    #[must_use]
    pub const fn width(&self) -> usize {
        Self::WIDTH
    }

    /// Framebuffer height in pixels (always [`Self::HEIGHT`]).
    #[must_use]
    pub const fn height(&self) -> usize {
        Self::HEIGHT
    }

    /// The pixel at `(x, y)`, or `None` if out of bounds.
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> Option<Rgb888> {
        if x >= Self::WIDTH || y >= Self::HEIGHT {
            return None;
        }
        self.pixels.get(y * Self::WIDTH + x).copied()
    }

    /// Set the pixel at `(x, y)`. Out-of-bounds coordinates are silently
    /// ignored.
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Rgb888) {
        if x >= Self::WIDTH || y >= Self::HEIGHT {
            return;
        }
        self.pixels[y * Self::WIDTH + x] = color;
    }

    /// Fill every pixel with `color`.
    pub fn fill(&mut self, color: Rgb888) {
        self.pixels.fill(color);
    }

    /// The pixel buffer in row-major order, ready for RGB888 presentation.
    #[must_use]
    pub fn pixels(&self) -> &[Rgb888] {
        &self.pixels
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Framebuffer;
    use crate::palette::Rgb888;

    #[test]
    fn dimensions_are_the_gba_resolution() {
        let fb = Framebuffer::new();
        assert_eq!(fb.width(), 240);
        assert_eq!(fb.height(), 160);
        assert_eq!(fb.pixels().len(), 240 * 160);
    }

    #[test]
    fn new_framebuffer_is_all_black() {
        let fb = Framebuffer::new();
        assert!(fb.pixels().iter().all(|&p| p == Rgb888::BLACK));
        assert_eq!(fb.pixel(0, 0), Some(Rgb888::BLACK));
        assert_eq!(fb.pixel(239, 159), Some(Rgb888::BLACK));
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(Framebuffer::default().pixels(), Framebuffer::new().pixels());
    }

    #[test]
    fn set_pixel_then_read_it_back() {
        let mut fb = Framebuffer::new();
        let red = Rgb888 { r: 255, g: 0, b: 0 };
        fb.set_pixel(10, 20, red);
        assert_eq!(fb.pixel(10, 20), Some(red));
        // Neighboring pixels untouched.
        assert_eq!(fb.pixel(9, 20), Some(Rgb888::BLACK));
        assert_eq!(fb.pixel(10, 19), Some(Rgb888::BLACK));
    }

    #[test]
    fn out_of_bounds_access_is_none_and_set_is_a_no_op() {
        let mut fb = Framebuffer::new();
        assert_eq!(fb.pixel(240, 0), None);
        assert_eq!(fb.pixel(0, 160), None);
        // Must not panic, and must not corrupt any in-bounds pixel.
        fb.set_pixel(240, 0, Rgb888 { r: 1, g: 2, b: 3 });
        fb.set_pixel(0, 160, Rgb888 { r: 1, g: 2, b: 3 });
        assert!(fb.pixels().iter().all(|&p| p == Rgb888::BLACK));
    }

    #[test]
    fn fill_sets_every_pixel() {
        let mut fb = Framebuffer::new();
        let gray = Rgb888 { r: 9, g: 9, b: 9 };
        fb.fill(gray);
        assert!(fb.pixels().iter().all(|&p| p == gray));
    }
}
