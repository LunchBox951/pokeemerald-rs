//! Palette staging for overlays that paint opaque colours straight into a
//! [`Framebuffer`].
//!
//! The field is staged where upstream stages it, at palette-sample time
//! through [`rendering::FrameEffects::palette`]. The windows, text and
//! sprites the menus paint carry no bank of their own once drawn, so
//! [`stage_overlay`] recovers which pixels a painter wrote and maps exactly
//! those through the bank's transform
//! (`BeginNormalPaletteFade`, `pokeemerald/src/palette.c:156-189`;
//! `UpdatePaletteFade`, `palette.c:114-131`).

use rendering::{Framebuffer, PaletteColorTransform, Rgb888};

/// Runs `draw` on `base`, with every pixel it writes mapped through
/// `transform`. `None` is the identity: `draw` runs once, directly on `base`.
///
/// `draw` must be deterministic and write opaque colours: it is replayed on a
/// black and on a white frame, and a pixel it wrote is exactly one that ends
/// equal on both. Pixels it did not write keep `base`'s value untouched, so an
/// already-staged field underneath is never staged twice. This is not a
/// substitute for painters that read the pixels beneath them.
pub(crate) fn stage_overlay(
    base: &mut Framebuffer,
    transform: Option<&dyn PaletteColorTransform>,
    draw: impl Fn(&mut Framebuffer),
) {
    let Some(transform) = transform else {
        draw(base);
        return;
    };
    let mut on_black = Framebuffer::new();
    on_black.fill(Rgb888::BLACK);
    let mut on_white = Framebuffer::new();
    on_white.fill(WHITE);
    draw(&mut on_black);
    draw(&mut on_white);
    for y in 0..Framebuffer::HEIGHT {
        for x in 0..Framebuffer::WIDTH {
            if let (Some(black), Some(white)) = (on_black.pixel(x, y), on_white.pixel(x, y)) {
                if black == white {
                    base.set_pixel(x, y, transform.transform(black));
                }
            }
        }
    }
}

const WHITE: Rgb888 = Rgb888 {
    r: 255,
    g: 255,
    b: 255,
};

/// A transform that returns its input. Passing it as a bank's transform
/// forces the staging path while the pixels must stay as drawn.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct IdentityTransform;

#[cfg(test)]
impl PaletteColorTransform for IdentityTransform {
    fn transform(&self, color: Rgb888) -> Rgb888 {
        color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Halve;

    impl PaletteColorTransform for Halve {
        fn transform(&self, color: Rgb888) -> Rgb888 {
            Rgb888 {
                r: color.r / 2,
                g: color.g / 2,
                b: color.b / 2,
            }
        }
    }

    const MARKER: Rgb888 = Rgb888 {
        r: 80,
        g: 100,
        b: 120,
    };

    fn paint(fb: &mut Framebuffer) {
        fb.set_pixel(1, 1, Rgb888::BLACK);
        fb.set_pixel(2, 1, WHITE);
        fb.set_pixel(3, 1, MARKER);
    }

    #[test]
    fn identity_draws_directly_onto_the_base() {
        let mut base = Framebuffer::new();
        base.fill(MARKER);
        stage_overlay(&mut base, None, paint);
        assert_eq!(base.pixel(1, 1), Some(Rgb888::BLACK));
        assert_eq!(base.pixel(2, 1), Some(WHITE));
        assert_eq!(base.pixel(0, 0), Some(MARKER));
    }

    #[test]
    fn written_pixels_are_staged_once_even_when_black_white_or_equal_to_the_base() {
        let mut base = Framebuffer::new();
        base.fill(MARKER);
        stage_overlay(&mut base, Some(&Halve), paint);
        assert_eq!(base.pixel(1, 1), Some(Rgb888::BLACK));
        assert_eq!(
            base.pixel(2, 1),
            Some(Rgb888 {
                r: 127,
                g: 127,
                b: 127
            })
        );
        assert_eq!(
            base.pixel(3, 1),
            Some(Rgb888 {
                r: 40,
                g: 50,
                b: 60
            })
        );
        assert_eq!(base.pixel(0, 0), Some(MARKER), "unwritten pixels are kept");
    }
}
