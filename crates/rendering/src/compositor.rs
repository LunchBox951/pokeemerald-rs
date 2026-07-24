//! The cross-layer priority compositor: up to four regular BG layers plus
//! the sprite layer, combined into one frame (S-2 slice 2).
//!
//! Ports the BG/OBJ ordering rules verified against
//! `mgba/src/gba/renderers/video-software.c` and `software-obj.c`:
//!
//! - A **lower [`priority`](BgSlot::new) number composites in front**,
//!   regardless of layer kind (sprite or BG).
//! - At equal priority, a **sprite always beats a BG** — in `video-software.c`
//!   a BG's per-pixel order key is `(priority << OFFSET_PRIORITY) |
//!   (index << OFFSET_INDEX) | FLAG_IS_BACKGROUND`, strictly greater (so
//!   strictly *behind*, in the "smaller key wins" comparison the renderer
//!   uses) than a sprite's bare `priority << OFFSET_PRIORITY` key at the same
//!   priority.
//! - At equal priority among BGs, the **lower BG index wins** — the index
//!   term above breaks the tie in ascending order (BG0 in front of BG1, and
//!   so on).
//! - At equal priority among sprites, the **lower OAM index wins** — see
//!   [`SpriteLayer::resolve_pixel`](crate::sprite::SpriteLayer::resolve_pixel).
//!
//! Affine BG layers slot in as of S-2 slice 3 (issue #98) via
//! [`BgSlot::new_affine`] — [`compose_frame`]'s own signature is unchanged
//! `(behavioral-fidelity)`.
//!
//! S-2 slice 4 (issue #99) adds hardware windows (`WIN0`/`WIN1`/`OBJWIN`),
//! color special effects (alpha blend, brighten, darken), and mosaic via
//! [`compose_frame_with_effects`] and the [`FrameEffects`] parameter
//! struct — [`compose_frame`] becomes a thin delegation to
//! [`compose_frame_with_effects`] with [`FrameEffects::default`], which
//! disables every slice-4 feature and so reproduces this slice's output
//! byte-for-byte, keeping [`compose_frame`]'s own signature (and every
//! existing caller) unchanged `(behavioral-fidelity)`.

use crate::affine::AffineMatrix;
use crate::bg::BgLayer;
use crate::bg_affine::{AffineBgLayer, Overflow};
use crate::effects::{self, EffectsConfig, LayerKind};
use crate::framebuffer::Framebuffer;
use crate::mosaic::{MosaicConfig, MosaicSize};
use crate::palette::Rgb888;
use crate::sprite::SpriteLayer;
use crate::window::WindowConfig;

/// A BG slot's per-pixel sampling mode: a regular BG (wrapping scroll
/// offsets, [`BgLayer`]) or an affine BG (matrix + reference point +
/// overflow, [`AffineBgLayer`]) — see [`BgSlot::new`]/[`BgSlot::new_affine`].
#[derive(Debug, Clone, Copy)]
enum BgKind<'a> {
    Regular {
        layer: BgLayer<'a>,
        scroll_x: u16,
        scroll_y: u16,
    },
    Affine {
        layer: AffineBgLayer<'a>,
        matrix: AffineMatrix,
        ref_x: i32,
        ref_y: i32,
        overflow: Overflow,
    },
}

/// One of up to four BG layers (regular or affine) participating in
/// priority composition, paired with the register-level state the GBA PPU
/// consults alongside the tile/palette data itself: which of BG0..BG3 this
/// is (breaks same-priority ties), the layer's priority, its per-pixel
/// sampling mode, and whether it's enabled at all.
#[derive(Debug, Clone, Copy)]
pub struct BgSlot<'a> {
    kind: BgKind<'a>,
    bg_index: u8,
    priority: u8,
    enabled: bool,
    mosaic: bool,
}

impl<'a> BgSlot<'a> {
    /// Build a regular BG slot. `bg_index` (`0..=3`, identifying BG0..BG3)
    /// and `priority` (`0..=3`) are masked to 2 bits, so this never panics.
    #[must_use]
    pub const fn new(
        layer: BgLayer<'a>,
        bg_index: u8,
        priority: u8,
        scroll_x: u16,
        scroll_y: u16,
        enabled: bool,
    ) -> Self {
        Self {
            kind: BgKind::Regular {
                layer,
                scroll_x,
                scroll_y,
            },
            bg_index: bg_index & 0x03,
            priority: priority & 0x03,
            enabled,
            mosaic: false,
        }
    }

