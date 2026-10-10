//! Pins affine-OBJ mosaic holds and trailing-spill behavior across window spans.

use super::super::{compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::{bpp4_row, opaque_bg_fixture};
use crate::affine::AffineMatrix;
use crate::effects::{ColorEffect, EffectsConfig, LayerTargets};
use crate::mosaic::MosaicSize;
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette, Rgb888};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};
use crate::window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};

#[test]
fn affine_obj_mosaic_hold_restarts_at_a_hardware_window_span_boundary() {
    // mGBA seeds each window span's affine OBJ mosaic hold from the
    // column one left of that span's start, not the screen-aligned block
    // origin (`mgba/src/gba/renderers/video-software.c:1052-1062`,
    // `software-obj.c:227-242,49-70`) `(behavioral-fidelity)`.
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&bpp4_row([1, 0, 0, 0, 2, 3, 0, 0]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0x1F, 0, 0);
    colors[2] = Bgr555::from_channels(0, 0x1F, 0);
    colors[3] = Bgr555::from_channels(0, 0, 0x1F);
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
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::IDENTITY];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let mut obj_on = WindowLayerEnable::NONE;
    obj_on.obj = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(6, 240), WindowRange::new(0, 1)),
                obj_on,
            )),
            win1: None,
            obj_window: None,
            winout: obj_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);

    let red = Bgr555::from_channels(0x1F, 0, 0).to_rgb888();
    let green = Bgr555::from_channels(0, 0x1F, 0).to_rgb888();
    let blue = Bgr555::from_channels(0, 0, 0x1F).to_rgb888();

    assert_eq!(fb.pixel(0, 0), Some(red), "x=0 samples source col 0");
    assert_eq!(
        fb.pixel(4, 0),
        Some(green),
        "x=4 starts the global block [4, 8) and samples source col 4"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(green),
        "x=5 still holds source col 4, before the window span boundary"
    );
    assert_eq!(
        fb.pixel(6, 0),
        Some(blue),
        "x=6 restarts the hold at the WIN0 span boundary, seeding from \
         source col 5 (inX - 1) -- not col 4, the global block origin"
    );
    assert_eq!(
        fb.pixel(7, 0),
        Some(blue),
        "x=7 still holds the span-restarted source col 5"
    );
}

#[test]
fn affine_obj_mosaic_trailing_spill_survives_a_later_window_span() {
    // mGBA can round an affine mosaic OBJ's trailing edge past its own
    // span, and the once-per-scanline sprite buffer keeps that spill
    // visible under a later span (`software-obj.c:227-242`)
    // `(behavioral-fidelity)`.
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&bpp4_row([1, 0, 0, 0, 0, 0, 3, 2]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0x1F, 0, 0);
    colors[2] = Bgr555::from_channels(0, 0x1F, 0);
    colors[3] = Bgr555::from_channels(0, 0, 0x1F);
    let palette = Palette::new(colors);

    let entries = [OamEntry::new(
        1,
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
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::IDENTITY];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let mut obj_on = WindowLayerEnable::NONE;
    obj_on.obj = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 240), WindowRange::new(0, 1)),
                obj_on,
            )),
            win1: None,
            obj_window: None,
            winout: obj_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);

    let green = Bgr555::from_channels(0, 0x1F, 0).to_rgb888();

    assert_eq!(
        fb.pixel(9, 0),
        Some(green),
        "x=9 is inside the WINOUT span and holds source col 7"
    );
    assert_eq!(
        fb.pixel(10, 0),
        Some(green),
        "x=10 was already written by the WINOUT pass's mosaic spill"
    );
    assert_eq!(
        fb.pixel(11, 0),
        Some(green),
        "x=11 was already written by the WINOUT pass's mosaic spill"
    );
}

fn compose_trailing_spill_under_effects(
    color: EffectsConfig,
    winout_effects: bool,
    win0_effects: bool,
) -> crate::framebuffer::Framebuffer {
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&bpp4_row([0, 0, 0, 0, 0, 0, 0, 2]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[2] = Bgr555::from_channels(0, 0x1F, 0);
    let palette = Palette::new(colors);

    let entries = [OamEntry::new(
        1,
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
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::IDENTITY];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let span = |effects| WindowLayerEnable {
        obj: true,
        effects,
        ..WindowLayerEnable::NONE
    };
    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 240), WindowRange::new(0, 1)),
                span(win0_effects),
            )),
            win1: None,
            obj_window: None,
            winout: span(winout_effects),
        },
        color,
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        ..FrameEffects::default()
    };
    compose_frame_with_effects(&sprites, &[], &effects)
}

