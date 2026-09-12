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
use crate::bg_affine::{AffineBgLayer, AffineMosaicHold, Overflow};
use crate::effects::{self, EffectsConfig, LayerKind};
use crate::framebuffer::Framebuffer;
use crate::mosaic::{MosaicConfig, MosaicSize};
use crate::palette::Rgb888;
use crate::sprite::SpriteLayer;
use crate::window::{WindowConfig, WindowLayerEnable};

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
    ///
    /// `bg_open` is whether [`crate::window`] currently permits this slot's
    /// BG index to *composite* at `(x, y)`; `hold_active` is whether its
    /// affine mosaic hold should keep advancing regardless of that — see
    /// [`AffineMosaicHold`]'s docs for why the two can differ. The caller
    /// always passes both (rather than skipping the call when closed) so a
    /// `Some` `affine_mosaic_hold` is told about every column. Every other
    /// slot kind ignores both and returns `None` on `!bg_open`, exactly as
    /// the caller's own pre-existing early-`continue` did.
    fn sample(
        &self,
        x: usize,
        y: usize,
        bg_mosaic: MosaicSize,
        bg_open: bool,
        hold_active: bool,
        affine_mosaic_hold: Option<&mut AffineMosaicHold>,
    ) -> Option<Rgb888> {
        if let Some(hold) = affine_mosaic_hold {
            if !hold_active {
                hold.close();
                return None;
            }
            if let BgKind::Affine {
                layer,
                matrix,
                ref_x,
                ref_y,
                overflow: Overflow::Transparent,
            } = self.kind
            {
                let (_, snapped_y) = bg_mosaic.snap(0, y);
                let sample = layer.sample_column_with_mosaic_hold(
                    hold,
                    matrix,
                    ref_x,
                    ref_y,
                    x,
                    snapped_y,
                    bg_mosaic.horizontal(),
                );
                // Only the returned color, not the state update above, is
                // gated on `bg_open` -- see this function's docs.
                return if bg_open { sample } else { None };
            }
        }
        if !bg_open {
            return None;
        }
        let (x, y) = if self.mosaic {
            if self.wrap_affine_bypasses_horizontal_mosaic(bg_mosaic) {
                bg_mosaic.vertical_only().snap(x, y)
            } else {
                bg_mosaic.snap(x, y)
            }
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

    /// Whether this slot is an affine [`Overflow::Wrap`] layer at a decoded
    /// horizontal mosaic size mGBA bypasses entirely — the `Wrap` sibling of
    /// the same decoded-size gate
    /// [`AffineBgLayer::sample_column_with_mosaic_hold`] applies for
    /// `Overflow::Transparent`; see its docs for the mGBA derivation. Only
    /// `Wrap` needs a separate check here: it never reaches that function's
    /// retry/hold path at all `(behavioral-fidelity)`.
    fn wrap_affine_bypasses_horizontal_mosaic(&self, bg_mosaic: MosaicSize) -> bool {
        matches!(
            self.kind,
            BgKind::Affine {
                overflow: Overflow::Wrap,
                ..
            }
        ) && bg_mosaic.horizontal() <= 2
    }

    /// Whether this slot needs an [`AffineMosaicHold`]: only an *enabled*,
    /// mosaic-enabled, affine [`Overflow::Transparent`] slot does.
    /// [`Overflow::Wrap`] and regular BGs snap to the block origin
    /// statelessly; see
    /// [`AffineBgLayer::sample_column_with_mosaic_hold`]'s docs for why
    /// `Overflow::Transparent` alone needs retry/hold state.
    fn needs_affine_mosaic_hold(&self) -> bool {
        self.enabled
            && self.mosaic
            && matches!(
                self.kind,
                BgKind::Affine {
                    overflow: Overflow::Transparent,
                    ..
                }
            )
    }
}

/// A candidate pixel's ordering key: `(priority, layer_rank)`. Lower sorts
/// in front. A sprite's `layer_rank` is always `0`, strictly less than any
/// BG's `1 + bg_index` — so a sprite wins any same-priority tie against a
/// BG, and BGs break same-priority ties by ascending `bg_index`, matching
/// the ordering rules in the module docs.
type OrderKey = (u8, u8);
type Candidate = (OrderKey, Rgb888, LayerKind, bool);

/// Insert a layer into the two frontmost candidates for one pixel.
///
/// Candidates arrive in the same order the old stable sort saw them
/// (sprite, then BG slots), so strict comparisons preserve the existing
/// first-seen tie-break for duplicate order keys.
fn insert_candidate(
    front: &mut Option<Candidate>,
    next: &mut Option<Candidate>,
    candidate: Candidate,
) {
    match *front {
        None => *front = Some(candidate),
        Some(front_candidate) if candidate.0 < front_candidate.0 => {
            *next = *front;
            *front = Some(candidate);
        }
        Some(_) => match *next {
            None => *next = Some(candidate),
            Some(next_candidate) if candidate.0 < next_candidate.0 => {
                *next = Some(candidate);
            }
            Some(_) => {}
        },
    }
}

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
    // mgba's global "any target2" signal (software-obj.c:181-185): the
    // backdrop target2 bit, OR any BG that is a BLDCNT target2 *and* enabled.
    // A forced-alpha OBJ that fails to blend against its immediate neighbor
    // keeps its brighten/darken variant only when this is false (see
    // effects::resolve_pixel_color). It is a per-frame constant, so compute it
    // once rather than per pixel.
    let any_target2 = effects.color.target2.backdrop
        || bg_slots.iter().any(|slot| {
            slot.enabled && effects.color.target2.contains(LayerKind::Bg(slot.bg_index))
        });

    // Positionally paired with `bg_slots`; `None` where a slot needs no
    // hold. Every column of the scanline must advance these, open or closed:
    // a hold detects a window-closed gap only by being told about it.
    let mut affine_mosaic_holds: Vec<Option<AffineMosaicHold>> = bg_slots
        .iter()
        .map(|slot| {
            slot.needs_affine_mosaic_hold()
                .then(AffineMosaicHold::default)
        })
        .collect();

    let mut framebuffer = Framebuffer::new();
    let width = framebuffer.width();
    for y in 0..framebuffer.height() {
        for hold in affine_mosaic_holds.iter_mut().flatten() {
            *hold = AffineMosaicHold::default();
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the framebuffer is 160 scanlines tall, well within u8"
        )]
        let span_starts = effects.windows.scanline_span_starts(y as u8);
        for x in 0..width {
            if span_starts.contains(&x) {
                for hold in affine_mosaic_holds.iter_mut().flatten() {
                    hold.close();
                }
            }
            let color = compose_pixel(
                sprites,
                bg_slots,
                effects,
                any_target2,
                &mut affine_mosaic_holds,
                &span_starts,
                x,
                y,
            );
            framebuffer.set_pixel(x, y, color);
        }
    }
    framebuffer
}