    /// Build an affine BG slot (S-2 slice 3, issue #98). `bg_index` and
    /// `priority` are masked exactly as in [`new`](Self::new). `ref_x`/
    /// `ref_y` are the frame's latched reference point in 20.8 fixed point
    /// (see [`crate::bg_affine`]'s module docs for why one static
    /// reference point per frame is the behaviorally-correct model of how
    /// pokeemerald drives `BG2X`/`BG2Y`).
    #[must_use]
    #[allow(clippy::too_many_arguments)] // Mirrors the affine BG's full per-frame register set.
    pub const fn new_affine(
        layer: AffineBgLayer<'a>,
        bg_index: u8,
        priority: u8,
        matrix: AffineMatrix,
        ref_x: i32,
        ref_y: i32,
        overflow: Overflow,
        enabled: bool,
    ) -> Self {
        Self {
            kind: BgKind::Affine {
                layer,
                matrix,
                ref_x,
                ref_y,
                overflow,
            },
            bg_index: bg_index & 0x03,
            priority: priority & 0x03,
            enabled,
            mosaic: false,
        }
    }

    /// Return a copy of this slot with its mosaic bit (`BGxCNT`'s mosaic
    /// bit) replaced — S-2 slice 4, issue #99. A builder rather than a
    /// `new`/`new_affine` parameter so every pre-slice-4 call site keeps
    /// working unchanged; defaults to `false`.
    #[must_use]
    pub const fn with_mosaic(mut self, mosaic: bool) -> Self {
        self.mosaic = mosaic;
        self
    }

    /// Sample this slot's resolved color at `(x, y)`, dispatching to the
    /// regular or affine layer per [`BgKind`]. When this slot's mosaic bit is
    /// set, `(x, y)` is first snapped to `bg_mosaic`'s block origin
    /// (`crate::mosaic`); [`MosaicSize::NONE`] makes this a no-op regardless
    /// of the slot's own mosaic bit, which is what keeps
    /// [`compose_frame`]'s output byte-for-byte unaffected by this slice.
    fn sample(&self, x: usize, y: usize, bg_mosaic: MosaicSize) -> Option<Rgb888> {
        let (x, y) = if self.mosaic {
            bg_mosaic.snap(x, y)
        } else {
            (x, y)
        };
        match self.kind {
            BgKind::Regular {
                layer,
                scroll_x,
                scroll_y,
            } => layer.sample_scrolled(x, y, scroll_x, scroll_y),
            BgKind::Affine {
                layer,
                matrix,
                ref_x,
                ref_y,
                overflow,
            } => layer.sample_pixel(matrix, ref_x, ref_y, overflow, x, y),
        }
    }
}

/// A candidate pixel's ordering key: `(priority, layer_rank)`. Lower sorts
/// in front. A sprite's `layer_rank` is always `0`, strictly less than any
/// BG's `1 + bg_index` — so a sprite wins any same-priority tie against a
/// BG, and BGs break same-priority ties by ascending `bg_index`, matching
/// the ordering rules in the module docs.
type OrderKey = (u8, u8);

/// Composite up to four [`BgSlot`]s and one [`SpriteLayer`] into a new
/// [`Framebuffer`], applying the GBA's priority ordering rules (module
/// docs).
///
/// `bg_slots` need not have exactly four entries (a scene may use fewer BG
/// layers); disabled slots contribute nothing. Pixels covered by no enabled,
/// opaque layer are left at the framebuffer's default backdrop
/// ([`Rgb888::BLACK`](crate::palette::Rgb888::BLACK)).
///
/// A thin delegation to [`compose_frame_with_effects`] with
/// [`FrameEffects::default`] (S-2 slice 4, issue #99) — every window/color
/// effect/mosaic feature is disabled by that default, so this function's
/// output, and its signature, are unaffected by that slice
/// `(behavioral-fidelity)`.
#[must_use]
pub fn compose_frame(sprites: &SpriteLayer<'_>, bg_slots: &[BgSlot<'_>]) -> Framebuffer {
    compose_frame_with_effects(sprites, bg_slots, &FrameEffects::default())
}

