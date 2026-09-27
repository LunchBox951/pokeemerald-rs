//! The cross-layer priority compositor: composites up to four BG layers
//! (regular or affine) and one sprite layer into a [`Framebuffer`], with
//! hardware windows, color special effects, and mosaic available through
//! [`compose_frame_with_effects`].
//!
//! Priority ordering, verified against
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

use crate::affine::AffineMatrix;
use crate::bg::BgLayer;
use crate::bg_affine::{AffineBgLayer, AffineMosaicHold, Overflow};
use crate::effects::{self, EffectsConfig, LayerKind};
use crate::framebuffer::Framebuffer;
use crate::mosaic::{MosaicConfig, MosaicSize};
use crate::palette::Rgb888;
use crate::sprite::{SpriteLayer, SpritePixel, WindowSpans};
use crate::window::{WindowConfig, WindowLayerEnable, WindowRegion};

/// A BG slot's per-pixel sampling mode: regular (scrolling) or affine.
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

/// Up to four of these compose into one frame: a regular or affine BG layer
/// plus the per-frame register state that governs its priority and sampling.
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

    /// Build an affine BG slot. `bg_index` and `priority` are masked exactly
    /// as in [`new`](Self::new). `ref_x`/`ref_y` are the frame's latched
    /// reference point in 20.8 fixed point (see [`crate::bg_affine`]'s module
    /// docs for why one static reference point per frame is the
    /// behaviorally-correct model of how pokeemerald drives `BG2X`/`BG2Y`).
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

    /// Return a copy of this slot with its mosaic bit (`BGxCNT`'s mosaic bit)
    /// replaced; defaults to `false`.
    #[must_use]
    pub const fn with_mosaic(mut self, mosaic: bool) -> Self {
        self.mosaic = mosaic;
        self
    }

    /// Sample this slot's resolved color at `(x, y)`, dispatching to the
    /// regular or affine layer per [`BgKind`]. When this slot's mosaic bit is
    /// set, `(x, y)` is first snapped to `bg_mosaic`'s block origin
    /// (`crate::mosaic`); [`MosaicSize::NONE`] makes this a no-op.
    ///
    /// `bg_open` is whether [`crate::window`] currently permits this slot's
    /// BG index to *composite* at `(x, y)`; `hold_active` is whether its
    /// affine mosaic hold should keep advancing regardless of that — see
    /// [`AffineMosaicHold`]'s docs for why the two can differ. The caller
    /// always passes both, rather than skipping the call when closed, so a
    /// `Some` `affine_mosaic_hold` is told about every column; only the
    /// returned color, not that bookkeeping, is gated on `bg_open`.
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

    /// Whether this slot is a `Wrap`-overflow affine layer at a horizontal
    /// mosaic size mGBA bypasses entirely — the same decoded-size gate
    /// [`AffineBgLayer::sample_column_with_mosaic_hold`] applies for
    /// `Overflow::Transparent`, but only `Wrap` needs it checked here since it
    /// never reaches that function's retry/hold path. See that function's
    /// docs for the mGBA derivation.
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
    /// [`AffineBgLayer::sample_column_with_mosaic_hold`] for why.
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

/// A candidate's `(priority, layer_rank)` sort key (lower sorts in front),
/// per the module docs' ordering. A sprite's `layer_rank` is `0`; a BG's is
/// `1 + bg_index`.
type OrderKey = (u8, u8);
/// `(order, color, kind, forced_alpha, color_semi_transparent)` — the last
/// two fields mirror [`SpritePixel`](crate::sprite::SpritePixel)'s
/// `semi_transparent`/`color_semi_transparent` split and are always `false`
/// for a BG candidate.
type Candidate = (OrderKey, Rgb888, LayerKind, bool, bool);

/// Insert a layer into the two frontmost candidates for one pixel.
///
/// Candidates arrive in a fixed order (sprite, then BG slots in slot order),
/// so a strict `<` comparison keeps the first-seen candidate as the
/// tie-break for duplicate order keys.
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
/// Behaviorally identical to calling [`compose_frame_with_effects`] with
/// [`FrameEffects::default()`]: every window, color-effect, and mosaic
/// feature disabled and the default black backdrop `(behavioral-fidelity)`.
#[must_use]
pub fn compose_frame(sprites: &SpriteLayer<'_>, bg_slots: &[BgSlot<'_>]) -> Framebuffer {
    compose_frame_with_effects(sprites, bg_slots, &FrameEffects::default())
}