/// Whether `bg_index`'s affine mosaic hold should keep advancing given
/// `partition_control` — the enable bits of whichever `WIN0`/`WIN1`/`WINOUT`
/// span this column falls in, independent of whether this exact column
/// *composites* (`bg_open`, which for an `OBJWIN`-masked column instead
/// reflects `OBJWIN`'s own control).
///
/// mGBA's `TEST_LAYER_ENABLED` runs a background's draw routine for a pass
/// whenever *either* that pass's own control (`currentWindow`, whichever of
/// `WIN0`/`WIN1`/`WINOUT` it is) or (when `OBJWIN` is enabled at all)
/// `OBJWIN`'s own control enables this BG — unconditionally, for every pass,
/// not only the outside-every-window one
/// (`mgba/src/gba/renderers/software-private.h:210-215`). See
/// [`AffineMosaicHold`]'s docs for why a pass that runs at all must keep
/// advancing the hold regardless of which columns `OBJWIN` later hides
/// `(behavioral-fidelity)`.
fn affine_mosaic_hold_participates(
    windows: &WindowConfig,
    partition_control: WindowLayerEnable,
    bg_index: u8,
) -> bool {
    partition_control.bg_enabled(bg_index)
        || windows
            .obj_window
            .is_some_and(|enable| enable.bg_enabled(bg_index))
}

