//! Pins basic (non-affine) OBJ mosaic snapping to its block origin.

use super::super::{compose_frame_with_effects, FrameEffects};
use super::shared::bpp4_tile_with_top_left_2x2;
use crate::mosaic::MosaicSize;
use crate::oam::{OamEntry, ObjShape};
use crate::palette::{Bgr555, Palette};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

#[test]
fn mosaic_snaps_obj_sampling_to_its_block_origin() {
    let bytes = bpp4_tile_with_top_left_2x2([[1, 2], [3, 4]]);
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0, 1, 0);
    colors[2] = Bgr555::from_channels(0, 2, 0);
    colors[3] = Bgr555::from_channels(0, 3, 0);
    colors[4] = Bgr555::from_channels(0, 4, 0);
    let palette = Palette::new(colors);
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
    .with_mosaic(true)];
    let sprites = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    let effects = FrameEffects {
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(2, 2),
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);

    let origin_color = Bgr555::from_channels(0, 1, 0).to_rgb888();
    assert_eq!(
        fb.pixel(1, 1),
        Some(origin_color),
        "snapped from (1,1) to (0,0)"
    );
}
