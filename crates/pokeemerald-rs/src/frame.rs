//! Conversion from rendered RGB pixels to the platform presentation format.

use platform::Frame;
use rendering::{Framebuffer, Rgb888};

/// Packs a composed framebuffer into [`Frame`]'s pixel format.
#[must_use]
pub fn to_platform_frame(framebuffer: &Framebuffer) -> Box<Frame> {
    framebuffer
        .pixels()
        .iter()
        .copied()
        .map(pack_rgb888)
        .collect::<Vec<_>>()
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| unreachable!("a framebuffer always has one platform frame of pixels"))
}

fn pack_rgb888(color: Rgb888) -> u32 {
    u32::from_be_bytes([0, color.r, color.g, color.b])
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
        let mut framebuffer = Framebuffer::new();
        framebuffer.set_pixel(
            0,
            0,
            Rgb888 {
                r: 10,
                g: 20,
                b: 30,
            },
        );
        framebuffer.set_pixel(
            Framebuffer::WIDTH - 1,
            Framebuffer::HEIGHT - 1,
            Rgb888 { r: 1, g: 2, b: 3 },
        );

        let frame = to_platform_frame(&framebuffer);
        assert_eq!(
            frame[0],
            pack_rgb888(Rgb888 {
                r: 10,
                g: 20,
                b: 30
            })
        );
        assert_eq!(frame.len(), Framebuffer::WIDTH * Framebuffer::HEIGHT);
        assert_eq!(
            *frame
                .last()
                .expect("a native-resolution frame is not empty"),
            pack_rgb888(Rgb888 { r: 1, g: 2, b: 3 })
        );
    }

    #[test]
    fn all_black_framebuffer_converts_to_an_all_zero_frame() {
        let framebuffer = Framebuffer::new();
        let frame = to_platform_frame(&framebuffer);
        assert!(frame.iter().all(|&pixel| pixel == 0));
    }
}