const OBJ_FULL_BRIGHTEN: EffectsConfig = EffectsConfig {
    effect: ColorEffect::Brighten,
    target1: LayerTargets {
        bg: [false; 4],
        obj: true,
        backdrop: false,
    },
    target2: LayerTargets {
        bg: [false; 4],
        obj: false,
        backdrop: false,
    },
    eva: 0,
    evb: 0,
    evy: 16,
};

const OBJ_ALPHA_ONTO_BACKDROP: EffectsConfig = EffectsConfig {
    effect: ColorEffect::AlphaBlend,
    target1: LayerTargets {
        bg: [false; 4],
        obj: true,
        backdrop: false,
    },
    target2: LayerTargets {
        bg: [false; 4],
        obj: false,
        backdrop: true,
    },
    eva: 0,
    evb: 16,
    evy: 0,
};

#[test]
fn affine_obj_mosaic_trailing_spill_keeps_its_writer_spans_brighten() {
    // mGBA bakes an OBJ's color-effect variant into the sprite buffer
    // during the writing span; a later span only composites that stored
    // color (`software-obj.c:176-203,424-430`, `software-private.h:54-65`)
    // `(behavioral-fidelity)`.
    let fb = compose_trailing_spill_under_effects(OBJ_FULL_BRIGHTEN, true, false);
    let white = Bgr555::from_channels(0x1F, 0x1F, 0x1F).to_rgb888();

    assert_eq!(fb.pixel(9, 0), Some(white), "x=9 is WINOUT's own column");
    assert_eq!(
        fb.pixel(10, 0),
        Some(white),
        "x=10 is WINOUT's spill and keeps its brighten inside WIN0"
    );
    assert_eq!(
        fb.pixel(11, 0),
        Some(white),
        "x=11 is WINOUT's spill and keeps its brighten inside WIN0"
    );
}

#[test]
fn affine_obj_mosaic_trailing_spill_ignores_the_later_spans_brighten() {
    let fb = compose_trailing_spill_under_effects(OBJ_FULL_BRIGHTEN, false, true);
    let green = Bgr555::from_channels(0, 0x1F, 0).to_rgb888();

    assert_eq!(fb.pixel(9, 0), Some(green), "x=9 is WINOUT's own column");
    assert_eq!(
        fb.pixel(10, 0),
        Some(green),
        "x=10 is WINOUT's unbrightened spill, which WIN0 cannot brighten"
    );
    assert_eq!(
        fb.pixel(11, 0),
        Some(green),
        "x=11 is WINOUT's unbrightened spill, which WIN0 cannot brighten"
    );
}

#[test]
fn affine_obj_mosaic_trailing_spill_keeps_its_writer_spans_target1() {
    // mGBA sets an OBJ's `FLAG_TARGET_1` from the writing span, and the
    // alpha postpass blends every stored target-1 pixel without
    // rechecking any window (`software-obj.c:159,180-189`,
    // `video-software.c:963-981`) `(behavioral-fidelity)`.
    let fb = compose_trailing_spill_under_effects(OBJ_ALPHA_ONTO_BACKDROP, true, false);

    assert_eq!(
        fb.pixel(9, 0),
        Some(Rgb888::BLACK),
        "x=9 is WINOUT's own column"
    );
    assert_eq!(
        fb.pixel(10, 0),
        Some(Rgb888::BLACK),
        "x=10 is WINOUT's target-1 spill and still blends inside WIN0"
    );

    let fb = compose_trailing_spill_under_effects(OBJ_ALPHA_ONTO_BACKDROP, false, true);
    let green = Bgr555::from_channels(0, 0x1F, 0).to_rgb888();

    assert_eq!(fb.pixel(9, 0), Some(green), "x=9 is WINOUT's own column");
    assert_eq!(
        fb.pixel(10, 0),
        Some(green),
        "x=10 is WINOUT's non-target-1 spill, which WIN0 cannot blend"
    );
}

