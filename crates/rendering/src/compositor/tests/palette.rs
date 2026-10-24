//! Pins the palette stage: per-bank transforms run before alpha resolution.

use super::super::{
    compose_frame_with_effects, BgSlot, FrameEffects, PaletteColorTransform, PaletteStage,
};
use super::shared::opaque_bg_fixture;
use crate::effects::{ColorEffect, EffectsConfig, LayerTargets};
use crate::oam::{OamEntry, ObjShape};
use crate::palette::{Bgr555, Palette, Rgb888};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

/// Channel-wise saturating add, so it is visibly non-idempotent: applying it
/// after a blend would give a different result than before.
#[derive(Debug)]
struct Add(Rgb888);

impl PaletteColorTransform for Add {
    fn transform(&self, color: Rgb888) -> Rgb888 {
        Rgb888 {
            r: color.r.saturating_add(self.0.r),
            g: color.g.saturating_add(self.0.g),
            b: color.b.saturating_add(self.0.b),
        }
    }
}

const RED: Rgb888 = Rgb888 { r: 255, g: 0, b: 0 };
const GREEN: Rgb888 = Rgb888 { r: 0, g: 255, b: 0 };

fn opaque_black_obj_entries() -> [OamEntry; 1] {
    [OamEntry::new(
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
    )]
}

/// Composes an opaque black OBJ over an opaque black BG0 with OBJ as alpha
/// target1, BG0 as target2, EVA = EVB = 8; returns the overlapping pixel.
fn overlap_pixel(palette: PaletteStage<'_>) -> Rgb888 {
    let (tiles, bg_palette, map) = opaque_bg_fixture(0);
    let bg_layer = crate::bg::BgLayer::new(&tiles, &bg_palette, &map);
    let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)];
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 0, 0);
    let sprite_palette = Palette::new(sprite_colors);
    let entries = opaque_black_obj_entries();
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);
    let effects = FrameEffects {
        color: EffectsConfig {
            effect: ColorEffect::AlphaBlend,
            target1: LayerTargets {
                obj: true,
                ..LayerTargets::default()
            },
            target2: LayerTargets {
                bg: [true, false, false, false],
                ..LayerTargets::default()
            },
            eva: 8,
            evb: 8,
            evy: 0,
        },
        palette,
        ..FrameEffects::default()
    };
    compose_frame_with_effects(&sprites, &slots, &effects)
        .pixel(0, 0)
        .unwrap()
}

#[test]
fn default_stage_is_identity() {
    assert_eq!(overlap_pixel(PaletteStage::default()), Rgb888::BLACK);
}

#[test]
fn stage_runs_before_alpha_and_is_selectable_per_bank() {
    let (red, green) = (Add(RED), Add(GREEN));
    // Blend-after-transform: (255*8 + 0*8) / 16 = 127. A transform applied
    // after the blend would instead yield 255.
    let bg_only = PaletteStage {
        bg: Some(&red),
        obj: None,
    };
    assert_eq!(overlap_pixel(bg_only), Rgb888 { r: 127, g: 0, b: 0 });
    let obj_only = PaletteStage {
        bg: None,
        obj: Some(&green),
    };
    assert_eq!(overlap_pixel(obj_only), Rgb888 { r: 0, g: 127, b: 0 });
    let both = PaletteStage {
        bg: Some(&red),
        obj: Some(&green),
    };
    assert_eq!(
        overlap_pixel(both),
        Rgb888 {
            r: 127,
            g: 127,
            b: 0
        }
    );
}

#[test]
fn backdrop_is_bg_palette_colour_zero() {
    let (red, green) = (Add(RED), Add(GREEN));
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let sprite_palette = Palette::new([Bgr555::default(); Palette::LEN]);
    let entries = opaque_black_obj_entries();
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);
    let stage = PaletteStage {
        bg: Some(&red),
        obj: Some(&green),
    };

    // Uncovered pixel: only the BG transform reaches the backdrop.
    let plain = FrameEffects {
        palette: stage,
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &plain);
    assert_eq!(fb.pixel(100, 100), Some(RED));

    // The OBJ (target1) alpha-blends against the transformed backdrop
    // (target2): OBJ black + green, backdrop black + red, halved.
    let blended = FrameEffects {
        color: EffectsConfig {
            effect: ColorEffect::AlphaBlend,
            target1: LayerTargets {
                obj: true,
                ..LayerTargets::default()
            },
            target2: LayerTargets {
                backdrop: true,
                ..LayerTargets::default()
            },
            eva: 8,
            evb: 8,
            evy: 0,
        },
        palette: stage,
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &blended);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Rgb888 {
            r: 127,
            g: 127,
            b: 0
        })
    );
}
