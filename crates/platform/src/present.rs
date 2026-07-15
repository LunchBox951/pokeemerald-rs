//! Presentation (S-1): integer-scale + letterbox math, and the blit that
//! expands a 240x160 pixel buffer into a window-sized `softbuffer` surface.
//!
//! No real game content is produced by this crate — [`crate::window::Platform`]
//! accepts any 240x160 buffer each frame (a static test pattern is enough to
//! prove the pipeline end-to-end); real BG/sprite output is `rendering`'s job
//! (S-2).

/// The GBA's native framebuffer width, in pixels.
pub const GBA_WIDTH: u32 = 240;
/// The GBA's native framebuffer height, in pixels.
pub const GBA_HEIGHT: u32 = 160;
/// The number of pixels in one native GBA frame.
pub const PIXEL_COUNT: usize = (GBA_WIDTH * GBA_HEIGHT) as usize;

/// One native-resolution frame: 240x160 pixels, row-major, matching
/// `softbuffer`'s `0RGB` (`u32`) pixel format.
pub type Frame = [u32; PIXEL_COUNT];

/// The result of fitting the native 240x160 buffer into a window: the
/// largest integer scale that fits, and the centered destination rectangle
/// it occupies (letterboxed on whichever axis has slack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Letterbox {
    /// The integer upscale factor applied to the native buffer.
    pub scale: u32,
    /// X offset (in window pixels) of the scaled image's top-left corner.
    pub dest_x: u32,
    /// Y offset (in window pixels) of the scaled image's top-left corner.
    pub dest_y: u32,
    /// Width of the scaled image, in window pixels (`GBA_WIDTH * scale`).
    pub scaled_width: u32,
    /// Height of the scaled image, in window pixels (`GBA_HEIGHT * scale`).
    pub scaled_height: u32,
}

impl Letterbox {
    /// Compute the largest integer scale of the native 240x160 buffer that
    /// fits within `window_width` x `window_height`, centered.
    ///
    /// The scale is clamped to a minimum of `1`: if the window is smaller
    /// than the native buffer, the (uncropped) scaled image simply extends
    /// past the window on whichever axes are too small, and callers must
    /// clip when blitting (see [`blit`]).
    #[must_use]
    pub fn compute(window_width: u32, window_height: u32) -> Self {
        let scale_x = window_width / GBA_WIDTH;
        let scale_y = window_height / GBA_HEIGHT;
        let scale = scale_x.min(scale_y).max(1);
        let scaled_width = GBA_WIDTH * scale;
        let scaled_height = GBA_HEIGHT * scale;
        let dest_x = window_width.saturating_sub(scaled_width) / 2;
        let dest_y = window_height.saturating_sub(scaled_height) / 2;
        Self {
            scale,
            dest_x,
            dest_y,
            scaled_width,
            scaled_height,
        }
    }
}

/// Blit `src` (a native 240x160 frame) into `dest`, a row-major
/// `dest_width * dest_height` buffer, nearest-neighbour upscaled and
/// letterboxed per `letterbox`. Pixels outside the scaled image (the
/// letterbox bars, or anything clipped by a too-small window) are filled
/// with black.
///
/// # Panics
///
/// Panics if `dest.len() != dest_width * dest_height` (as `usize`).
pub fn blit(
    src: &Frame,
    letterbox: &Letterbox,
    dest_width: u32,
    dest_height: u32,
    dest: &mut [u32],
) {
    assert_eq!(
        dest.len(),
        dest_width as usize * dest_height as usize,
        "destination buffer size does not match dest_width * dest_height"
    );
    for y in 0..dest_height {
        for x in 0..dest_width {
            let idx = y as usize * dest_width as usize + x as usize;
            dest[idx] = sample(src, letterbox, x, y);
        }
    }
}

/// The color of one destination pixel: black outside the scaled image,
/// otherwise the nearest-neighbour source pixel.
fn sample(src: &Frame, letterbox: &Letterbox, x: u32, y: u32) -> u32 {
    const BLACK: u32 = 0;

    if x < letterbox.dest_x || y < letterbox.dest_y {
        return BLACK;
    }
    let rel_x = x - letterbox.dest_x;
    let rel_y = y - letterbox.dest_y;
    if rel_x >= letterbox.scaled_width || rel_y >= letterbox.scaled_height {
        return BLACK;
    }
    let src_x = rel_x / letterbox.scale;
    let src_y = rel_y / letterbox.scale;
    if src_x >= GBA_WIDTH || src_y >= GBA_HEIGHT {
        return BLACK; // Unreachable given `Letterbox::compute`'s invariants; defensive only.
    }
    src[src_y as usize * GBA_WIDTH as usize + src_x as usize]
}