#[test]
fn affine_obj_mosaic_hold_restarts_when_the_window_span_starts_at_the_raw_edge() {
    // A span ending exactly at the sprite's raw right edge never rounds
    // (`condition == end`); the next span, starting at that same edge,
    // owns the rounding instead and seeds its own hold
    // (`software-obj.c:227-241`) `(behavioral-fidelity)`.
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&bpp4_row([1, 0, 0, 0, 0, 0, 3, 2]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0x1F, 0, 0);
    colors[2] = Bgr555::from_channels(0, 0x1F, 0);
    colors[3] = Bgr555::from_channels(0, 0, 0x1F);
    let palette = Palette::new(colors);

    let entries = [OamEntry::new(
        2,
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
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::IDENTITY];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let mut obj_on = WindowLayerEnable::NONE;
    obj_on.obj = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 240), WindowRange::new(0, 1)),
                obj_on,
            )),
            win1: None,
            obj_window: None,
            winout: obj_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);

    let blue = Bgr555::from_channels(0, 0, 0x1F).to_rgb888();
    let green = Bgr555::from_channels(0, 0x1F, 0).to_rgb888();

    assert_eq!(
        fb.pixel(8, 0),
        Some(blue),
        "x=8 is inside the WINOUT span and holds source col 6"
    );
    assert_eq!(
        fb.pixel(9, 0),
        Some(blue),
        "x=9 still holds the WINOUT pass's source col 6 -- its own pass \
         never rounds past the raw edge"
    );
    assert_eq!(
        fb.pixel(10, 0),
        Some(green),
        "x=10 restarts at the WIN0 span, which starts exactly at the raw \
         edge and seeds source col 7 (inX - 1)"
    );
    assert_eq!(
        fb.pixel(11, 0),
        Some(green),
        "x=11 still holds the WIN0 pass's restarted source col 7"
    );
}

#[test]
fn affine_obj_mosaic_trailing_spill_needs_its_owning_span_to_draw_obj() {
    // mGBA skips a span's sprite pass entirely when its control disables
    // OBJ (`video-software.c:1052-1062`); an identity matrix's later
    // OBJ-enabled span still fails its own bounds test, so the spill
    // stays unwritten (`software-obj.c:241,49-70`) `(behavioral-fidelity)`.
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&bpp4_row([1, 0, 0, 0, 0, 0, 3, 2]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0x1F, 0, 0);
    colors[2] = Bgr555::from_channels(0, 0x1F, 0);
    colors[3] = Bgr555::from_channels(0, 0, 0x1F);
    let palette = Palette::new(colors);

    let entries = [OamEntry::new(
        1,
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
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::IDENTITY];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let mut obj_on = WindowLayerEnable::NONE;
    obj_on.obj = true;
    let backdrop = Bgr555::from_channels(0x1F, 0x1F, 0).to_rgb888();

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 240), WindowRange::new(0, 1)),
                obj_on,
            )),
            win1: None,
            obj_window: None,
            winout: WindowLayerEnable::NONE,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        backdrop,
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);

    assert_eq!(
        fb.pixel(9, 0),
        Some(backdrop),
        "x=9 is inside the WINOUT span, which disables OBJ"
    );
    assert_eq!(
        fb.pixel(10, 0),
        Some(backdrop),
        "x=10 is WIN0's spill column, but the WINOUT pass that owns the \
         trailing rounding never ran"
    );
    assert_eq!(
        fb.pixel(11, 0),
        Some(backdrop),
        "x=11 is likewise unwritten by the skipped WINOUT pass"
    );
}