/// Bundled optional per-frame effects for [`compose_frame_with_effects`]:
/// hardware windows, color special effects, and mosaic.
///
/// [`Default`] disables every one of them (no active window, no color
/// effect, no mosaic, black backdrop) — see [`compose_frame`] for what that
/// makes it equivalent to.
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
/// effects, and mosaic.
///
/// Per pixel: composes every enabled BG's and (if not window-masked) the
/// sprite's opaque contribution by the module docs' priority ordering, then
/// resolves the front layer through [`effects::resolve_pixel_color`] against
/// the next layer or the backdrop — see that function's docs for which
/// second target is eligible.
#[must_use]
pub fn compose_frame_with_effects(
    sprites: &SpriteLayer<'_>,
    bg_slots: &[BgSlot<'_>],
    effects: &FrameEffects,
) -> Framebuffer {
    // mgba's global "any target2" signal (software-obj.c:181-185): the
    // backdrop target2 bit, OR any BG that is a BLDCNT target2 *and* enabled.
    // See effects::resolve_pixel_color's docs for what this drives. It is a
    // per-frame constant, so compute it once rather than per pixel.
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
        let (span_starts, span_draws_obj, span_suppresses_objwin) = {
            let scanline = y as u8;
            let starts = effects.windows.scanline_span_starts(scanline);
            let draws_obj: Vec<bool> = starts
                .iter()
                .map(|&start| span_runs_obj_pass(&effects.windows, start as u8, scanline))
                .collect();
            // A span's own WIN0/WIN1-vs-OBJWIN rank, ignoring the per-pixel
            // OBJWIN mask: mGBA drops an OBJWIN sprite for a whole pass when
            // that pass's own rank outranks OBJWIN's (`software-obj.c:161`,
            // `video-software.c:131-134`) `(behavioral-fidelity)`.
            let suppresses_objwin: Vec<bool> = starts
                .iter()
                .map(|&start| {
                    effects
                        .windows
                        .classify_with_region(start as u8, scanline, false)
                        .1
                        .suppresses_objwin_hole()
                })
                .collect();
            (starts, draws_obj, suppresses_objwin)
        };
        let window_spans = WindowSpans::new(&span_starts, &span_draws_obj, &span_suppresses_objwin);
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
                window_spans,
                x,
                y,
            );
            framebuffer.set_pixel(x, y, color);
        }
    }
    framebuffer
}

/// Whether the hardware-window span starting at column `start` runs the OBJ
/// pass on scanline `y` at all.
///
/// mGBA skips a span's whole sprite-preprocessing pass unless that span's own
/// control enables OBJ or `OBJWIN` is enabled in `DISPCNT`; a skipped span
/// writes nothing into the once-per-scanline sprite buffer
/// (`mgba/src/gba/renderers/video-software.c:1052-1062`). A span is one run of
/// a single window region, so its start column classifies all of it, and an
/// `OBJWIN` mask never partitions the scanline `(behavioral-fidelity)`.
fn span_runs_obj_pass(windows: &WindowConfig, start: u8, y: u8) -> bool {
    windows.classify(start, y, false).obj || windows.obj_window.is_some()
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
/// `window_spans` carries this scanline's hardware-window spans and which of
/// them run the OBJ pass at all, for
/// [`SpriteLayer::sample_affine_local`](crate::sprite::SpriteLayer)'s affine
/// mosaic hold restart.
#[allow(clippy::cast_possible_truncation)] // Framebuffer coordinates are always < 240/160, well within u8.
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
    window_spans: WindowSpans<'_>,
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
    // The backdrop's brighten/darken variant is chosen from that same
    // OBJWIN-independent span, not from `window.effects` below
    // (`crate::effects::backdrop_variant`) `(behavioral-fidelity)`.
    let span_backdrop =
        effects::backdrop_variant(&effects.color, partition_control.effects, effects.backdrop);
    let sprite = window
        .obj
        .then(|| sprites.resolve_pixel_with_mosaic_windowed(x, y, effects.mosaic.obj, window_spans))
        .flatten();
    let window_effects = pixel_window_effects(
        &effects.windows,
        sprite,
        (window, region),
        partition_control,
        wy,
    );

    let mut front = None;
    let mut next = None;

    if let Some(pixel) = sprite {
        insert_candidate(
            &mut front,
            &mut next,
            (
                (pixel.priority, 0),
                pixel.color,
                LayerKind::Obj,
                pixel.semi_transparent,
                pixel.color_semi_transparent,
            ),
        );
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
                false,
            ),
        );
    }

    let Some((_, front_color, front_kind, front_semi_transparent, front_color_semi_transparent)) =
        front
    else {
        // Nothing drawn: the backdrop itself is shown, already resolved to
        // its span variant (effects::resolve_pixel_color never alpha-blends
        // the backdrop against itself).
        return span_backdrop;
    };
    let next = next.map(|(_, color, kind, _, _)| (color, kind));
    effects::resolve_pixel_color(
        &effects.color,
        window_effects,
        any_target2,
        (
            front_color,
            front_kind,
            front_semi_transparent,
            front_color_semi_transparent,
        ),
        next,
        span_backdrop,
    )
}