/// A static test pattern (a 16x16 checkerboard) proving the presentation
/// pipeline end-to-end. No real game content is produced by this crate —
/// real BG/sprite rendering is `rendering`'s job (S-2).
#[must_use]
#[allow(clippy::missing_panics_doc)] // infallible: the vec is exactly PIXEL_COUNT long
pub fn test_pattern() -> Box<Frame> {
    // Built via `Vec` (heap) rather than a boxed array literal, which would
    // momentarily place all 38400 u32s on the stack before boxing.
    let mut frame: Box<Frame> = vec![0u32; PIXEL_COUNT]
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| unreachable!("vec![_; PIXEL_COUNT] always has length PIXEL_COUNT"));
    for y in 0..GBA_HEIGHT {
        for x in 0..GBA_WIDTH {
            let idx = y as usize * GBA_WIDTH as usize + x as usize;
            let checker = (x / 16 + y / 16) % 2 == 0;
            frame[idx] = if checker { 0x00FF_FFFF } else { 0x0000_0000 };
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_fit_uses_full_window_no_bars() {
        // 480x320 is exactly 2x the native 240x160.
        let lb = Letterbox::compute(480, 320);
        assert_eq!(lb.scale, 2);
        assert_eq!((lb.dest_x, lb.dest_y), (0, 0));
        assert_eq!((lb.scaled_width, lb.scaled_height), (480, 320));
    }

    #[test]
    fn non_integer_window_letterboxes_and_centers() {
        // 500x350: scale 2 fits both axes (480<=500, 320<=350), with slack
        // split evenly on each axis.
        let lb = Letterbox::compute(500, 350);
        assert_eq!(lb.scale, 2);
        assert_eq!((lb.scaled_width, lb.scaled_height), (480, 320));
        assert_eq!((lb.dest_x, lb.dest_y), (10, 15));
    }

    #[test]
    fn smaller_than_native_window_clamps_scale_to_one() {
        // 100x80 can't fit even a 1x native image; scale still clamps to a
        // minimum of 1, and the image is centered (i.e. offset clamped to 0
        // on the too-small axes) rather than vanishing.
        let lb = Letterbox::compute(100, 80);
        assert_eq!(lb.scale, 1);
        assert_eq!((lb.scaled_width, lb.scaled_height), (240, 160));
        assert_eq!((lb.dest_x, lb.dest_y), (0, 0));
    }

    #[test]
    fn huge_window_scales_up_and_letterboxes() {
        // 4000x3000: scale_x=16 (3840<=4000), scale_y=18 (2880<=3000) -> 16.
        let lb = Letterbox::compute(4000, 3000);
        assert_eq!(lb.scale, 16);
        assert_eq!((lb.scaled_width, lb.scaled_height), (3840, 2560));
        assert_eq!((lb.dest_x, lb.dest_y), (80, 220));
    }

    #[test]
    fn zero_sized_window_does_not_panic() {
        let lb = Letterbox::compute(0, 0);
        assert_eq!(lb.scale, 1);
        assert_eq!((lb.dest_x, lb.dest_y), (0, 0));
    }

    fn solid_frame(color: u32) -> Box<Frame> {
        vec![color; PIXEL_COUNT]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!("vec![_; PIXEL_COUNT] always has length PIXEL_COUNT"))
    }

    #[test]
    fn blit_exact_fit_fills_every_pixel_from_source() {
        let src = solid_frame(0x00FF_0000);
        let lb = Letterbox::compute(480, 320);
        let mut dest = vec![0u32; 480 * 320];
        blit(&src, &lb, 480, 320, &mut dest);
        assert!(dest.iter().all(|&p| p == 0x00FF_0000));
    }

    #[test]
    fn blit_letterboxes_with_black_bars() {
        let src = solid_frame(0x00FF_0000);
        let lb = Letterbox::compute(500, 350);
        let mut dest = vec![0xFFFF_FFFFu32; 500 * 350]; // pre-fill to prove bars get overwritten
        blit(&src, &lb, 500, 350, &mut dest);

        // Top-left corner is in the letterbox bar: black.
        assert_eq!(dest[0], 0);
        // Center of the image: the source color.
        let center_idx = 175 * 500 + 250;
        assert_eq!(dest[center_idx], 0x00FF_0000);
    }

    #[test]
    fn blit_into_smaller_than_native_window_clips_without_panicking() {
        let src = solid_frame(0x00AA_BB00);
        let lb = Letterbox::compute(100, 80);
        let mut dest = vec![0u32; 100 * 80];
        blit(&src, &lb, 100, 80, &mut dest);
        // Every visible pixel is still sourced from the (clipped) image.
        assert!(dest.iter().all(|&p| p == 0x00AA_BB00));
    }

    #[test]
    #[should_panic(expected = "destination buffer size does not match")]
    fn blit_panics_on_mismatched_buffer_len() {
        let src = solid_frame(0);
        let lb = Letterbox::compute(480, 320);
        let mut dest = vec![0u32; 10]; // deliberately wrong size
        blit(&src, &lb, 480, 320, &mut dest);
    }

    #[test]
    fn test_pattern_has_both_checker_colors() {
        let pattern = test_pattern();
        assert!(pattern.contains(&0x00FF_FFFF));
        assert!(pattern.contains(&0x0000_0000));
    }
}