#[test]
fn affine_obj_mosaic_trailing_spill_renders_from_a_later_obj_enabled_span() {
    // mGBA reruns trailing-edge rounding independently in every span
    // whose own end doesn't bind it (`software-obj.c:227-242`)
    // `(behavioral-fidelity)`.
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&bpp4_row([1, 0, 0, 0, 0, 0, 3, 2]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0x1F, 0, 0);
    colors[2] = Bgr555::from_channels(0, 0x1F, 0);
    colors[3] = Bgr555::from_channels(0, 0, 0x1F);
    let palette = Palette::new(colors);

    let entries = [OamEntry::new(
        1,
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
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::new(
        -AffineMatrix::ONE,
        0,
        0,
        AffineMatrix::ONE,
    )];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let mut obj_on = WindowLayerEnable::NONE;
    obj_on.obj = true;
    let backdrop = Bgr555::from_channels(0x1F, 0x1F, 0).to_rgb888();
    let red = Bgr555::from_channels(0x1F, 0, 0).to_rgb888();

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 240), WindowRange::new(0, 1)),
                obj_on,
            )),
            win1: None,
            obj_window: None,
            winout: WindowLayerEnable::NONE,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        backdrop,
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);

    assert_eq!(
        fb.pixel(9, 0),
        Some(backdrop),
        "x=9 is inside the WINOUT span, which disables OBJ"
    );
    assert_eq!(
        fb.pixel(10, 0),
        Some(red),
        "x=10 is the WIN0 pass's own first column: it rounds the trailing \
         edge to 12 itself and seeds source col 0"
    );
    assert_eq!(
        fb.pixel(11, 0),
        Some(red),
        "x=11 still holds the WIN0 pass's source col 0"
    );
}

#[test]
fn affine_obj_mosaic_trailing_spill_prefers_an_opaque_span() {
    // mGBA's transparent-pixel write leaves the sprite buffer slot
    // available to a later span's opaque fetch
    // (`software-obj.c:49-70,227-242`) `(behavioral-fidelity)`.
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&bpp4_row([1, 0, 0, 0, 0, 0, 3, 2]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0x1F, 0, 0);
    colors[2] = Bgr555::from_channels(0, 0x1F, 0);
    colors[3] = Bgr555::from_channels(0, 0, 0x1F);
    let palette = Palette::new(colors);

    let entries = [OamEntry::new(
        1,
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
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::new(
        -AffineMatrix::ONE,
        0,
        0,
        AffineMatrix::ONE,
    )];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let mut obj_on = WindowLayerEnable::NONE;
    obj_on.obj = true;
    let backdrop = Bgr555::from_channels(0x1F, 0x1F, 0).to_rgb888();
    let red = Bgr555::from_channels(0x1F, 0, 0).to_rgb888();

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 240), WindowRange::new(0, 1)),
                obj_on,
            )),
            win1: None,
            obj_window: None,
            winout: obj_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        backdrop,
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);

    assert_eq!(
        fb.pixel(10, 0),
        Some(red),
        "the WINOUT pass's transparent hold leaves the slot unwritten, so \
         the WIN0 pass's opaque source col 0 still lands"
    );
}

#[test]
fn affine_obj_mosaic_trailing_spill_keeps_a_worse_sprites_color_underneath() {
    // Transparent promotion blocks this entry's own later opaque span
    // without drawing color (`software-obj.c:79-85`).
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 64];
    bytes[..32].fill(0x44);
    bytes[32..36].copy_from_slice(&bpp4_row([1, 0, 0, 0, 0, 0, 3, 2]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0x1F, 0, 0);
    colors[4] = Bgr555::from_channels(0, 0x1F, 0);
    let palette = Palette::new(colors);

    let b_opaque_prio1 = OamEntry::new(
        8,
        0,
        0,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        1,
        true,
    );
    let affine_prio0 = OamEntry::new(
        1,
        0,
        1,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        0,
        true,
    )
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 });
    let entries = [b_opaque_prio1, affine_prio0];
    let matrices = [AffineMatrix::new(
        -AffineMatrix::ONE,
        0,
        0,
        AffineMatrix::ONE,
    )];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let mut obj_on = WindowLayerEnable::NONE;
    obj_on.obj = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 240), WindowRange::new(0, 1)),
                obj_on,
            )),
            win1: None,
            obj_window: None,
            winout: obj_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);

    let green = Bgr555::from_channels(0, 0x1F, 0).to_rgb888();
    assert_eq!(
        fb.pixel(10, 0),
        Some(green),
        "B's opaque green already claims this slot, blocking the affine \
         entry's own later WIN0 span from overwriting it with red"
    );
}