/// Resolve one pixel's final color for [`compose_frame_with_effects`].
///
/// `affine_mosaic_holds` pairs positionally with `bg_slots` — one
/// [`AffineMosaicHold`] (or `None`) per slot, advanced one column at a time
/// across a scanline; see [`compose_frame_with_effects`]'s docs.
///
/// `window_spans` is the sorted, `0`-led list of screen columns where each
/// hardware-window span on this scanline begins; see
/// `sprite::SpriteLayer::sample_affine_local` for what it seeds and why.
#[expect(
    clippy::cast_possible_truncation,
    reason = "framebuffer coordinates are always < 240/160, well within u8"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the per-pixel state compose_frame_with_effects threads through every column"
)]
fn compose_pixel(
    sprites: &SpriteLayer<'_>,
    bg_slots: &[BgSlot<'_>],
    effects: &FrameEffects,
    any_target2: bool,
    affine_mosaic_holds: &mut [Option<AffineMosaicHold>],
    window_spans: &[usize],
    x: usize,
    y: usize,
) -> Rgb888 {
    let (wx, wy) = (x as u8, y as u8);

    let objwin_mask = effects.windows.obj_window.is_some()
        && sprites.objwin_mask_with_mosaic(x, y, effects.mosaic.obj);
    let (window, region) = effects.windows.classify_with_region(wx, wy, objwin_mask);

    // An `OBJWIN` mask never partitions the scanline, so the enable bits
    // that decide whether a span runs a layer's draw routine at all come
    // from a classification with the mask forced off
    // (`mgba/src/gba/renderers/video-software.c:903-912`)
    // `(behavioral-fidelity)`.
    let (partition_control, _) = effects.windows.classify_with_region(wx, wy, false);

    let mut front = None;
    let mut next = None;

    if window.obj {
        if let Some(pixel) = sprites.resolve_pixel_with_mosaic_windowed(
            x,
            y,
            effects.mosaic.obj,
            window_spans,
            region.suppresses_objwin_hole(),
        ) {
            insert_candidate(
                &mut front,
                &mut next,
                (
                    (pixel.priority, 0),
                    pixel.color,
                    LayerKind::Obj,
                    pixel.semi_transparent,
                ),
            );
        }
    }
    for (slot, affine_mosaic_hold) in bg_slots.iter().zip(affine_mosaic_holds.iter_mut()) {
        if !slot.enabled {
            continue;
        }
        let bg_open = window.bg_enabled(slot.bg_index);
        let hold_active =
            affine_mosaic_hold_participates(&effects.windows, partition_control, slot.bg_index);
        let Some(color) = slot.sample(
            x,
            y,
            effects.mosaic.bg,
            bg_open,
            hold_active,
            affine_mosaic_hold.as_mut(),
        ) else {
            continue;
        };
        insert_candidate(
            &mut front,
            &mut next,
            (
                (slot.priority, 1 + slot.bg_index),
                color,
                LayerKind::Bg(slot.bg_index),
                false,
            ),
        );
    }

    let Some((_, front_color, front_kind, front_semi_transparent)) = front else {
        // Nothing drawn: the backdrop itself is shown, subject only to
        // brighten/darken (effects::resolve_pixel_color never alpha-blends
        // the backdrop against itself).
        return effects::resolve_pixel_color(
            &effects.color,
            window.effects,
            any_target2,
            (effects.backdrop, LayerKind::Backdrop, false),
            None,
            effects.backdrop,
        );
    };
    let next = next.map(|(_, color, kind, _)| (color, kind));
    effects::resolve_pixel_color(
        &effects.color,
        window.effects,
        any_target2,
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
    use crate::palette::{Bgr555, Palette, Rgb888};
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

    /// A single 8x8-tile *affine* BG layer whose columns 0..8 each carry a
    /// distinct opaque color (channel `column + 1`) -- distinguishes "holds
    /// its own column's texel" from "holds a neighbor's" at column
    /// granularity, unlike [`opaque_affine_bg_fixture`]'s single flat color.
    fn gradient_affine_bg_fixture() -> (Tileset, Palette, AffineTilemap) {
        let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
        let mut bytes = vec![0u8; tile_byte_len];
        for (column, byte) in bytes[..BitDepth::TILE_DIM].iter_mut().enumerate() {
            *byte = u8::try_from(column + 1).unwrap();
        }
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        for (column, color) in colors[1..=BitDepth::TILE_DIM].iter_mut().enumerate() {
            *color = Bgr555::from_channels(u8::try_from(column + 1).unwrap(), 0, 0);
        }
        let palette = Palette::new(colors);
        let tilemap = AffineTilemap::new(1, 1, vec![0]).unwrap();
        (tileset, palette, tilemap)
    }

    /// An 8-tile-wide, 1-tile-tall *affine* BG layer whose tiles 0..8 are
    /// each flat-filled with a distinct opaque color (channel
    /// `tile_index + 1`) -- for tests that need to distinguish which *tile*
    /// (not just which column within one tile) a sample landed in, e.g. with
    /// a scaled affine matrix that crosses tile boundaries over a handful of
    /// screen columns.
    fn eight_tile_gradient_affine_bg_fixture() -> (Tileset, Palette, AffineTilemap) {
        const TILE_COUNT: usize = 8;
        let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
        let mut bytes = vec![0u8; tile_byte_len * TILE_COUNT];
        for (tile_index, chunk) in bytes.chunks_exact_mut(tile_byte_len).enumerate() {
            chunk.fill(u8::try_from(tile_index + 1).unwrap());
        }
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        for (tile_index, color) in colors[1..=TILE_COUNT].iter_mut().enumerate() {
            *color = Bgr555::from_channels(u8::try_from(tile_index + 1).unwrap(), 0, 0);
        }
        let palette = Palette::new(colors);
        let tile_indices: Vec<u8> = (0..TILE_COUNT).map(|i| u8::try_from(i).unwrap()).collect();
        let tilemap = AffineTilemap::new(TILE_COUNT, 1, tile_indices).unwrap();
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
    fn affine_mosaic_retries_the_next_pixel_when_the_block_origin_is_out_of_bounds() {
        // The identity matrix with the reference point one texture pixel to
        // the left puts screen x=0 at texture x=-1 (out of bounds under
        // `Overflow::Transparent`) and screen x=1 at texture x=0. mGBA's
        // mode-2 mosaic fetch `continue`s past an out-of-bounds coordinate
        // without reloading `mosaicWait`, so the next horizontal pixel
        // retries the fetch and draws -- and, since the block is 4 wide, x=2
        // and x=3 then hold that retried value rather than each
        // independently re-snapping to the rejected origin
        // (`MODE_2_COORD_NO_OVERFLOW`/`MODE_2_MOSAIC`,
        // `mgba/src/gba/renderers/software-bg.c:24-42`) `(behavioral-fidelity)`.
        let (tiles, palette, tilemap) = opaque_affine_bg_fixture(9);
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let one_texture_pixel = i32::from(AffineMatrix::ONE);
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            AffineMatrix::IDENTITY,
            -one_texture_pixel,
            0,
            Overflow::Transparent,
            true,
        )
        .with_mosaic(true);
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let effects = FrameEffects {
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(4, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let held = Bgr555::from_channels(9, 0, 0).to_rgb888();
        assert_eq!(
            fb.pixel(0, 0),
            Some(crate::palette::Rgb888::BLACK),
            "texture x=-1 is out of bounds, so this pixel draws nothing"
        );
        assert_eq!(
            fb.pixel(1, 0),
            Some(held),
            "the retry at screen x=1 samples texture x=0 and draws it"
        );
        assert_eq!(
            fb.pixel(2, 0),
            Some(held),
            "x=2 holds the value the retry drew at x=1, not a fresh (still in-bounds) sample"
        );
        assert_eq!(
            fb.pixel(3, 0),
            Some(held),
            "x=3 holds the value the retry drew at x=1, completing the 4-wide block"
        );
    }

    #[test]
    fn affine_mosaic_wrap_overflow_still_snaps_to_the_block_origin() {
        // `Overflow::Wrap` always succeeds (it masks into range), so it never
        // hits the retry path -- the pre-existing block-origin snap already
        // matches mGBA's overflow-branch affine mosaic exactly. This guards
        // both sides of that: above
        // `BgSlot::wrap_affine_bypasses_horizontal_mosaic`'s decoded-size
        // gate a 4-wide block must still collapse to its origin's texel, and
        // that origin must stay wrapped rather than going transparent. A
        // per-column gradient is what makes snapping visible; the reference
        // point sits one texture pixel left, so the block origin wraps to
        // texture column 7 and the next block lands on column 3.
        let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let one_texture_pixel = i32::from(AffineMatrix::ONE);
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            AffineMatrix::IDENTITY,
            -one_texture_pixel,
            0,
            Overflow::Wrap,
            true,
        )
        .with_mosaic(true);
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let effects = FrameEffects {
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(4, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let wrapped_origin = Bgr555::from_channels(8, 0, 0).to_rgb888();
        for x in 0..4 {
            assert_eq!(
                fb.pixel(x, 0),
                Some(wrapped_origin),
                "x={x} must draw the 4-wide block origin's wrapped texel"
            );
        }
        assert_eq!(
            fb.pixel(4, 0),
            Some(Bgr555::from_channels(4, 0, 0).to_rgb888()),
            "x=4 starts the next block and snaps to its own origin"
        );
    }

    #[test]
    fn affine_mosaic_wrap_overflow_bypasses_snapping_at_a_decoded_block_size_of_two() {
        // `BgSlot::wrap_affine_bypasses_horizontal_mosaic`'s docs cite the
        // mGBA decoded-size gate this proves: a 2-wide BG mosaic must leave
        // x=0 and x=1 sampling their own per-column-gradient texel, not both
        // snapped to column 0.
        let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            AffineMatrix::IDENTITY,
            0,
            0,
            Overflow::Wrap,
            true,
        )
        .with_mosaic(true);
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let effects = FrameEffects {
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(2, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
        let column1 = Bgr555::from_channels(2, 0, 0).to_rgb888();
        assert_eq!(fb.pixel(0, 0), Some(column0), "x=0 samples its own column");
        assert_eq!(
            fb.pixel(1, 0),
            Some(column1),
            "x=1 must sample its own column, not snap to x=0's block origin"
        );
    }

    #[test]
    fn affine_mosaic_hold_reopens_seeded_from_the_snapped_origin_not_the_pre_gap_hold() {
        // mGBA re-invokes its mode-2 background draw routine (and re-derives
        // `mosaicWait` and the snapped block origin) once per hardware-window
        // region in which the layer is enabled
        // (`mgba/src/gba/renderers/video-software.c:628-675`,
        // `mgba/src/gba/renderers/software-private.h:173-192`), so a window
        // gap must reset the retry/hold state rather than merely hide it. The
        // reopened span is then seeded from the *snapped block origin*
        // column, not left blank and not resuming the pre-gap hold
        // (`software-bg.c:66-73`).
        //
        // A per-column gradient (distinct color per texture column) makes
        // all three outcomes distinguishable at x=5: the pre-gap hold (from
        // x=0..2) is column 0's color; a naive "start blank" would be
        // `None`; the correct seed is column 4's color (the snapped origin
        // for block 4 at x=5).
        let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            AffineMatrix::IDENTITY,
            0,
            0,
            Overflow::Transparent,
            true,
        )
        .with_mosaic(true);
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        // WIN0 excludes BG0 for x in [3, 5); WINOUT (everywhere else)
        // includes it, so BG0 is open at x=0..3, closed at x=3..5, and open
        // again at x=5..8.
        let bg0_off = WindowLayerEnable::NONE;
        let mut bg0_on = WindowLayerEnable::NONE;
        bg0_on.bg[0] = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: Some((
                    WindowRect::new(WindowRange::new(3, 5), WindowRange::new(0, 1)),
                    bg0_off,
                )),
                win1: None,
                obj_window: None,
                winout: bg0_on,
            },
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(4, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
        let column4 = Bgr555::from_channels(5, 0, 0).to_rgb888();
        assert_eq!(
            fb.pixel(2, 0),
            Some(column0),
            "x=2 holds column 0's texel, fetched at x=0; the window closes right after"
        );
        assert_eq!(
            fb.pixel(3, 0),
            Some(crate::palette::Rgb888::BLACK),
            "x=3 is window-closed: nothing from this slot draws here"
        );
        assert_eq!(
            fb.pixel(5, 0),
            Some(column4),
            "x=5 reopens mid-block (phase 1 of 4): seeded from the snapped \
             origin at x=4 (column 4), not blank and not column 0's pre-gap hold"
        );
        assert_eq!(
            fb.pixel(6, 0),
            Some(column4),
            "x=6 still holds the seed from x=5's span"
        );
        assert_eq!(
            fb.pixel(7, 0),
            Some(column4),
            "x=7 still holds the seed from x=5's span"
        );
    }

    #[test]
    fn affine_mosaic_hold_does_not_cross_a_same_enabled_window_region_boundary() {
        // mGBA partitions a scanline into hardware-window regions and never
        // coalesces adjacent regions with equal control bits -- it
        // re-invokes its background draw routine (and re-derives mosaic hold
        // state fresh from that region's own start column) once per region,
        // even where the same BG stays enabled across the boundary between
        // two of them (`mgba/src/gba/renderers/video-software.c:628-675,458-505`).
        // WIN0 [0, 5) and WINOUT both enable BG0 here, so BG0's own enable
        // bit never toggles -- x=5 is still a region boundary (WIN0 ->
        // WINOUT), and the hold must not survive it.
        //
        // An 8x horizontal scale makes screen x map to texture x =
        // 8*(screen_x - 3), landing each screen column in a different one
        // of 8 flat-colored tiles a few columns apart.
        let (tiles, palette, tilemap) = eight_tile_gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let scale = 8 * AffineMatrix::ONE;
        let matrix = AffineMatrix::new(scale, 0, 0, AffineMatrix::ONE);
        let reference_x = -24 * i32::from(AffineMatrix::ONE); // texture x = 8*(screen_x - 3)
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            matrix,
            reference_x,
            0,
            Overflow::Transparent,
            true,
        )
        .with_mosaic(true);
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let mut bg0_on = WindowLayerEnable::NONE;
        bg0_on.bg[0] = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: Some((
                    WindowRect::new(WindowRange::new(0, 5), WindowRange::new(0, 1)),
                    bg0_on,
                )),
                win1: None,
                obj_window: None,
                winout: bg0_on,
            },
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(4, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let first_tile_color = Bgr555::from_channels(1, 0, 0).to_rgb888();
        let second_tile_color = Bgr555::from_channels(2, 0, 0).to_rgb888();
        assert_eq!(
            fb.pixel(4, 0),
            Some(first_tile_color),
            "x=4 still holds tile 0, fetched at x=3 inside WIN0"
        );
        assert_eq!(
            fb.pixel(5, 0),
            Some(second_tile_color),
            "x=5 starts a fresh span at the WIN0/WINOUT boundary, seeded from the \
             snapped origin (screen x=4, texture tile 1) -- not tile 0's held \
             color carried over from inside WIN0"
        );
    }

    #[test]
    fn affine_mosaic_hold_does_not_cross_a_zero_width_window_boundary() {
        // A WIN0 with equal horizontal endpoints matches no pixel, but mGBA
        // still splits the scanline around it: `_breakWindowInner` inserts
        // the prefix segment [0, 5), the empty segment [5, 5), and the
        // suffix segment [5, 240) as three distinct windows
        // (`mgba/src/gba/renderers/video-software.c:458-496`), and the
        // scanline loop re-invokes the mode-2 draw routine -- re-deriving
        // `mosaicWait` and the snapped block origin from that segment's own
        // start column -- once per segment
        // (`video-software.c:628-675`, `software-private.h:173-192`). So
        // x=5 is a hold-resetting boundary here for exactly the same reason
        // it is in `affine_mosaic_hold_does_not_cross_a_same_enabled_window_region_boundary`,
        // whose geometry and expectations this mirrors with WIN0 [0, 5)
        // replaced by the zero-width WIN0 [5, 5).
        let (tiles, palette, tilemap) = eight_tile_gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let scale = 8 * AffineMatrix::ONE;
        let matrix = AffineMatrix::new(scale, 0, 0, AffineMatrix::ONE);
        let reference_x = -24 * i32::from(AffineMatrix::ONE); // texture x = 8*(screen_x - 3)
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            matrix,
            reference_x,
            0,
            Overflow::Transparent,
            true,
        )
        .with_mosaic(true);
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let mut bg0_on = WindowLayerEnable::NONE;
        bg0_on.bg[0] = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: Some((
                    WindowRect::new(WindowRange::new(5, 5), WindowRange::new(0, 1)),
                    bg0_on,
                )),
                win1: None,
                obj_window: None,
                winout: bg0_on,
            },
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(4, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let first_tile_color = Bgr555::from_channels(1, 0, 0).to_rgb888();
        let second_tile_color = Bgr555::from_channels(2, 0, 0).to_rgb888();
        assert_eq!(
            fb.pixel(4, 0),
            Some(first_tile_color),
            "x=4 still holds tile 0, fetched at x=3 before the split"
        );
        assert_eq!(
            fb.pixel(5, 0),
            Some(second_tile_color),
            "x=5 starts mGBA's suffix segment, so the span reopens seeded from the \
             snapped origin (screen x=4, texture tile 1) -- not tile 0's held color"
        );
    }

    #[test]
    fn affine_mosaic_hold_survives_an_objwin_mask_edge() {
        // See `AffineMosaicHold`'s docs for why an OBJWIN mask edge must not
        // reset the hold. BG0 is enabled both by WINOUT and by OBJWIN here,
        // so it stays visible on both sides of the mask edge at x=4.
        //
        // Reference x=-3 texture pixels puts screen x=0..2 out of bounds
        // (texture x=-3..-1), screen x=3 at texture x=0 (column 0, channel 1)
        // -- the block-4 hold then covers x=3..6.
        let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let one_texture_pixel = i32::from(AffineMatrix::ONE);
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            AffineMatrix::IDENTITY,
            -3 * one_texture_pixel,
            0,
            Overflow::Transparent,
            true,
        )
        .with_mosaic(true);

        // A mode-window sprite masking columns 4..8 (as in
        // `objwin_mode_sprite_gates_a_layer_and_never_draws_its_own_color`),
        // putting the OBJWIN edge at x=4, inside the x=3..6 hold span.
        let mut mask_tile = [0u8; 32];
        for row in mask_tile.chunks_exact_mut(4) {
            row.copy_from_slice(&[0x00, 0x00, 0xFF, 0xFF]);
        }
        let mask_tileset = Tileset::decode(BitDepth::Bpp4, &mask_tile).unwrap();
        let mut mask_colors = [Bgr555::default(); Palette::LEN];
        mask_colors[15] = Bgr555::from_channels(0, 31, 31);
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

        let mut bg0_on = WindowLayerEnable::NONE;
        bg0_on.bg[0] = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: None,
                win1: None,
                obj_window: Some(bg0_on),
                winout: bg0_on,
            },
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(4, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
        assert_eq!(
            fb.pixel(3, 0),
            Some(column0),
            "x=3 (outside the mask) establishes the hold"
        );
        assert_eq!(
            fb.pixel(5, 0),
            Some(column0),
            "x=5 (inside the mask, past the x=4 OBJWIN edge) must still hold \
             column 0's texel -- the edge is not a region boundary"
        );
    }

    #[test]
    fn affine_mosaic_hold_advances_through_pixels_hidden_by_objwin() {
        // See `affine_mosaic_hold_participates`'s docs for why a column
        // OBJWIN hides must still advance the hold. WINOUT enables BG0 but
        // OBJWIN does not, so masked columns draw nothing while the hold
        // keeps advancing underneath.
        //
        // Same setup as the mask-edge test above: reference x=-3 texture
        // pixels and a block-4 hold spanning x=3..6, holding column 0's
        // (channel 1) texel.
        let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let one_texture_pixel = i32::from(AffineMatrix::ONE);
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            AffineMatrix::IDENTITY,
            -3 * one_texture_pixel,
            0,
            Overflow::Transparent,
            true,
        )
        .with_mosaic(true);

        // A mode-window sprite masking only columns 4 and 5: row bytes
        // `0x00, 0x00, 0xFF, 0x00` leave column pairs (0,1) and (2,3)
        // transparent, (4,5) opaque, and (6,7) transparent again (tile.rs's
        // decode order, as in the sibling mask-edge test above).
        let mut mask_tile = [0u8; 32];
        for row in mask_tile.chunks_exact_mut(4) {
            row.copy_from_slice(&[0x00, 0x00, 0xFF, 0x00]);
        }
        let mask_tileset = Tileset::decode(BitDepth::Bpp4, &mask_tile).unwrap();
        let mut mask_colors = [Bgr555::default(); Palette::LEN];
        mask_colors[15] = Bgr555::from_channels(0, 31, 31);
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

        let mut bg0_on = WindowLayerEnable::NONE;
        bg0_on.bg[0] = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: None,
                win1: None,
                obj_window: Some(WindowLayerEnable::NONE), // OBJWIN never enables BG0
                winout: bg0_on,
            },
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(4, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
        assert_eq!(
            fb.pixel(3, 0),
            Some(column0),
            "x=3 (unmasked) establishes the hold"
        );
        assert_eq!(
            fb.pixel(4, 0),
            Some(crate::palette::Rgb888::BLACK),
            "x=4 is OBJWIN-hidden: nothing composites, but the hold must still advance"
        );
        assert_eq!(
            fb.pixel(5, 0),
            Some(crate::palette::Rgb888::BLACK),
            "x=5 is still OBJWIN-hidden"
        );
        assert_eq!(
            fb.pixel(6, 0),
            Some(column0),
            "x=6 (unmasked again) must still show column 0's held texel -- the \
             hidden columns advanced the hold invisibly rather than resetting it"
        );
    }

    #[test]
    fn affine_mosaic_hold_advances_through_unmasked_columns_an_objwin_only_bg_cannot_show() {
        // The mirror image of the two OBJWIN tests above: here WINOUT
        // disables BG0 and only OBJWIN enables it, so an *unmasked* column
        // both composites nothing (OBJWIN doesn't cover it, and WINOUT
        // wouldn't show it anyway) and must still let the hold advance --
        // `affine_mosaic_hold_participates`'s OR must not depend on which of
        // its two terms happens to be the one composited elsewhere.
        //
        // Same reference/mosaic setup as the sibling tests: reference x=-3
        // texture pixels and a block-4 hold spanning x=3..6, holding column
        // 0's (channel 1) texel. The mask (columns 4,5 opaque) leaves x=3
        // and x=6 unmasked.
        let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let one_texture_pixel = i32::from(AffineMatrix::ONE);
        let slot = BgSlot::new_affine(
            layer,
            0,
            0,
            AffineMatrix::IDENTITY,
            -3 * one_texture_pixel,
            0,
            Overflow::Transparent,
            true,
        )
        .with_mosaic(true);

        let mut mask_tile = [0u8; 32];
        for row in mask_tile.chunks_exact_mut(4) {
            row.copy_from_slice(&[0x00, 0x00, 0xFF, 0x00]);
        }
        let mask_tileset = Tileset::decode(BitDepth::Bpp4, &mask_tile).unwrap();
        let mut mask_colors = [Bgr555::default(); Palette::LEN];
        mask_colors[15] = Bgr555::from_channels(0, 31, 31);
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

        let mut bg0_on = WindowLayerEnable::NONE;
        bg0_on.bg[0] = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: None,
                win1: None,
                obj_window: Some(bg0_on),        // only OBJWIN enables BG0
                winout: WindowLayerEnable::NONE, // WINOUT never does
            },
            mosaic: crate::mosaic::MosaicConfig {
                bg: MosaicSize::new(4, 1),
                obj: MosaicSize::NONE,
            },
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

        let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
        assert_eq!(
            fb.pixel(3, 0),
            Some(crate::palette::Rgb888::BLACK),
            "x=3 is unmasked (WINOUT never enables BG0 here) but must still \
             establish the hold from OBJWIN's participation alone"
        );
        assert_eq!(
            fb.pixel(4, 0),
            Some(column0),
            "x=4 is masked, so OBJWIN's enable composites the held texel"
        );
        assert_eq!(fb.pixel(5, 0), Some(column0), "x=5 is still masked");
        assert_eq!(
            fb.pixel(6, 0),
            Some(crate::palette::Rgb888::BLACK),
            "x=6 is unmasked again -- composites nothing, but the hold state \
             established at x=3 must have kept advancing underneath"
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
    fn affine_obj_mosaic_hold_restarts_at_a_hardware_window_span_boundary() {
        // mGBA re-invokes sprite preprocessing once per hardware-window span
        // and seeds an affine OBJ's mosaic hold from the column one left of
        // that span's start, not the screen-aligned block origin
        // (`mgba/src/gba/renderers/video-software.c:1052-1062`,
        // `software-obj.c:227-242,49-70`).
        use crate::oam::AffineMode;

        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // row 0: col 0 -> index 1
        bytes[2] = 0x32; // row 0: col 4 -> index 2, col 5 -> index 3
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
        let sprites = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

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
        // mGBA rounds an affine mosaic OBJ's `condition` past the span end
        // when the sprite's own right edge stops before it
        // (`software-obj.c:227-242`: the `end` clamp happens *before* the
        // mosaic rounding, which is then skipped only when `condition ==
        // end`), so the earlier span's pass writes the spill columns into the
        // per-scanline `spriteLayer` buffer. That buffer is cleared once per
        // scanline (`video-software.c:894-901`) and composited per span
        // (`GBAVideoSoftwareRendererPostprocessSprite`), so the later span
        // shows the earlier pass's held column.
        //
        // Identity 8x8 affine OBJ at x = 1, OBJ mosaic H = 4, WIN0 opening at
        // x = 10. Raw right edge 9 rounds to 12, so x = 10..=11 are the spill
        // of block [8, 12) and must still show source col 7.
        use crate::oam::AffineMode;

        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // row 0: col 0 -> index 1
        bytes[3] = 0x23; // row 0: col 6 -> index 3, col 7 -> index 2
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
        let sprites = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

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

    #[test]
    fn affine_obj_mosaic_hold_restarts_when_the_window_span_starts_at_the_raw_edge() {
        // When the sprite's raw right edge lands exactly on a window-span
        // boundary, mGBA's own pass never rounds: `condition` (the raw edge)
        // equals `end` (the span's own end), and mosaic rounding is skipped
        // whenever `condition == end` (`software-obj.c:227-239`). The
        // rounding instead belongs to the *next* span, which starts exactly
        // at the raw edge and independently seeds its hold from its own
        // start (`software-obj.c:241`) -- so the spill columns must look up
        // the span containing the raw edge itself, not the span one column
        // before it.
        //
        // Identity 8x8 affine OBJ at x = 2 (raw right edge 10), OBJ mosaic H
        // = 4, WIN0 opening at x = 10 (exactly the raw edge). The leading
        // block [8, 12) is split: x = 8..=9 hold source col 6 from the
        // WINOUT pass (no spill, since its own `condition == end`), and
        // x = 10..=11 hold source col 7, restarted by the WIN0 pass.
        use crate::oam::AffineMode;

        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // row 0: col 0 -> index 1
        bytes[3] = 0x23; // row 0: col 6 -> index 3, col 7 -> index 2
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
        let sprites = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

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
    fn alpha_blend_end_to_end_over_two_bg_layers() {
        // BG0 (r channel 0, priority 0, target1) alpha-blended with BG1 (r
        // channel 31 -> byte 255, priority 1, target2) at eva=evb=8 (50/50)
        // must land on the same 8-bit-oracle midpoint effects::alpha_blend
        // computes in isolation (see effects tests'
        // alpha_blend_hand_computed_50_50): (0*8+255*8)/16 = 127.
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
        assert_eq!(fb.pixel(0, 0), Some(Rgb888 { r: 127, g: 0, b: 0 }));
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
        // Same eva=evb=8 blend of r channels 0 and 255 as
        // alpha_blend_end_to_end_over_two_bg_layers above: 127.
        assert_eq!(
            fb.pixel(0, 0),
            Some(Rgb888 { r: 127, g: 0, b: 0 }),
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
        // eva=evb=8 blend of (0,0,0) and (255,255,255) per channel (8-bit
        // oracle, module docs): (0*8+255*8)/16 = 127 on every channel.
        assert_eq!(
            fb.pixel(0, 0),
            Some(Rgb888 {
                r: 127,
                g: 127,
                b: 127
            }),
            "BG blended 50/50 with the white backdrop"
        );
    }

    #[test]
    fn objwin_transparent_hole_promotes_a_worse_sprite_over_the_bg() {
        // Finding 1 end-to-end: opaque sprite B (priority 2, OAM index 0) sits
        // under a priority-0 OBJWIN-mode sprite whose texel here is a
        // transparent hole; a BG sits between them at priority 1. mgba's
        // SPRITE_DRAW_PIXEL_*_OBJWIN transparent branch upgrades B's stored OBJ
        // order to 0, so the OBJ layer (still B's color) beats the BG, even
        // though B's own priority (2) is worse than the BG's (1).
        let (ts, pal, tm) = opaque_bg_fixture(7);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)]; // BG priority 1

        // tile 0 fully opaque (B), tile 1 fully transparent (the OBJWIN hole).
        let mut two_tiles = [0u8; 64];
        two_tiles[..32].copy_from_slice(&[0xFFu8; 32]);
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
        let objwin_hole_prio0 = OamEntry::new(
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
        )
        .with_mode(ObjMode::Window);

        let entries = [b_opaque_prio2, objwin_hole_prio0];
        let sprites = SpriteLayer::new(&entries, &shared, &shared, &palette);
        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0, 9).to_rgb888()),
            "the OBJWIN hole upgrades B to priority 0, beating the BG"
        );

        // Control: without the OBJWIN sprite, B stays priority 2 and the
        // priority-1 BG wins.
        let control_entries = [b_opaque_prio2];
        let control_sprites = SpriteLayer::new(&control_entries, &shared, &shared, &palette);
        let control_fb = compose_frame(&control_sprites, &slots);
        assert_eq!(
            control_fb.pixel(0, 0),
            Some(Bgr555::from_channels(7, 0, 0).to_rgb888()),
            "without the OBJWIN hole, B keeps priority 2 and the BG wins"
        );
    }

    #[test]
    fn semi_transparent_obj_variant_dropped_by_a_deeper_enabled_target2_bg() {
        // Finding 3 end-to-end: a semi-transparent OBJ (forced alpha) that is a
        // BLDCNT first target under BRIGHTEN sits over BG_a (priority 1, its
        // immediate neighbour, NOT a target2) with BG_b (priority 2, a target2)
        // enabled deeper in the frame. Because *some* target2 exists globally,
        // mgba clears the brighten variant and the OBJ shows its raw (black)
        // color. The control clears BG_b's target2 bit -> no target2 anywhere
        // -> the OBJ is brightened to white.
        let (tiles_a, palette_a, map_a) = opaque_bg_fixture(5);
        let (tiles_b, palette_b, map_b) = opaque_bg_fixture(10);
        let layer_a = crate::bg::BgLayer::new(&tiles_a, &palette_a, &map_a);
        let layer_b = crate::bg::BgLayer::new(&tiles_b, &palette_b, &map_b);
        let slots = [
            BgSlot::new(layer_a, 0, 1, 0, 0, true), // BG0, priority 1 (immediate next)
            BgSlot::new(layer_b, 1, 2, 0, 0, true), // BG1, priority 2 (deeper)
        ];

        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 0, 0); // black
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
            0, // priority 0: the front layer
            true,
        )
        .with_mode(ObjMode::SemiTransparent)];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let base_color = EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets {
                bg: [false; 4],
                obj: true,
                backdrop: false,
            },
            target2: LayerTargets {
                bg: [false, true, false, false], // BG1 (deeper) is a target2
                obj: false,
                backdrop: false,
            },
            eva: 8,
            evb: 8,
            evy: 16,
        };

        let effects = FrameEffects {
            color: base_color,
            ..FrameEffects::default()
        };
        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0, 0).to_rgb888()),
            "a deeper enabled target2 BG clears the variant -> raw (black) OBJ"
        );

        // Control: drop BG1's target2 bit -> no target2 anywhere -> variant
        // survives -> the OBJ is brightened to white.
        let mut control_color = base_color;
        control_color.target2 = LayerTargets::default();
        let control_effects = FrameEffects {
            color: control_color,
            ..FrameEffects::default()
        };
        let control_fb = compose_frame_with_effects(&sprites, &slots, &control_effects);
        assert_eq!(
            control_fb.pixel(0, 0),
            Some(Bgr555::from_channels(31, 31, 31).to_rgb888()),
            "no target2 anywhere -> the semi-transparent OBJ is brightened to white"
        );
    }

    // -- S-2, issue #329: per-scanline OAM admission budget ----------------

    #[test]
    fn compositor_agrees_between_the_objwin_mask_and_visible_sprite_layer_under_exhaustion() {
        // The OBJWIN mask and the visible OBJ layer are gated by the exact
        // same per-scanline OAM admission stage (`crate::oam_budget`), so
        // they must move together as the scanline's cycle budget is
        // exhausted or not -- neither can show a late sprite the other has
        // already dropped.
        //
        // Two late entries: an OBJWIN-mode sprite at x=0 (would enable BG1
        // there via the `obj_window` mask) and a Normal-mode sprite at
        // x=100 (would beat BG0 there by priority). Both cost 62 (64-px
        // wide, on-screen x) -- identical to the transparent fillers ahead
        // of them -- so the documented 1210-budget cutoff at OAM index 19
        // (`oam_budget.rs`) applies uniformly across the whole array: 19
        // fillers exhaust the budget before either late entry is reached,
        // 17 fillers leave both comfortably inside it.
        let (bg0_tiles, bg0_palette, bg0_map) = opaque_bg_fixture(9);
        let bg0 = crate::bg::BgLayer::new(&bg0_tiles, &bg0_palette, &bg0_map);
        let (bg1_tiles, bg1_palette, bg1_map) = opaque_bg_fixture(4);
        let bg1 = crate::bg::BgLayer::new(&bg1_tiles, &bg1_palette, &bg1_map);
        let slots = [
            BgSlot::new(bg0, 0, 3, 0, 0, true), // worst priority: the "floor"
            BgSlot::new(bg1, 1, 0, 0, 0, true), // only ever shown via the OBJWIN mask
        ];

        let mut two_tiles = [0u8; 64];
        two_tiles[..32].copy_from_slice(&[0xFFu8; 32]); // tile 0: opaque (index 15)
        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(31, 31, 31); // white
        let sprite_palette = Palette::new(sprite_colors);

        let wide_64 = |x_raw: u16, tile: u16| {
            OamEntry::new(
                x_raw,
                0,
                tile,
                0,
                BitDepth::Bpp4,
                false,
                false,
                ObjShape::Square,
                3, // 64x64
                0,
                true,
            )
        };

        let mut obj_bg1_only = WindowLayerEnable::NONE;
        obj_bg1_only.bg[1] = true;
        let mut winout = WindowLayerEnable::NONE;
        winout.bg[0] = true;
        winout.obj = true;
        let effects = FrameEffects {
            windows: WindowConfig {
                win0: None,
                win1: None,
                obj_window: Some(obj_bg1_only),
                winout,
            },
            ..FrameEffects::default()
        };

        let build_entries = |filler_count: usize| -> Vec<OamEntry> {
            let mut entries = vec![wide_64(0, 1); filler_count]; // transparent fillers
            entries.push(wide_64(0, 0).with_mode(ObjMode::Window)); // mask sprite, x=0
            entries.push(wide_64(100, 0)); // visible sprite, x=100, priority 0
            entries
        };

        // Exhausted: 19 fillers push both late entries past the budget.
        let exhausted_entries = build_entries(19);
        let exhausted_sprites = SpriteLayer::new(
            &exhausted_entries,
            &sprite_tileset,
            &sprite_tileset,
            &sprite_palette,
        );
        let exhausted_fb = compose_frame_with_effects(&exhausted_sprites, &slots, &effects);
        assert_eq!(
            exhausted_fb.pixel(0, 0),
            Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
            "the mask sprite is dropped -> BG1 stays hidden, BG0 (winout) shows through"
        );
        assert_eq!(
            exhausted_fb.pixel(100, 0),
            Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
            "the visible sprite is dropped -> BG0 shows through instead"
        );

        // Admitted: 17 fillers leave both late entries inside the budget.
        let admitted_entries = build_entries(17);
        let admitted_sprites = SpriteLayer::new(
            &admitted_entries,
            &sprite_tileset,
            &sprite_tileset,
            &sprite_palette,
        );
        let admitted_fb = compose_frame_with_effects(&admitted_sprites, &slots, &effects);
        assert_eq!(
            admitted_fb.pixel(0, 0),
            Some(Bgr555::from_channels(4, 0, 0).to_rgb888()),
            "the mask sprite is admitted -> BG1 shows through the OBJWIN mask"
        );
        assert_eq!(
            admitted_fb.pixel(100, 0),
            Some(Bgr555::from_channels(31, 31, 31).to_rgb888()),
            "the visible sprite is admitted -> its own color beats BG0 by priority"
        );
    }

    #[test]
    fn composing_a_frame_walks_oam_once_per_scanline() {
        // The per-scanline OAM admission stage is consulted by both
        // `SpriteLayer::resolve_pixel_with_mosaic` and
        // `objwin_mask_with_mosaic`, both of which `compose_pixel` calls per
        // pixel. `SpriteLayer`'s one-slot per-scanline cache is what keeps
        // that at one walk per row (160 a frame) instead of one per pixel
        // per path (up to 76,800) -- and both paths reading that one slot is
        // what makes it structurally impossible for them to disagree.
        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
        let sprite_palette = Palette::new(sprite_colors);
        let entries = vec![
            OamEntry::new(
                0,
                0,
                0,
                0,
                BitDepth::Bpp4,
                false,
                false,
                ObjShape::Square,
                3, // 64x64
                0,
                true,
            ),
            OamEntry::new(
                0,
                0,
                0,
                0,
                BitDepth::Bpp4,
                false,
                false,
                ObjShape::Square,
                3,
                0,
                true,
            )
            .with_mode(ObjMode::Window),
        ];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        crate::oam_budget::reset_walk_count();
        let _ = compose_frame(&sprites, &[]);
        assert_eq!(
            crate::oam_budget::walk_count(),
            crate::framebuffer::Framebuffer::HEIGHT,
            "one OAM walk per scanline, not per pixel"
        );

        // Enabling OBJWIN adds a second per-pixel consumer of the admission
        // stage; the walk count must not move.
        let mut winout = WindowLayerEnable::NONE;
        winout.obj = true;
        let effects = FrameEffects {
            windows: WindowConfig {
                win0: None,
                win1: None,
                obj_window: Some(WindowLayerEnable::NONE),
                winout,
            },
            ..FrameEffects::default()
        };
        crate::oam_budget::reset_walk_count();
        let _ = compose_frame_with_effects(&sprites, &[], &effects);
        assert_eq!(
            crate::oam_budget::walk_count(),
            crate::framebuffer::Framebuffer::HEIGHT,
            "the OBJWIN mask path shares the visible path's cached admission"
        );
    }

    #[test]
    fn objwin_sprite_hole_must_not_upgrade_obj_order_inside_win0() {
        // mgba drops an OBJWIN sprite outright inside WIN0/WIN1 (software-obj.c:161,
        // video-software.c:131-134), so its hole cannot promote a worse-priority OBJ
        // there, but still does so in WINOUT.
        let (tiles, palette, map) = opaque_bg_fixture(9); // red BG0
        let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
        let slots = [BgSlot::new(layer, 0, 1, 0, 0, true)]; // BG0 at priority 1

        // Tile 0: solid index 15 (the normal sprite). Tile 1: columns 0..4
        // transparent, columns 4..8 index 14 (the OBJWIN mask sprite).
        let mut tile_bytes = [0xFFu8; 64];
        for row in tile_bytes[32..].chunks_exact_mut(4) {
            row.copy_from_slice(&[0x00, 0x00, 0xEE, 0xEE]);
        }
        let sprite_tiles = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 9, 0); // green: the normal sprite
        sprite_colors[14] = Bgr555::from_channels(0, 31, 31); // must never render
        let sprite_palette = Palette::new(sprite_colors);
        let entries = [
            // OAM 0: normal sprite, worst priority, opaque everywhere.
            OamEntry::new(
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
            ),
            // OAM 1: OBJWIN sprite, best priority, transparent over x < 4.
            OamEntry::new(
                0,
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
            .with_mode(ObjMode::Window),
        ];
        let sprites = SpriteLayer::new(&entries, &sprite_tiles, &sprite_tiles, &sprite_palette);

        let mut bg0_and_obj = WindowLayerEnable::NONE;
        bg0_and_obj.bg[0] = true;
        bg0_and_obj.obj = true;
        let effects = FrameEffects {
            windows: WindowConfig {
                // WIN0 covers x < 2 only; x = 3 falls through to WINOUT.
                win0: Some((
                    WindowRect::new(WindowRange::new(0, 2), WindowRange::new(0, 8)),
                    bg0_and_obj,
                )),
                win1: None,
                obj_window: Some(bg0_and_obj),
                winout: bg0_and_obj,
            },
            ..FrameEffects::default()
        };

        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(3, 0),
            Some(Bgr555::from_channels(0, 9, 0).to_rgb888()),
            "WINOUT: the OBJWIN hole is drawn, promoting the priority-3 OBJ ahead of BG0"
        );
        assert_eq!(
            fb.pixel(1, 0),
            Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
            "WIN0 outranks OBJWIN, so the OBJWIN sprite is skipped there and the \
             OBJ stays at priority 3, behind BG0's priority 1"
        );
    }
}
