//! Converting a `rendering` [`Framebuffer`] into a `platform`
//! presentation-ready [`Frame`] (I-1 slice 1).
//!
//! `rendering` and `platform` intentionally don't depend on each other
//! `(minimal-deps)` (see `rendering`'s crate docs), so this integration
//! crate owns the one place that bridges their pixel formats:
//! `rendering`'s per-channel [`Rgb888`] versus `platform`'s packed `0RGB`
//! `u32` (matching `softbuffer`'s pixel format, see `platform::present`).

use platform::{Frame, PIXEL_COUNT};
use rendering::{Framebuffer, Rgb888};

/// Convert a composed [`Framebuffer`] into a `platform`-ready [`Frame`],
/// packing each [`Rgb888`] pixel into `softbuffer`'s `0x00RRGGBB` `u32`
/// layout.
///
/// Built via `Vec` (heap) rather than a boxed array literal, matching
/// `platform::present::test_pattern`'s reasoning: a boxed array literal
/// would momentarily place all `PIXEL_COUNT` `u32`s on the stack before
/// boxing.
#[must_use]
pub fn to_platform_frame(framebuffer: &Framebuffer) -> Box<Frame> {
    let mut frame: Box<Frame> = vec![0u32; PIXEL_COUNT]
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| unreachable!("vec![_; PIXEL_COUNT] always has length PIXEL_COUNT"));
    // `Framebuffer` and `Frame` are both fixed at the GBA's native 240x160
    // resolution, so they always hold exactly `PIXEL_COUNT` pixels in the
    // same row-major order; `zip` alone (no explicit length check) is
    // therefore complete, not just defensive.
    for (dst, &src) in frame.iter_mut().zip(framebuffer.pixels()) {
        *dst = pack_rgb888(src);
    }
    frame
}

/// Pack one [`Rgb888`] pixel into `softbuffer`'s `0x00RRGGBB` layout (bits
/// 24-31 unused, matching `platform::present::test_pattern`'s reference
/// values).
fn pack_rgb888(color: Rgb888) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}

#[cfg(test)]
mod tests {
    use super::{pack_rgb888, to_platform_frame};
    use rendering::{Framebuffer, Rgb888};

    #[test]
    fn pack_matches_softbuffer_0rgb_layout() {
        assert_eq!(pack_rgb888(Rgb888 { r: 0, g: 0, b: 0 }), 0x0000_0000);
        assert_eq!(
            pack_rgb888(Rgb888 {
                r: 255,
                g: 255,
                b: 255
            }),
            0x00FF_FFFF
        );
        assert_eq!(
            pack_rgb888(Rgb888 {
                r: 0x12,
                g: 0x34,
                b: 0x56
            }),
            0x0012_3456
        );
    }

    #[test]
    fn to_platform_frame_converts_every_pixel_in_row_major_order() {
        let mut fb = Framebuffer::new();
        fb.set_pixel(
            0,
            0,
            Rgb888 {
                r: 10,
                g: 20,
                b: 30,
            },
        );
        fb.set_pixel(239, 159, Rgb888 { r: 1, g: 2, b: 3 });

        let frame = to_platform_frame(&fb);
        assert_eq!(
            frame[0],
            pack_rgb888(Rgb888 {
                r: 10,
                g: 20,
                b: 30
            })
        );
        assert_eq!(frame.len(), 240 * 160);
        assert_eq!(
            *frame.last().expect("PIXEL_COUNT frame is never empty"),
            pack_rgb888(Rgb888 { r: 1, g: 2, b: 3 })
        );
    }

    #[test]
    fn all_black_framebuffer_converts_to_an_all_zero_frame() {
        let fb = Framebuffer::new();
        let frame = to_platform_frame(&fb);
        assert!(frame.iter().all(|&p| p == 0));
    }
}