#[test]
fn objwin_affine_mosaic_spill_written_in_winout_still_promotes_inside_win0() {
    // mGBA suppresses OBJWIN per writing pass, not per column reading its
    // spill (`software-obj.c:161`, `video-software.c:131-134`)
    // `(behavioral-fidelity)`.
    use crate::oam::AffineMode;

    const HOLE: u8 = 0;
    let (bg_tiles, bg_palette, bg_map) = opaque_bg_fixture(9);
    let bg = crate::bg::BgLayer::new(&bg_tiles, &bg_palette, &bg_map);
    let slots = [BgSlot::new(bg, 0, 1, 0, 0, true)];

    let mut tile_bytes = [0u8; 64];
    tile_bytes[..32].fill(0xFF);
    tile_bytes[32..36].copy_from_slice(&bpp4_row([5, 5, 5, 5, 5, HOLE, 5, 5]));
    let sprite_tiles = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
    sprite_colors[5] = Bgr555::from_channels(0, 31, 31);
    let sprite_palette = Palette::new(sprite_colors);

    let entries = [
        OamEntry::new(
            8,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            3,
            true,
        ),
        OamEntry::new(
            1,
            0,
            1,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )
        .with_mode(ObjMode::Window)
        .with_mosaic(true)
        .with_affine(AffineMode::Affine { matrix_num: 0 }),
    ];
    let matrices = [AffineMatrix::new(
        AffineMatrix::ONE / 4,
        0,
        0,
        AffineMatrix::ONE,
    )];
    let sprites = SpriteLayer::new(&entries, &sprite_tiles, &sprite_tiles, &sprite_palette)
        .with_affine_matrices(&matrices);

    let mut bg0_and_obj = WindowLayerEnable::NONE;
    bg0_and_obj.bg[0] = true;
    bg0_and_obj.obj = true;
    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 240), WindowRange::new(0, 1)),
                bg0_and_obj,
            )),
            win1: None,
            obj_window: Some(bg0_and_obj),
            winout: bg0_and_obj,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(4, 1),
        },
        ..FrameEffects::default()
    };

    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    let green = Bgr555::from_channels(0, 9, 0).to_rgb888();
    let red = Bgr555::from_channels(9, 0, 0).to_rgb888();

    assert_eq!(
        fb.pixel(9, 0),
        Some(green),
        "x=9 reads the WINOUT pass's spill in its own span: the OBJWIN \
         hole promotes the priority-3 OBJ ahead of BG0"
    );
    assert_eq!(
        fb.pixel(10, 0),
        Some(green),
        "x=10 was written by that same WINOUT pass, so WIN0's rank over \
         OBJWIN cannot retract the promotion the spill already made"
    );
    assert_eq!(
        fb.pixel(11, 0),
        Some(green),
        "x=11 is the last spilled column of block [8, 12)"
    );
    assert_eq!(
        fb.pixel(12, 0),
        Some(red),
        "x=12 is past the rounded trailing edge: no OBJWIN hole, so the \
         priority-3 OBJ stays behind BG0"
    );
}

/// Renders row 0 of a red 8x8 affine mosaic OBJ at x=0 (mosaic width 16, zero
/// matrix) under the given `WIN0`/`WIN1` controls, `(range, obj, effects)`
/// each, with `WINOUT` showing nothing and a full OBJ brighten selected.
fn zero_matrix_mosaic_obj_under_windows(
    win0: (WindowRange, bool, bool),
    win1: (WindowRange, bool, bool),
) -> crate::Framebuffer {
    use crate::oam::AffineMode;

    let mut bytes = [0u8; 32];
    for row in 0..8 {
        bytes[row * 4..row * 4 + 4].copy_from_slice(&bpp4_row([1; 8]));
    }
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(0x1F, 0, 0);
    let palette = Palette::new(colors);

    let entries = [OamEntry::new(
        1,
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
    .with_mosaic(true)
    .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::new(0, 0, 0, 0)];
    let sprites =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);

    let window = |(x, obj, effects): (WindowRange, bool, bool)| {
        (
            WindowRect::new(x, WindowRange::new(0, 1)),
            WindowLayerEnable {
                obj,
                effects,
                ..WindowLayerEnable::NONE
            },
        )
    };
    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some(window(win0)),
            win1: Some(window(win1)),
            obj_window: None,
            winout: WindowLayerEnable::NONE,
        },
        color: OBJ_FULL_BRIGHTEN,
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::NONE,
            obj: MosaicSize::new(16, 1),
        },
        ..FrameEffects::default()
    };
    compose_frame_with_effects(&sprites, &[], &effects)
}

