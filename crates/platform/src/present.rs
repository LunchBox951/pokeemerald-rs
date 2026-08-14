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
///
/// Only constructible via [`Letterbox::compute`], which upholds the
/// invariant (relied on by [`blit`]'s pixel sampling) that `scale` is never
/// zero.
///
/// # Examples
///
/// Fields are private, so external code cannot build a `Letterbox` with an
/// invalid zero `scale`:
///
/// ```compile_fail
/// let _ = platform::present::Letterbox {
///     scale: 0,
///     dest_x: 0,
///     dest_y: 0,
///     scaled_width: 0,
///     scaled_height: 0,
/// };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Letterbox {
    /// The integer upscale factor applied to the native buffer.
    scale: u32,
    /// X offset (in window pixels) of the scaled image's top-left corner.
    dest_x: u32,
    /// Y offset (in window pixels) of the scaled image's top-left corner.
    dest_y: u32,
    /// Width of the scaled image, in window pixels (`GBA_WIDTH * scale`).
    scaled_width: u32,
    /// Height of the scaled image, in window pixels (`GBA_HEIGHT * scale`).
    scaled_height: u32,
    /// Scaled-pixel offset cropped from the left when `scaled_width` exceeds
    /// the window (i.e. the window is smaller than the native buffer).
    crop_x: u32,
    /// Scaled-pixel offset cropped from the top when `scaled_height`
    /// exceeds the window (i.e. the window is smaller than the native
    /// buffer).
    crop_y: u32,
}

impl Letterbox {
    /// Compute the largest integer scale of the native 240x160 buffer that
    /// fits within `window_width` x `window_height`, centered.
    ///
    /// The scale is clamped to a minimum of `1`: if the window is smaller
    /// than the native buffer, the scaled image is cropped evenly on
    /// whichever axes are too small, and callers see only the centered
    /// portion that fits (see [`blit`]).
    #[must_use]
    pub fn compute(window_width: u32, window_height: u32) -> Self {
        let scale_x = window_width / GBA_WIDTH;
        let scale_y = window_height / GBA_HEIGHT;
        let scale = scale_x.min(scale_y).max(1);
        let scaled_width = GBA_WIDTH * scale;
        let scaled_height = GBA_HEIGHT * scale;
        let dest_x = window_width.saturating_sub(scaled_width) / 2;
        let dest_y = window_height.saturating_sub(scaled_height) / 2;
        let crop_x = scaled_width.saturating_sub(window_width) / 2;
        let crop_y = scaled_height.saturating_sub(window_height) / 2;
        Self {
            scale,
            dest_x,
            dest_y,
            scaled_width,
            scaled_height,
            crop_x,
            crop_y,
        }
    }

    /// The integer upscale factor applied to the native buffer.
    #[must_use]
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// X offset (in window pixels) of the scaled image's top-left corner.
    #[must_use]
    pub fn dest_x(&self) -> u32 {
        self.dest_x
    }

    /// Y offset (in window pixels) of the scaled image's top-left corner.
    #[must_use]
    pub fn dest_y(&self) -> u32 {
        self.dest_y
    }

    /// Width of the scaled image, in window pixels (`GBA_WIDTH * scale`).
    #[must_use]
    pub fn scaled_width(&self) -> u32 {
        self.scaled_width
    }

    /// Height of the scaled image, in window pixels (`GBA_HEIGHT * scale`).
    #[must_use]
    pub fn scaled_height(&self) -> u32 {
        self.scaled_height
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
    // Widened so `rel_{x,y} + crop_{x,y}` can't overflow before dividing.
    let src_x = (u64::from(rel_x) + u64::from(letterbox.crop_x)) / u64::from(letterbox.scale);
    let src_y = (u64::from(rel_y) + u64::from(letterbox.crop_y)) / u64::from(letterbox.scale);
    if src_x >= u64::from(GBA_WIDTH) || src_y >= u64::from(GBA_HEIGHT) {
        return BLACK; // Unreachable given `Letterbox::compute`'s invariants; defensive only.
    }
    let index = src_y * u64::from(GBA_WIDTH) + src_x;
    let index = usize::try_from(index).unwrap_or_else(|_| unreachable!("bounded by PIXEL_COUNT"));
    src[index]
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
        assert_eq!(lb.scale(), 2);
        assert_eq!((lb.dest_x(), lb.dest_y()), (0, 0));
        assert_eq!((lb.scaled_width(), lb.scaled_height()), (480, 320));
    }

    #[test]
    fn non_integer_window_letterboxes_and_centers() {
        // 500x350: scale 2 fits both axes (480<=500, 320<=350), with slack
        // split evenly on each axis.
        let lb = Letterbox::compute(500, 350);
        assert_eq!(lb.scale(), 2);
        assert_eq!((lb.scaled_width(), lb.scaled_height()), (480, 320));
        assert_eq!((lb.dest_x(), lb.dest_y()), (10, 15));
    }

    #[test]
    fn smaller_than_native_window_clamps_scale_to_one() {
        // 100x80 can't fit even a 1x native image; scale still clamps to a
        // minimum of 1, and the image is centered (i.e. offset clamped to 0
        // on the too-small axes) rather than vanishing.
        let lb = Letterbox::compute(100, 80);
        assert_eq!(lb.scale(), 1);
        assert_eq!((lb.scaled_width(), lb.scaled_height()), (240, 160));
        assert_eq!((lb.dest_x(), lb.dest_y()), (0, 0));
    }

    #[test]
    fn huge_window_scales_up_and_letterboxes() {
        // 4000x3000: scale_x=16 (3840<=4000), scale_y=18 (2880<=3000) -> 16.
        let lb = Letterbox::compute(4000, 3000);
        assert_eq!(lb.scale(), 16);
        assert_eq!((lb.scaled_width(), lb.scaled_height()), (3840, 2560));
        assert_eq!((lb.dest_x(), lb.dest_y()), (80, 220));
    }

    #[test]
    fn zero_sized_window_does_not_panic() {
        let lb = Letterbox::compute(0, 0);
        assert_eq!(lb.scale(), 1);
        assert_eq!((lb.dest_x(), lb.dest_y()), (0, 0));
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

    /// Encodes source coordinates `(x, y)` into a single pixel value.
    fn encode(x: u32, y: u32) -> u32 {
        x | (y << 16)
    }

    /// A frame whose pixel value is [`encode`]d from its own source
    /// coordinates, so a sampled pixel's source position can be read back
    /// out of the destination buffer.
    fn coord_frame() -> Box<Frame> {
        let mut frame: Box<Frame> = vec![0u32; PIXEL_COUNT]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!("vec![_; PIXEL_COUNT] always has length PIXEL_COUNT"));
        for y in 0..GBA_HEIGHT {
            for x in 0..GBA_WIDTH {
                let idx = y as usize * GBA_WIDTH as usize + x as usize;
                frame[idx] = encode(x, y);
            }
        }
        frame
    }

    #[test]
    fn blit_into_smaller_than_native_window_centers_cropped_region() {
        let src = coord_frame();
        let lb = Letterbox::compute(100, 80);
        let mut dest = vec![0u32; 100 * 80];
        blit(&src, &lb, 100, 80, &mut dest);

        // Top-left destination pixel: the centered crop starts at (70, 40).
        assert_eq!(dest[0], encode(70, 40));
        // Bottom-right destination pixel (99, 79): source (169, 119).
        let bottom_right_idx = 79 * 100 + 99;
        assert_eq!(dest[bottom_right_idx], encode(169, 119));
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