/// Bundled optional per-frame effects for [`compose_frame_with_effects`]:
/// hardware windows, color special effects, and mosaic (S-2 slice 4, issue
/// #99).
///
/// [`Default`] disables every one of them (no active window, no color
/// effect, no mosaic, black backdrop) — this is what makes
/// [`compose_frame`] byte-for-byte equivalent to calling
/// [`compose_frame_with_effects`] with a default `FrameEffects`
/// `(behavioral-fidelity)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameEffects {
    /// Hardware window configuration (`WIN0`/`WIN1`/`OBJWIN`/`WINOUT`).
    pub windows: WindowConfig,
    /// Color special-effect configuration (`BLDCNT`/`BLDALPHA`/`BLDY`).
    pub color: EffectsConfig,
    /// BG/OBJ mosaic block sizes (`MOSAIC`).
    pub mosaic: MosaicConfig,
    /// The backdrop color shown where no enabled, opaque layer covers a
    /// pixel — defaults to [`Rgb888::BLACK`](crate::palette::Rgb888::BLACK),
    /// matching [`Framebuffer::new`]'s default fill.
    pub backdrop: Rgb888,
}

/// [`compose_frame`], extended with hardware windows, color special
/// effects, and mosaic (S-2 slice 4, issue #99).
///
/// Per pixel: gather every enabled BG slot's and (if not window-masked) the
/// sprite layer's opaque contribution, sort by the module docs' priority
/// ordering, then resolve the front (topmost) one through
/// [`effects::resolve_pixel_color`] against the layer immediately behind it
/// (or the backdrop, if nothing else was drawn) — see that function's docs
/// for exactly which second target a pixel is allowed to blend against.
#[must_use]
pub fn compose_frame_with_effects(
    sprites: &SpriteLayer<'_>,
    bg_slots: &[BgSlot<'_>],
    effects: &FrameEffects,
) -> Framebuffer {
    let mut framebuffer = Framebuffer::new();
    for y in 0..framebuffer.height() {
        for x in 0..framebuffer.width() {
            let color = compose_pixel(sprites, bg_slots, effects, x, y);
            framebuffer.set_pixel(x, y, color);
        }
    }
    framebuffer
}

/// Resolve one pixel's final color for [`compose_frame_with_effects`].
#[allow(clippy::cast_possible_truncation)] // Framebuffer coordinates are always < 240/160, well within u8.
fn compose_pixel(
    sprites: &SpriteLayer<'_>,
    bg_slots: &[BgSlot<'_>],
    effects: &FrameEffects,
    x: usize,
    y: usize,
) -> Rgb888 {
    let (wx, wy) = (x as u8, y as u8);

    let objwin_mask = sprites.objwin_mask_with_mosaic(x, y, effects.mosaic.obj);
    let window = effects.windows.classify(wx, wy, objwin_mask);

    let mut candidates: Vec<(OrderKey, Rgb888, LayerKind, bool)> =
        Vec::with_capacity(bg_slots.len() + 1);

    if window.obj {
        if let Some(pixel) = sprites.resolve_pixel_with_mosaic(x, y, effects.mosaic.obj) {
            candidates.push((
                (pixel.priority, 0),
                pixel.color,
                LayerKind::Obj,
                pixel.semi_transparent,
            ));
        }
    }
    for slot in bg_slots {
        if !slot.enabled || !window.bg_enabled(slot.bg_index) {
            continue;
        }
        let Some(color) = slot.sample(x, y, effects.mosaic.bg) else {
            continue;
        };
        candidates.push((
            (slot.priority, 1 + slot.bg_index),
            color,
            LayerKind::Bg(slot.bg_index),
            false,
        ));
    }
    // Stable sort: equal keys (only possible with a caller-misconfigured
    // duplicate bg_index) keep the earlier-pushed (sprite, then earlier
    // bg_slots) entry first, matching compose_frame's pre-slice-4 `<=`
    // tie-break.
    candidates.sort_by_key(|&(key, ..)| key);

    let mut candidates = candidates.into_iter();
    let Some((_, front_color, front_kind, front_semi_transparent)) = candidates.next() else {
        // Nothing drawn: the backdrop itself is shown, subject only to
        // brighten/darken (effects::resolve_pixel_color never alpha-blends
        // the backdrop against itself).
        return effects::resolve_pixel_color(
            &effects.color,
            window.effects,
            (effects.backdrop, LayerKind::Backdrop, false),
            None,
            effects.backdrop,
        );
    };
    let next = candidates.next().map(|(_, color, kind, _)| (color, kind));
    effects::resolve_pixel_color(
        &effects.color,
        window.effects,
        (front_color, front_kind, front_semi_transparent),
        next,
        effects.backdrop,
    )
}

#[cfg(test)]
mod tests {
    use super::{compose_frame, compose_frame_with_effects, BgSlot, FrameEffects};
    use crate::affine::AffineMatrix;
    use crate::bg_affine::{AffineBgLayer, AffineTilemap, Overflow};
    use crate::effects::{ColorEffect, EffectsConfig, LayerTargets};
    use crate::mosaic::MosaicSize;
    use crate::oam::ObjMode;
    use crate::oam::{OamEntry, ObjShape};
    use crate::palette::{Bgr555, Palette};
    use crate::sprite::SpriteLayer;
    use crate::tile::{BitDepth, Tileset};
    use crate::tilemap::{ScreenEntry, Tilemap};
    use crate::window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};

    /// A fully opaque 1x1-tile (8x8px) BG layer using palette index 15 in
    /// bank 0, plus its owning tileset/palette/tilemap (kept alive by the
    /// caller for the lifetime of the returned [`crate::bg::BgLayer`]).
    fn opaque_bg_fixture(color_channel: u8) -> (Tileset, Palette, Tilemap) {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(color_channel, 0, 0);
        let palette = Palette::new(colors);
        let entries = vec![ScreenEntry::new(0, false, false, 0)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();
        (tileset, palette, tilemap)
    }

    fn empty_sprite_layer<'a>(entries: &'a [OamEntry], tileset: &'a Tileset) -> SpriteLayer<'a> {
        SpriteLayer::new(entries, tileset, tileset, &EMPTY_PALETTE)
    }

    // A palette of all-default (black, and every index resolves to the same
    // color) used only where sprite entries are empty and never sampled.
    static EMPTY_PALETTE: Palette = Palette::new([Bgr555::from_raw(0); Palette::LEN]);

    #[test]
    fn bg_vs_bg_lower_priority_number_wins() {
        let (tiles_x, palette_x, map_x) = opaque_bg_fixture(1);
        let (tiles_y, palette_y, map_y) = opaque_bg_fixture(2);
        let layer_a = crate::bg::BgLayer::new(&tiles_x, &palette_x, &map_x);
        let layer_b = crate::bg::BgLayer::new(&tiles_y, &palette_y, &map_y);

        // BG1 (worse priority 3) vs BG0 (better priority 0, but declared
        // second) -- priority must win over bg_index/declaration order.
        let slots = [
            BgSlot::new(layer_a, 1, 3, 0, 0, true),
            BgSlot::new(layer_b, 0, 0, 0, 0, true),
        ];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(2, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn bg_vs_bg_same_priority_lower_bg_index_wins() {
        let (tiles_x, palette_x, map_x) = opaque_bg_fixture(1); // will be bg_index 2
        let (tiles_y, palette_y, map_y) = opaque_bg_fixture(2); // will be bg_index 0
        let layer_a = crate::bg::BgLayer::new(&tiles_x, &palette_x, &map_x);
        let layer_b = crate::bg::BgLayer::new(&tiles_y, &palette_y, &map_y);

        // Same priority (1) for both; bg_index 0 must win over bg_index 2
        // despite being declared second in the slice.
        let slots = [
            BgSlot::new(layer_a, 2, 1, 0, 0, true),
            BgSlot::new(layer_b, 0, 1, 0, 0, true),
        ];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(2, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn disabled_bg_slot_contributes_nothing() {
        let (ts, pal, tm) = opaque_bg_fixture(9);
        let layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        let slots = [BgSlot::new(layer, 0, 0, 0, 0, false)];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(fb.pixel(0, 0), Some(crate::palette::Rgb888::BLACK));
    }

    #[test]
    fn sprite_vs_bg_same_priority_sprite_wins() {
        let (ts, pal, tm) = opaque_bg_fixture(1);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        // BG at priority 2, best (lowest) bg_index (0) — still must lose to
        // a same-priority sprite.
        let slots = [BgSlot::new(bg_layer, 0, 2, 0, 0, true)];

        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
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
            2, // same priority as the BG
            true,
        )];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 9, 0).to_rgb888())
        );
    }

    #[test]
    fn sprite_lower_priority_number_beats_a_better_indexed_bg() {
        let (ts, pal, tm) = opaque_bg_fixture(1);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        // BG at the best possible priority (0).
        let slots = [BgSlot::new(bg_layer, 0, 0, 0, 0, true)];

        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
        let sprite_palette = Palette::new(sprite_colors);
        // Sprite at a WORSE priority (3) than the BG (0) -- the BG must win.
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
            3,
            true,
        )];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(1, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn transparent_sprite_pixel_lets_the_bg_show_through() {
        let (ts, pal, tm) = opaque_bg_fixture(4);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        let slots = [BgSlot::new(bg_layer, 0, 3, 0, 0, true)];

        // A fully transparent (all index-0) sprite at the best priority --
        // it must not occlude the BG at all.
        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0x00u8; 32]).unwrap();
        let sprite_palette = Palette::new([Bgr555::default(); Palette::LEN]);
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
        )];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(4, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn better_sprites_transparent_hole_promotes_a_worse_sprite_over_the_bg() {
        // Finding 1: opaque sprite B (priority 2) sits under sprite A
        // (priority 0), whose texel at this pixel is a transparent hole; a BG
        // sits between them at priority 1. On hardware A's hole upgrades B's
        // stored OBJ order to priority 0, so the OBJ layer (still showing B's
        // color) beats the BG — even though B's own priority (2) is worse
        // than the BG's (1). Pre-fix the OBJ pixel carried priority 2 and the
        // BG wrongly won.
        let (ts, pal, tm) = opaque_bg_fixture(7);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)]; // BG priority 1

        // A single shared 4bpp tileset: tile 0 fully opaque (B draws it),
        // tile 1 fully transparent (A draws it). B is OAM index 0 so it
        // writes first; A (index 1) then upgrades the order via its hole.
        let mut two_tiles = [0u8; 64];
        two_tiles[..32].copy_from_slice(&[0xFFu8; 32]); // tile 0 -> index 15 everywhere
        let shared = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0, 0, 9); // B's color (blue)
        let palette = Palette::new(colors);

        let b_opaque_prio2 = OamEntry::new(
            0,
            0,
            0, // tile 0 (opaque)
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            2,
            true,
        );
        let a_transparent_prio0 = OamEntry::new(
            0,
            0,
            1, // tile 1 (transparent)
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        );
        let entries = [b_opaque_prio2, a_transparent_prio0];
        let sprites = SpriteLayer::new(&entries, &shared, &shared, &palette);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0, 9).to_rgb888()),
            "B's color must beat the BG because A's hole upgrades it to priority 0"
        );
    }

    #[test]
    fn pixel_with_no_opaque_layer_stays_at_the_backdrop() {
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);
        let fb = compose_frame(&sprites, &[]);
        assert_eq!(fb.pixel(0, 0), Some(crate::palette::Rgb888::BLACK));
    }

    /// A fully opaque 1x1-tile (8x8px) *affine* BG layer using flat 8bpp
    /// palette index 200, plus its owning tileset/palette/tilemap.
    fn opaque_affine_bg_fixture(color_channel: u8) -> (Tileset, Palette, AffineTilemap) {
        let tileset = Tileset::decode(BitDepth::Bpp8, &[200u8; 64]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[200] = Bgr555::from_channels(color_channel, 0, 0);
        let palette = Palette::new(colors);
        let tilemap = AffineTilemap::new(1, 1, vec![0]).unwrap();
        (tileset, palette, tilemap)
    }

    #[test]
    fn affine_bg_slot_participates_in_priority_ordering_like_a_regular_bg() {
        // An affine BG (priority 0, best) must beat a regular BG (priority
        // 1) at the same pixel, exactly as two regular BGs would — proving
        // BgSlot::new_affine slots into compose_frame's existing ordering
        // without changing compose_frame's own signature.
        let (affine_tiles, affine_palette, affine_tilemap) = opaque_affine_bg_fixture(9);
        let affine_layer = AffineBgLayer::new(&affine_tiles, &affine_palette, &affine_tilemap);
        let (regular_tiles, regular_palette, regular_map) = opaque_bg_fixture(1);
        let regular_layer = crate::bg::BgLayer::new(&regular_tiles, &regular_palette, &regular_map);

        let slots = [
            BgSlot::new_affine(
                affine_layer,
                0,
                0, // best priority
                AffineMatrix::IDENTITY,
                0,
                0,
                Overflow::Transparent,
                true,
            ),
            BgSlot::new(regular_layer, 1, 1, 0, 0, true), // worse priority
        ];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
            "the affine BG's better priority must win"
        );
    }

    #[test]
    fn disabled_affine_bg_slot_contributes_nothing() {
        let (tiles, palette, tilemap) = opaque_affine_bg_fixture(9);
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let slots = [BgSlot::new_affine(
            layer,
            0,
            0,
            AffineMatrix::IDENTITY,
            0,
            0,
            Overflow::Transparent,
            false, // disabled
        )];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(fb.pixel(0, 0), Some(crate::palette::Rgb888::BLACK));
    }

    // -- S-2 slice 4 (issue #99): windows, color effects, mosaic -----------

    /// A 4bpp tile whose top-left 2x2 block has a distinct opaque color per
    /// pixel — (0,0)=index 1, (1,0)=index 2, (0,1)=index 3, (1,1)=index 4 —
    /// so mosaic block-snapping (which forces a whole block to read the
    /// block origin's pixel) is observable, unlike a uniformly-opaque tile.
    fn quadrant_bg_fixture() -> (Tileset, Palette, Tilemap) {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x21; // (0,0)=index 1, (1,0)=index 2
        bytes[4] = 0x43; // (0,1)=index 3, (1,1)=index 4
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(1, 0, 0);
        colors[2] = Bgr555::from_channels(2, 0, 0);
        colors[3] = Bgr555::from_channels(3, 0, 0);
        colors[4] = Bgr555::from_channels(4, 0, 0);
        let palette = Palette::new(colors);
        let entries = vec![ScreenEntry::new(0, false, false, 0)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();
        (tileset, palette, tilemap)
    }

    #[test]
    fn no_effects_default_reproduces_compose_frame_byte_for_byte() {
        // A non-trivial mixed scene (two BGs at different priorities plus a
        // sprite) composed both ways must match exactly -- the explicit
        // "no-effects default" regression the DoD calls for.
        let (tiles_a, palette_a, map_a) = opaque_bg_fixture(3);
        let (tiles_b, palette_b, map_b) = opaque_bg_fixture(6);
        let layer_a = crate::bg::BgLayer::new(&tiles_a, &palette_a, &map_a);
        let layer_b = crate::bg::BgLayer::new(&tiles_b, &palette_b, &map_b);
        let slots = [
            BgSlot::new(layer_a, 0, 1, 0, 0, true),
            BgSlot::new(layer_b, 1, 0, 0, 0, true),
        ];

        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
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
            2,
            true,
        )];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let via_compose_frame = compose_frame(&sprites, &slots);
        let via_effects_default =
            compose_frame_with_effects(&sprites, &slots, &FrameEffects::default());
        assert_eq!(via_compose_frame.pixels(), via_effects_default.pixels());
    }

    #[test]
    fn window_gates_a_bg_layer_by_region_even_though_its_own_enable_bit_is_on() {
        // BG0 (red, priority 0 -- otherwise always wins) is only enabled
        // inside WIN0 (x<4); BG1 (blue, priority 1) is only enabled via
        // WINOUT (outside every window). Both slots' own `enabled` bit is
        // `true` throughout -- only the window's per-region bg-enable bit
        // gates visibility here.
        let (tiles_r, palette_r, map_r) = opaque_bg_fixture(9); // red channel
        let (tiles_b, _palette_b, map_b) = opaque_bg_fixture(0); // blue via a dedicated color below
        let layer_r = crate::bg::BgLayer::new(&tiles_r, &palette_r, &map_r);
        let mut blue_colors = [Bgr555::default(); Palette::LEN];
        blue_colors[15] = Bgr555::from_channels(0, 0, 9);
        let blue_palette = Palette::new(blue_colors);
        let layer_b = crate::bg::BgLayer::new(&tiles_b, &blue_palette, &map_b);
        let slots = [
            BgSlot::new(layer_r, 0, 0, 0, 0, true),
            BgSlot::new(layer_b, 1, 1, 0, 0, true),
        ];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let mut bg0_only = WindowLayerEnable::NONE;
        bg0_only.bg[0] = true;
        let mut bg1_only = WindowLayerEnable::NONE;
        bg1_only.bg[1] = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: Some((
                    WindowRect::new(WindowRange::new(0, 4), WindowRange::new(0, 8)),
                    bg0_only,
                )),
                win1: None,
                obj_window: None,
                winout: bg1_only,
            },
            ..FrameEffects::default()
        };

        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
            "inside WIN0, only BG0 is enabled"
        );
        assert_eq!(
            fb.pixel(5, 0),
            Some(Bgr555::from_channels(0, 0, 9).to_rgb888()),
            "outside every window (WINOUT), only BG1 is enabled, despite BG0's better priority"
        );
    }

    #[test]
    fn objwin_mode_sprite_gates_a_layer_and_never_draws_its_own_color() {
        // A mode-2 (OBJWIN) sprite covering the right half of an 8x8 tile
        // only contributes a mask -- BG0 is enabled only where that mask is
        // set (via `obj_window`'s enable bits), and the mask sprite's own
        // (loud, distinctive) color must never appear on screen.
        let (tiles, palette, map) = opaque_bg_fixture(9);
        let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
        let slots = [BgSlot::new(layer, 0, 0, 0, 0, true)];

        // Each row is 4 bytes covering column pairs (0,1) (2,3) (4,5) (6,7);
        // a byte's low nibble is its left pixel, high nibble its right
        // (tile.rs's decode order). `0x00, 0x00, 0xFF, 0xFF` per row makes
        // columns 0..4 transparent and columns 4..8 opaque (index 15).
        let mut mask_tile = [0u8; 32];
        for row in mask_tile.chunks_exact_mut(4) {
            row.copy_from_slice(&[0x00, 0x00, 0xFF, 0xFF]);
        }
        let mask_tileset = Tileset::decode(BitDepth::Bpp4, &mask_tile).unwrap();
        let mut mask_colors = [Bgr555::default(); Palette::LEN];
        mask_colors[15] = Bgr555::from_channels(0, 31, 31); // a loud color that must never render
        let mask_palette = Palette::new(mask_colors);
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
        .with_mode(ObjMode::Window)];
        let sprites = SpriteLayer::new(&entries, &mask_tileset, &mask_tileset, &mask_palette);

        let mut bg0_only = WindowLayerEnable::NONE;
        bg0_only.bg[0] = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: None,
                win1: None,
                obj_window: Some(bg0_only),
                winout: WindowLayerEnable::NONE,
            },
            ..FrameEffects::default()
        };

        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(1, 0),
            Some(crate::palette::Rgb888::BLACK),
            "outside the OBJWIN mask, BG0 must stay disabled"
        );
        assert_eq!(
            fb.pixel(5, 0),
            Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
            "inside the OBJWIN mask, BG0 must be enabled"
        );
        assert_ne!(
            fb.pixel(5, 0),
            Some(Bgr555::from_channels(0, 31, 31).to_rgb888()),
            "the mask sprite itself must never draw a visible pixel"
        );
    }

    #[test]
    fn mosaic_snaps_bg_sampling_to_its_block_origin() {
        let (tileset, palette, tilemap) = quadrant_bg_fixture();
        let layer = crate::bg::BgLayer::new(&tileset, &palette, &tilemap);
        let slot = BgSlot::new(layer, 0, 0, 0, 0, true).with_mosaic(true);
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let effects = FrameEffects {
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(2, 2),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        // The whole 2x2 block containing (0,0)..(1,1) must all read the
        // block origin's color (index 1), not each pixel's own distinct
        // color.
        let origin_color = Bgr555::from_channels(1, 0, 0).to_rgb888();
        assert_eq!(fb.pixel(0, 0), Some(origin_color));
        assert_eq!(
            fb.pixel(1, 0),
            Some(origin_color),
            "snapped from (1,0) to (0,0)"
        );
        assert_eq!(
            fb.pixel(0, 1),
            Some(origin_color),
            "snapped from (0,1) to (0,0)"
        );
        assert_eq!(
            fb.pixel(1, 1),
            Some(origin_color),
            "snapped from (1,1) to (0,0)"
        );
    }

    #[test]
    fn mosaic_only_applies_to_bg_slots_with_their_own_mosaic_bit_set() {
        let (tileset, palette, tilemap) = quadrant_bg_fixture();
        let layer = crate::bg::BgLayer::new(&tileset, &palette, &tilemap);
        // Mosaic is NOT requested on this slot, even though a 2x2 BG mosaic
        // size is configured for the frame.
        let slot = BgSlot::new(layer, 0, 0, 0, 0, true);
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let effects = FrameEffects {
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(2, 2),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        assert_eq!(
            fb.pixel(1, 1),
            Some(Bgr555::from_channels(4, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn mosaic_snaps_obj_sampling_to_its_block_origin() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x21; // (0,0)=index 1, (1,0)=index 2
        bytes[4] = 0x43; // (0,1)=index 3, (1,1)=index 4
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

    #[test]
    fn alpha_blend_end_to_end_over_two_bg_layers() {
        // BG0 (black, priority 0, target1) alpha-blended with BG1 (white,
        // priority 1, target2) at eva=evb=8 (50/50) must land on the 5-bit
        // midpoint color, exactly as effects::alpha_blend computes in
        // isolation.
        let (tiles_a, palette_a, map_a) = opaque_bg_fixture(0);
        let (tiles_b, palette_b, map_b) = opaque_bg_fixture(31);
        let layer_a = crate::bg::BgLayer::new(&tiles_a, &palette_a, &map_a);
        let layer_b = crate::bg::BgLayer::new(&tiles_b, &palette_b, &map_b);
        let slots = [
            BgSlot::new(layer_a, 0, 0, 0, 0, true),
            BgSlot::new(layer_b, 1, 1, 0, 0, true),
        ];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let effects = FrameEffects {
            color: EffectsConfig {
                effect: ColorEffect::AlphaBlend,
                target1: LayerTargets {
                    bg: [true, false, false, false],
                    obj: false,
                    backdrop: false,
                },
                target2: LayerTargets {
                    bg: [false, true, false, false],
                    obj: false,
                    backdrop: false,
                },
                eva: 8,
                evb: 8,
                evy: 0,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(15, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn semi_transparent_obj_forces_blend_end_to_end_overriding_brighten() {
        // BLDCNT selects BRIGHTEN, and OBJ is not even configured target1 --
        // but a semi-transparent (OAM mode 1) sprite over a target2 BG must
        // still alpha-blend, per OamEntry::with_mode's docs.
        let (tiles, palette, map) = opaque_bg_fixture(31);
        let bg_layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
        let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)];

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

        let effects = FrameEffects {
            color: EffectsConfig {
                effect: ColorEffect::Brighten,
                target1: LayerTargets::default(), // OBJ not configured target1
                target2: LayerTargets {
                    bg: [true, false, false, false],
                    obj: false,
                    backdrop: false,
                },
                eva: 8,
                evb: 8,
                evy: 16,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(15, 0, 0).to_rgb888()),
            "semi-transparency must force alpha blend even though BRIGHTEN was selected"
        );
    }

    #[test]
    fn brighten_end_to_end_over_a_single_bg_layer() {
        let (tiles, palette, map) = opaque_bg_fixture(0);
        let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
        let slots = [BgSlot::new(layer, 0, 0, 0, 0, true)];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let effects = FrameEffects {
            color: EffectsConfig {
                effect: ColorEffect::Brighten,
                target1: LayerTargets {
                    bg: [true, false, false, false],
                    obj: false,
                    backdrop: false,
                },
                target2: LayerTargets::default(),
                eva: 0,
                evb: 0,
                evy: 16,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(31, 31, 31).to_rgb888()),
            "full brighten (evy=16) of black must reach white"
        );
    }

    #[test]
    fn backdrop_blending_end_to_end_when_nothing_is_behind_the_front_layer() {
        // A single BG (black, target1) alpha-blended against the backdrop
        // (white, target2-backdrop) when nothing else is drawn underneath.
        let (tiles, palette, map) = opaque_bg_fixture(0);
        let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
        let slots = [BgSlot::new(layer, 0, 0, 0, 0, true)];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let effects = FrameEffects {
            color: EffectsConfig {
                effect: ColorEffect::AlphaBlend,
                target1: LayerTargets {
                    bg: [true, false, false, false],
                    obj: false,
                    backdrop: false,
                },
                target2: LayerTargets {
                    bg: [false; 4],
                    obj: false,
                    backdrop: true,
                },
                eva: 8,
                evb: 8,
                evy: 0,
            },
            backdrop: Bgr555::from_channels(31, 31, 31).to_rgb888(),
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(15, 15, 15).to_rgb888()),
            "BG blended 50/50 with the white backdrop"
        );
    }
}