/// Asserts the spill block `[10, 16)` on row 0 is `expected` and nothing else
/// of the sprite shows beyond it.
fn assert_spill_block_is(fb: &crate::Framebuffer, expected: Rgb888) {
    let black = Bgr555::default().to_rgb888();
    for x in 10..16 {
        assert_eq!(fb.pixel(x, 0), Some(expected), "x={x}");
    }
    assert_eq!(fb.pixel(16, 0), Some(black));
    assert_eq!(fb.pixel(10, 1), Some(black));
}

#[test]
fn affine_obj_mosaic_spill_from_a_zero_width_win0_pass_keeps_its_brighten() {
    // mGBA inserts WIN0 [10,10) as a real pass ahead of WIN1 [10,240)
    // (`video-software.c:476-495,908-911`); that pass's sprite preprocess
    // rounds the condition 8 up to 16 and writes columns 10..15 with WIN0's
    // effects-enabled variant palette (`software-obj.c:227-242`). WIN1's
    // later pass cannot overwrite equal-priority pixels (`software-obj.c:80`).
    let fb = zero_matrix_mosaic_obj_under_windows(
        (WindowRange::new(10, 10), true, true),
        (WindowRange::new(10, 240), true, false),
    );
    assert_spill_block_is(&fb, Bgr555::from_channels(0x1F, 0x1F, 0x1F).to_rgb888());
}

#[test]
fn affine_obj_mosaic_spill_from_a_zero_width_win1_pass_keeps_its_brighten() {
    // WIN1 [10,10) survives a WIN0 that merely begins at its column: the
    // insertion search skips passes ending at the new window's start
    // (`video-software.c:463,478-499`).
    let fb = zero_matrix_mosaic_obj_under_windows(
        (WindowRange::new(10, 240), true, false),
        (WindowRange::new(10, 10), true, true),
    );
    assert_spill_block_is(&fb, Bgr555::from_channels(0x1F, 0x1F, 0x1F).to_rgb888());
}

#[test]
fn affine_obj_mosaic_spill_skips_a_zero_width_pass_whose_obj_is_disabled() {
    // A pass with OBJ disabled and OBJWIN off runs no sprite preprocess
    // (`video-software.c:1056`), so only WIN1's effects-off pass writes.
    let fb = zero_matrix_mosaic_obj_under_windows(
        (WindowRange::new(10, 10), false, true),
        (WindowRange::new(10, 240), true, false),
    );
    assert_spill_block_is(&fb, Bgr555::from_channels(0x1F, 0, 0).to_rgb888());
}

#[test]
fn affine_obj_mosaic_spill_ignores_a_zero_width_win1_pass_overwritten_by_win0() {
    // WIN0 [9,240) overwrites the interior of the empty WIN1 [10,10) pass and
    // trims it away (`video-software.c:478-499`).
    let fb = zero_matrix_mosaic_obj_under_windows(
        (WindowRange::new(9, 240), true, false),
        (WindowRange::new(10, 10), true, true),
    );
    let red = Bgr555::from_channels(0x1F, 0, 0).to_rgb888();
    for x in 10..16 {
        assert_eq!(fb.pixel(x, 0), Some(red), "x={x}");
    }
}

#[test]
fn affine_obj_mosaic_spill_ignores_a_zero_width_win1_pass_trimmed_by_a_win0_ending_there() {
    // WIN0 [0,10) ends at the empty WIN1 [10,10) pass; the overwrite loop
    // trims passes ending at or before WIN0's end (`video-software.c:478-486`).
    let fb = zero_matrix_mosaic_obj_under_windows(
        (WindowRange::new(0, 10), false, false),
        (WindowRange::new(10, 10), true, true),
    );
    let black = Bgr555::default().to_rgb888();
    for x in 10..16 {
        assert_eq!(fb.pixel(x, 0), Some(black), "x={x}");
    }
}
