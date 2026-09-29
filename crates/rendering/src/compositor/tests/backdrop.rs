//! Pins forced-alpha blending against a window span's resolved backdrop variant.

use super::super::{compose_frame_with_effects, FrameEffects};
use crate::effects::{ColorEffect, EffectsConfig, LayerTargets};
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette, Rgb888};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

#[test]
fn forced_alpha_blends_against_the_span_backdrop_variant() {
    // No BG or OBJ is a second target; the backdrop is the configured
    // target2, and effects::backdrop_variant resolves its colour to the
    // span's variant before the blend.
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 0, 0);
    let sprite_palette = Palette::new(sprite_colors);
    let entries = [OamEntry::new(
        0,
        0,
        0,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        0,
        true,
    )
    .with_mode(ObjMode::SemiTransparent)];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let white = Bgr555::from_channels(31, 31, 31).to_rgb888();
    // EVA = 0, EVB = 16 shows the second target alone, so the displayed
    // pixel is whichever backdrop variant mgba blended against.
    for (effect, backdrop, expected) in [
        (ColorEffect::Brighten, Rgb888::BLACK, white),
        (ColorEffect::Darken, white, Rgb888::BLACK),
    ] {
        let effects = FrameEffects {
            color: EffectsConfig {
                effect,
                target1: LayerTargets {
                    bg: [false; 4],
                    obj: false,
                    backdrop: true,
                },
                target2: LayerTargets {
                    bg: [false; 4],
                    obj: false,
                    backdrop: true,
                },
                eva: 0,
                evb: 16,
                evy: 16,
            },
            backdrop,
            ..FrameEffects::default()
        };

        let fb = compose_frame_with_effects(&sprites, &[], &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(expected),
            "forced alpha blends against the span's backdrop variant, not the raw backdrop"
        );
    }
}