/// Resolves [`compose_pixel`]'s [`effects::PixelWindowEffects`] from the
/// pixel's `OBJWIN`-mask-resolved `window` and `region`, its
/// `OBJWIN`-independent `partition_control` span, and the resolved `sprite`.
///
/// mGBA bakes an OBJ's variant color and target-1/reblend flags from the span
/// whose pass writes the sprite buffer, and a later span only composites what
/// is stored (`mgba/src/gba/renderers/software-obj.c:159,176-208,424-430`).
/// That writer is the column's own span except for an affine mosaic trailing
/// spill (`software-obj.c:227-242`), so every OBJ-only signal comes from the
/// writing span that [`SpritePixel`] records `(behavioral-fidelity)`.
fn pixel_window_effects(
    windows: &WindowConfig,
    sprite: Option<SpritePixel>,
    (window, region): (WindowLayerEnable, WindowRegion),
    partition_control: WindowLayerEnable,
    y: u8,
) -> effects::PixelWindowEffects {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a span start is a framebuffer column, below 240"
    )]
    let span_effects = |span_start: usize| windows.classify(span_start as u8, y, false).effects;
    let (flags_span_effects, color_span_effects) = sprite.map_or(
        (partition_control.effects, partition_control.effects),
        |pixel| {
            (
                span_effects(pixel.span_start),
                span_effects(pixel.color_span_start),
            )
        },
    );
    // mGBA's `objwinSlowPath` (`effects::resolve_pixel_color`'s docs):
    // `OBJWIN`'s own blend-enable bit compared against the writing pass's
    // `WIN0`/`WIN1`/`WINOUT` span, never the OBJWIN-mask-resolved `window` --
    // sprites are only ever preprocessed against such a span
    // (`mgba/src/gba/renderers/video-software.c:1052-1055`)
    // `(behavioral-fidelity)`.
    let objwin_slow_path = |writer_effects: bool| {
        windows
            .obj_window
            .is_some_and(|enable| enable.effects != writer_effects)
    };
    effects::PixelWindowEffects {
        enabled: window.effects,
        static_span_enabled: flags_span_effects,
        objwin_slow_path: objwin_slow_path(flags_span_effects),
        // Where `OBJWIN` resolves this pixel, mGBA draws from
        // `objwinPalette`, the variant only when `OBJWIN` also enables
        // effects (`software-obj.c:206-208`).
        color_variant_enabled: color_span_effects
            && (region != WindowRegion::ObjWindow || window.effects),
        color_objwin_slow_path: objwin_slow_path(color_span_effects),
    }
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
        let (tiles_x, palette_x, map_x) = opaque_bg_fixture(1);
        let (tiles_y, palette_y, map_y) = opaque_bg_fixture(2);
        let layer_a = crate::bg::BgLayer::new(&tiles_x, &palette_x, &map_x);
        let layer_b = crate::bg::BgLayer::new(&tiles_y, &palette_y, &map_y);

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
        let slots = [BgSlot::new(bg_layer, 0, 0, 0, 0, true)];

        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
        let sprite_palette = Palette::new(sprite_colors);
        // The sprite's priority is worse than the BG's, so the BG must win.
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

        // 0x00 fills every texel with palette index 0 (transparent).
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
        // B (OAM index 0, opaque, priority 2) sits under A (OAM index 1,
        // transparent, priority 0), with the BG between them at priority 1.
        // On hardware, A's transparent texel upgrades B's already-stored OBJ
        // order to priority 0, so the OBJ layer (still B's color) beats the
        // BG even though B's own priority (2) is worse than the BG's (1)
        // (`mgba/src/gba/renderers/software-obj.c:76-85,116-125`).
        let (ts, pal, tm) = opaque_bg_fixture(7);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)];

        // Tile 0 is opaque (drawn by B); tile 1 is transparent (drawn by A).
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

    /// A single affine BG tile whose first row (y=0) has a distinct opaque
    /// color per column (channel `column + 1`) -- distinguishes "holds its
    /// own column's texel" from "holds a neighbor's" at column granularity,
    /// unlike [`opaque_affine_bg_fixture`]'s single flat color.
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
        // Affine slots share compose_frame's ordering, not a separate path.
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

    fn bpp4_tile_with_top_left_2x2(
        palette_indices_by_row: [[u8; 2]; 2],
    ) -> [u8; BitDepth::Bpp4.tile_byte_len()] {
        const BYTES_PER_ROW: usize = BitDepth::TILE_DIM / 2;
        let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len()];
        for (row, [left, right]) in palette_indices_by_row.into_iter().enumerate() {
            bytes[row * BYTES_PER_ROW] = (right << 4) | left;
        }
        bytes
    }

    fn bpp4_row(
        palette_indices_by_column: [u8; BitDepth::TILE_DIM],
    ) -> [u8; BitDepth::TILE_DIM / 2] {
        let mut row = [0u8; BitDepth::TILE_DIM / 2];
        for (byte, [left, right]) in row
            .iter_mut()
            .zip(palette_indices_by_column.as_chunks::<2>().0)
        {
            *byte = (right << 4) | left;
        }
        row
    }

    fn bpp4_tile_with_every_row(
        palette_indices_by_column: [u8; BitDepth::TILE_DIM],
    ) -> [u8; BitDepth::Bpp4.tile_byte_len()] {
        let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len()];
        for row in bytes.chunks_exact_mut(BitDepth::TILE_DIM / 2) {
            row.copy_from_slice(&bpp4_row(palette_indices_by_column));
        }
        bytes
    }

    fn quadrant_bg_fixture() -> (Tileset, Palette, Tilemap) {
        let bytes = bpp4_tile_with_top_left_2x2([[1, 2], [3, 4]]);
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
        let (tiles_r, palette_r, map_r) = opaque_bg_fixture(9);
        let (tiles_b, _palette_b, map_b) = opaque_bg_fixture(0);
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
        let (tiles, palette, map) = opaque_bg_fixture(9);
        let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
        let slots = [BgSlot::new(layer, 0, 0, 0, 0, true)];

        let mask_tile = bpp4_tile_with_every_row([0, 0, 0, 0, 15, 15, 15, 15]);
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
        // mGBA's out-of-bounds mode-2 mosaic fetch leaves `mosaicWait`
        // unchanged, so the next column's retry seeds the hold instead
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
        // mGBA restarts the mode-2 draw routine once per hardware-window
        // region, re-seeding its mosaic hold from that region's own snapped
        // block origin rather than resuming the pre-gap hold
        // (`mgba/src/gba/renderers/video-software.c:628-675`,
        // `software-private.h:173-192`, `software-bg.c:66-73`)
        // `(behavioral-fidelity)`.
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
        // mGBA restarts BG drawing per hardware-window region and never
        // coalesces adjacent regions sharing the same control bits
        // (`mgba/src/gba/renderers/video-software.c:458-505,628-675`)
        // `(behavioral-fidelity)`.
        let (tiles, palette, tilemap) = eight_tile_gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let scale = 8 * AffineMatrix::ONE;
        let matrix = AffineMatrix::new(scale, 0, 0, AffineMatrix::ONE);
        let reference_x = -24 * i32::from(AffineMatrix::ONE);
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
        // mGBA's `_breakWindowInner` still splits the scanline around a
        // zero-width WIN0, resetting the mosaic hold at the split
        // (`mgba/src/gba/renderers/video-software.c:458-496,628-675`)
        // `(behavioral-fidelity)`.
        let (tiles, palette, tilemap) = eight_tile_gradient_affine_bg_fixture();
        let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
        let scale = 8 * AffineMatrix::ONE;
        let matrix = AffineMatrix::new(scale, 0, 0, AffineMatrix::ONE);
        let reference_x = -24 * i32::from(AffineMatrix::ONE);
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

        let mask_tile = bpp4_tile_with_every_row([0, 0, 0, 0, 15, 15, 15, 15]);
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

        let mask_tile = bpp4_tile_with_every_row([0, 0, 0, 0, 15, 15, 0, 0]);
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
                obj_window: Some(WindowLayerEnable::NONE),
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

        let mask_tile = bpp4_tile_with_every_row([0, 0, 0, 0, 15, 15, 0, 0]);
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
                winout: WindowLayerEnable::NONE,
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
        let sprites = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

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
        let sprites = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

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
        let sprites = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

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
        let sprites = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

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

    #[test]
    fn alpha_blend_end_to_end_over_two_bg_layers() {
        // eva=evb=8 (50/50) blend of r channels 0 and 255: (0*8+255*8)/16 = 127.
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
        // OBJ is not configured target1 here, but a semi-transparent OBJ
        // still forces alpha blend regardless of BLDCNT's selected effect
        // (OamEntry::with_mode's contract).
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
        // eva=evb=8 blend of the sprite's r=0 and the BG's r=255:
        // (0*8+255*8)/16 = 127.
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
        // eva=evb=8 blend of black (0,0,0) and white (255,255,255) per
        // channel: (0*8+255*8)/16 = 127 on every channel.
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
        // Opaque sprite B (priority 2) sits under a priority-0 OBJWIN-mode
        // sprite whose texel here is a transparent hole, with a BG between
        // them at priority 1. Per SpritePixel's flag-only-overwrite contract,
        // that transparent texel upgrades B's stored priority to 0 without
        // replacing its color, so the OBJ layer beats the BG despite B's own
        // priority (2) being worse than the BG's (1).
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
    fn transparent_semi_transparent_obj_reblends_a_normal_objs_retained_variant_color() {
        // A transparent, better-priority semi-transparent OBJ promotes
        // priority over an already-brightened worse-priority Normal OBJ
        // without replacing its color (SpritePixel::color_semi_transparent).
        // mGBA re-brightens that surviving color again in its reblend
        // postpass (`video-software.c:982-1013`), so this crate must double-
        // brighten it too.
        let (tiles_a, palette_a, map_a) = opaque_bg_fixture(1); // BG0: priority 1, not target2
        let (tiles_b, palette_b, map_b) = opaque_bg_fixture(2); // BG1: priority 2, target2
        let layer_a = crate::bg::BgLayer::new(&tiles_a, &palette_a, &map_a);
        let layer_b = crate::bg::BgLayer::new(&tiles_b, &palette_b, &map_b);
        let slots = [
            BgSlot::new(layer_a, 0, 1, 0, 0, true),
            BgSlot::new(layer_b, 1, 2, 0, 0, true),
        ];

        // Tile 0: opaque everywhere at palette index 15 (the worse-priority
        // Normal OBJ). Tile 1: fully transparent (the better-priority
        // semi-transparent OBJ, transparent at (0, 0)).
        let mut two_tiles = [0u8; 64];
        two_tiles[..32].fill(0xFF);
        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(8, 4, 2);
        let sprite_palette = Palette::new(sprite_colors);
        let worse_priority_opaque_normal = OamEntry::new(
            0,
            0,
            0, // tile 0 (opaque)
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            2, // worse priority
            true,
        );
        let better_priority_transparent_semi = OamEntry::new(
            0,
            0,
            1, // tile 1 (transparent)
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0, // better priority
            true,
        )
        .with_mode(ObjMode::SemiTransparent);
        let entries = [
            worse_priority_opaque_normal,
            better_priority_transparent_semi,
        ];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let effects = FrameEffects {
            color: EffectsConfig {
                effect: ColorEffect::Brighten,
                target1: LayerTargets {
                    bg: [false; 4],
                    obj: true,
                    backdrop: false,
                },
                target2: LayerTargets {
                    bg: [false, true, false, false],
                    obj: false,
                    backdrop: false,
                },
                eva: 8,
                evb: 8,
                evy: 8,
            },
            ..FrameEffects::default()
        };

        let fb = compose_frame_with_effects(&sprites, &slots, &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Rgb888 {
                r: 207,
                g: 199,
                b: 195,
            }),
            "the retained Normal OBJ variant color is brightened again by the reblend postpass"
        );
    }

    #[test]
    fn semi_transparent_obj_reblend_brightens_with_a_deeper_enabled_target2_bg() {
        // A semi-transparent OBJ (forced alpha) sits over BG_a
        // (priority 1, its immediate neighbour, NOT a target2) with BG_b
        // (priority 2, a target2) enabled deeper in the frame. See
        // effects::resolve_pixel_color's contract for why a global target2
        // still brightens this surviving pixel. The control drops BG_b's
        // target2 bit -> no target2 anywhere -> brightened either way.
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
            Some(Bgr555::from_channels(31, 31, 31).to_rgb888()),
            "a deeper enabled target2 BG clears the variant, but the surviving reblend OBJ is postprocessed to white"
        );

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

    #[test]
    fn compositor_agrees_between_the_objwin_mask_and_visible_sprite_layer_under_exhaustion() {
        // Both late entries cost 62 (64px wide, on-screen), identical to the
        // filler sprites ahead of them, so the oam_budget cutoff at OAM index
        // 19 (`oam_budget.rs`) applies uniformly: 19 fillers exhaust it
        // before either late entry is reached, 17 leave both inside it. The
        // OBJWIN mask and the visible OBJ layer both read that one cached
        // admission decision (`crate::oam_budget`), so they move together.
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
        sprite_colors[15] = Bgr555::from_channels(31, 31, 31);
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

    #[test]
    fn objwin_mask_never_changes_the_uncovered_backdrop_variant() {
        // The backdrop variant is chosen once per static span
        // (`effects::backdrop_variant`), not per OBJWIN-masked pixel: WINOUT
        // enables effects here while OBJWIN doesn't, so both columns brighten.
        let mut mask_tile = [0u8; 32];
        for row in mask_tile.chunks_exact_mut(4) {
            row.copy_from_slice(&[0x00, 0x00, 0xFF, 0xFF]); // columns 4..8 opaque
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

        // WINOUT enables color effects; OBJWIN enables nothing at all.
        let mut winout = WindowLayerEnable::NONE;
        winout.effects = true;

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: None,
                win1: None,
                obj_window: Some(WindowLayerEnable::NONE),
                winout,
            },
            color: EffectsConfig {
                effect: ColorEffect::Brighten,
                target1: LayerTargets {
                    bg: [false; 4],
                    obj: false,
                    backdrop: true,
                },
                target2: LayerTargets::default(),
                eva: 0,
                evb: 0,
                evy: 16,
            },
            backdrop: Rgb888::BLACK,
            ..FrameEffects::default()
        };

        let white = Bgr555::from_channels(31, 31, 31).to_rgb888();
        let fb = compose_frame_with_effects(&sprites, &[], &effects);
        assert_eq!(
            fb.pixel(1, 0),
            Some(white),
            "outside the OBJWIN mask the WINOUT span brightens the backdrop"
        );
        assert_eq!(
            fb.pixel(5, 0),
            Some(white),
            "the OBJWIN mask must not re-select the backdrop variant"
        );
    }

    #[test]
    fn objwin_slow_path_reblends_a_normal_obj_that_is_not_a_target1_layer() {
        // OBJWIN enabled with a blend-enable bit that differs from WINOUT's
        // own triggers mGBA's `objwinSlowPath` (`software-obj.c:176,180-192`),
        // which reblends a plain Normal-mode OBJ even though BLDCNT never
        // marks it as a target1 layer.
        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(31, 31, 31);
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
        )]; // Normal mode (default), never a target1 OBJ below
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let mut winout = WindowLayerEnable::NONE;
        winout.obj = true;
        winout.effects = true;

        let color = EffectsConfig {
            effect: ColorEffect::Darken,
            target1: LayerTargets::default(), // deliberately excludes LayerKind::Obj
            target2: LayerTargets {
                bg: [false; 4],
                obj: false,
                backdrop: true,
            },
            eva: 0,
            evb: 0,
            evy: 16,
        };

        let effects = FrameEffects {
            windows: WindowConfig {
                win0: None,
                win1: None,
                obj_window: Some(WindowLayerEnable::NONE), // OBJWIN blend bit off, WINOUT's is on
                winout,
            },
            color,
            backdrop: Rgb888::BLACK,
            ..FrameEffects::default()
        };

        let fb = compose_frame_with_effects(&sprites, &[], &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Rgb888::BLACK),
            "objwin_slow_path reblends the Normal OBJ even though it is not a target1 layer"
        );

        // Control: OBJWIN's own blend-enable bit now matches WINOUT's, so
        // objwin_slow_path is false -- the non-target1 Normal OBJ is never a
        // reblend candidate and stays raw white.
        let mut matched_obj_window = WindowLayerEnable::NONE;
        matched_obj_window.effects = true;
        let control_effects = FrameEffects {
            windows: WindowConfig {
                obj_window: Some(matched_obj_window),
                ..effects.windows
            },
            ..effects
        };
        let control_fb = compose_frame_with_effects(&sprites, &[], &control_effects);
        let white = Bgr555::from_channels(31, 31, 31).to_rgb888();
        assert_eq!(
            control_fb.pixel(0, 0),
            Some(white),
            "without objwin_slow_path, a non-target1 Normal OBJ is never reblended"
        );
    }

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
}
