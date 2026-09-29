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
/// per the module docs' ordering. A sprite's `layer_rank` is
/// [`SPRITE_LAYER_RANK`]; a BG's is [`bg_layer_rank`].
type OrderKey = (u8, u8);

/// [`OrderKey`]'s sprite `layer_rank`: a sprite always ranks ahead of every
/// BG at equal priority (module docs).
const SPRITE_LAYER_RANK: u8 = 0;

/// [`OrderKey`]'s BG `layer_rank` for `bg_index`: strictly behind
/// [`SPRITE_LAYER_RANK`] at equal priority, and ordered by `bg_index` among
/// BGs (module docs).
const fn bg_layer_rank(bg_index: u8) -> u8 {
    1 + bg_index
}

/// `(order, color, kind, forced_alpha, color_semi_transparent)` — the last
/// two fields mirror [`SpritePixel`]'s
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
                (pixel.priority, SPRITE_LAYER_RANK),
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
                (slot.priority, bg_layer_rank(slot.bg_index)),
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
mod tests;
