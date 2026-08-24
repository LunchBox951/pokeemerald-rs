//! Hardware color special effects: alpha blend, brightness increase/decrease
//! (S-2 slice 4, issue #99; reworked to the 32-bit oracle in issue #380).
//!
//! Ports `BLDCNT`/`BLDALPHA`/`BLDY`'s per-pixel color math, verified against
//! `mgba/include/mgba-util/image.h`'s `mColorMix5Bit` and
//! `mgba/src/gba/renderers/software-private.h`'s `_brighten`/`_darken`.
//!
//! **The oracle is stock desktop mGBA (SDL/Qt), not `COLOR_16_BIT`.** mGBA
//! selects the pixel format these functions operate on at compile time
//! (`mgba/include/mgba-util/image.h:15-20`): `COLOR_16_BIT` makes `mColor` a
//! packed 5-bit-per-channel `uint16_t` and runs the effect math on those 5
//! bits directly; without it (the default), `mColor` is a `uint32_t` and
//! palette colors are first expanded to 8 bits per channel by
//! `mColorFrom555` (`image.h:253-266`, `M_RGB5_TO_BGR8` plus
//! `color |= (color >> 5) & 0x070707`, i.e. `(c5 << 3) | (c5 >> 2)` — the
//! exact formula [`crate::palette::expand_5_to_8`] already uses to build
//! every [`Rgb888`] this crate produces), with `_brighten`/`_darken`/
//! `mColorMix5Bit` then running on those already-8-bit channels
//! (`software-private.h:237-245`, `:270-278`; `image.h:307-327`).
//! `COLOR_16_BIT` is only ever added for the Wii, 3DS, and libretro targets
//! (`mgba/src/platform/wii/CMakeLists.txt:11`,
//! `mgba/src/platform/3ds/CMakeLists.txt:14`, and `mgba/CMakeLists.txt:1026`
//! respectively); a normal desktop SDL or Qt build never defines it and
//! always takes the 32-bit path. `docs/acceptance/v1.md:16-24` binds v1
//! completion to "the observable experience of playing alone on mGBA" — that
//! means stock desktop mGBA (`docs/principles.md`'s `reference-only`), so
//! [`alpha_blend`], [`brighten`], and [`darken`] operate directly on
//! [`Rgb888`]'s already-8-bit-expanded channels with 8-bit saturation,
//! matching the 32-bit path bit for bit, rather than compressing back down
//! to 5 bits first `(behavioral-fidelity)`.
//!
//! Bit for bit carries one asymmetry. All three routines mask each channel
//! *in place* inside the packed `u32` (`& 0xFF`, `& 0xFF00`, `& 0xFF0000`;
//! red is the low lane, `image.h:37-39`) rather than unpacking it, so a
//! shifted lane's mask performs that lane's final shift-down and can
//! truncate a second time. `mColorMix5Bit` and `_brighten` are unaffected —
//! their shifted lanes floor a *sum*, and a floor taken after the lane's
//! shift lands on the same value the unpacked per-channel formula floors to.
//! `_darken` is the exception: its shifted lanes floor a *difference*, and
//! flooring the difference is `c - ceil(c * y / 16)`, one darker than red's
//! unmasked `c - (c * y) / 16` wherever `c * y` is not a multiple of 16.
//! `_darken(0xFFFFFF, 7)` is `(144, 143, 143)`, not a uniform 144. Hence
//! the split into `darken_red_channel` and `darken_shifted_channel` below.
//! All three routines are pinned lane by lane against committed expected-value
//! fixtures. The fixture sweeps cover the expanded channel domain and usable
//! weight range without embedding an executable copy of the reference
//! implementation `(behavioral-fidelity)`.
//!
//! `EVA`/`EVB`/`EVY` are 5-bit register fields (`0..=31`) but hardware caps
//! every one of them at 16 (100%) before using it — `mgba`'s
//! `video-software.c:325-344` clamps `blda`/`bldb`/`bldy` to `0x10` right
//! when the register is written; values `17..=31` behave identically to 16.
//! That capping is independent of the `COLOR_16_BIT` pixel-format choice
//! above.

use crate::palette::Rgb888;

/// The hardware's four `BLDCNT` special-effect modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorEffect {
    /// No color special effect (`BLDCNT` mode `00`).
    #[default]
    None,
    /// Alpha blend the first target with the second (`BLDCNT` mode `01`,
    /// `BLDALPHA`'s `EVA`/`EVB`).
    AlphaBlend,
    /// Brighten (blend toward white) the first target (`BLDCNT` mode `10`,
    /// `BLDY`'s `EVY`).
    Brighten,
    /// Darken (blend toward black) the first target (`BLDCNT` mode `11`,
    /// `BLDY`'s `EVY`).
    Darken,
}

/// Which composited layer produced a pixel, for testing against
/// [`LayerTargets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    /// One of BG0..BG3 (`0..=3`).
    Bg(u8),
    /// The OBJ (sprite) layer.
    Obj,
    /// The backdrop (shown where no enabled, opaque layer covers a pixel).
    Backdrop,
}

/// Which layers count as a color-effect target — `BLDCNT`'s six target-1 (or
/// target-2) bits: BG0..BG3, OBJ, and the backdrop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayerTargets {
    /// Per-BG (BG0..BG3) target bits.
    pub bg: [bool; 4],
    /// Whether OBJ is a target.
    pub obj: bool,
    /// Whether the backdrop is a target.
    pub backdrop: bool,
}

impl LayerTargets {
    /// Whether `kind` is a member of this target set.
    #[must_use]
    pub const fn contains(&self, kind: LayerKind) -> bool {
        match kind {
            LayerKind::Bg(index) => self.bg[(index & 0x03) as usize],
            LayerKind::Obj => self.obj,
            LayerKind::Backdrop => self.backdrop,
        }
    }
}

/// Full per-frame color-effect configuration: `BLDCNT`'s selected effect and
/// target sets, plus `BLDALPHA`'s `EVA`/`EVB` and `BLDY`'s `EVY` raw
/// (uncapped) weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EffectsConfig {
    /// The selected special effect.
    pub effect: ColorEffect,
    /// The first-target ("A") layer set.
    pub target1: LayerTargets,
    /// The second-target ("B") layer set.
    pub target2: LayerTargets,
    /// `BLDALPHA`'s raw `EVA` weight (`0..=31`; capped at 16 when used).
    pub eva: u8,
    /// `BLDALPHA`'s raw `EVB` weight (`0..=31`; capped at 16 when used).
    pub evb: u8,
    /// `BLDY`'s raw `EVY` weight (`0..=31`; capped at 16 when used).
    pub evy: u8,
}

/// Cap a raw 5-bit register weight (`0..=31`) at 16 (100%) — see the module
/// docs.
const fn cap_weight(raw: u8) -> u32 {
    if raw > 16 {
        16
    } else {
        raw as u32
    }
}

/// Mix one 8-bit channel the way the 32-bit `mColorMix5Bit` mixes one byte
/// lane (`image.h:307-327`): `(a*weight_a + b*weight_b) / 16`, saturating at
/// 255 (8-bit max) rather than 31 — mGBA signals the per-channel overflow
/// with a spare 9th bit (`& 0x1FF`, `if (c & 0x100) c = 0xFF`) because its
/// three channels are packed into one `u32`; doing each channel as an
/// independent scalar here makes that the same plain saturating clamp.
/// `weight_a`/`weight_b` are assumed already capped (module docs).
#[allow(clippy::cast_possible_truncation)] // `mixed` is clamped to `0..=255` just above the cast.
const fn mix_channel(a: u8, b: u8, weight_a: u32, weight_b: u32) -> u8 {
    let mixed = (a as u32 * weight_a + b as u32 * weight_b) / 16;
    if mixed > 255 {
        255
    } else {
        mixed as u8
    }
}

/// Alpha-blend `first` (the first target, weighted by `eva`) with `second`
/// (the second target, weighted by `evb`) — `BLDALPHA`'s per-channel
/// `(first*eva + second*evb) / 16`, saturating, both weights capped at 16.
#[must_use]
pub const fn alpha_blend(first: Rgb888, second: Rgb888, eva: u8, evb: u8) -> Rgb888 {
    let wa = cap_weight(eva);
    let wb = cap_weight(evb);
    Rgb888 {
        r: mix_channel(first.r, second.r, wa, wb),
        g: mix_channel(first.g, second.g, wa, wb),
        b: mix_channel(first.b, second.b, wa, wb),
    }
}

/// Brighten one 8-bit channel toward white — the 32-bit `_brighten`
/// (`software-private.h:237-245`): `c + (255-c)*y/16`, `y` already capped.
#[allow(clippy::cast_possible_truncation)] // `r` is provably `<=255` (c<=255, y<=16 => (255-c)*y/16<=255-c, so c+that<=255).
const fn brighten_channel(c: u8, y: u32) -> u8 {
    let c32 = c as u32;
    let r = c32 + ((255 - c32) * y) / 16;
    r as u8
}

/// Brighten `color` toward white by `evy` sixteenths — `BLDY`'s brighten
/// mode (`_brighten`), `evy` capped at 16.
#[must_use]
pub const fn brighten(color: Rgb888, evy: u8) -> Rgb888 {
    let y = cap_weight(evy);
    Rgb888 {
        r: brighten_channel(color.r, y),
        g: brighten_channel(color.g, y),
        b: brighten_channel(color.b, y),
    }
}

/// Darken the *red* channel toward black — the 32-bit `_darken`'s low lane
/// (`software-private.h:271-272`), whose `& 0xFF` mask discards nothing the
/// `/ 16` did not already: `c - floor(c*y/16)`, `y` already capped.
#[allow(clippy::cast_possible_truncation)] // c*y/16 <= c <= 255, so the subtraction result is `<=255` and non-negative.
const fn darken_red_channel(c: u8, y: u32) -> u8 {
    let c32 = c as u32;
    let r = c32 - (c32 * y) / 16;
    r as u8
}

/// Darken a *shifted* (green or blue) channel toward black — the 32-bit
/// `_darken`'s high lanes (`software-private.h:274-278`). There the `/ 16`
/// divides an exact multiple of the lane's shift and the `& 0xFF00` /
/// `& 0xFF0000` mask does the truncating instead, which rounds the amount
/// subtracted *up*: `c - ceil(c*y/16)`, i.e. `c - (c*y + 15)/16` — see the
/// module docs. `y` already capped.
#[allow(clippy::cast_possible_truncation)] // ceil(c*y/16) <= c <= 255 for y <= 16, so the result is `0..=255`.
const fn darken_shifted_channel(c: u8, y: u32) -> u8 {
    let c32 = c as u32;
    let r = c32 - (c32 * y).div_ceil(16);
    r as u8
}

/// Darken `color` toward black by `evy` sixteenths — `BLDY`'s darken mode
/// (`_darken`), `evy` capped at 16. Red rounds differently from green and
/// blue; the module docs explain why.
#[must_use]
pub const fn darken(color: Rgb888, evy: u8) -> Rgb888 {
    let y = cap_weight(evy);
    Rgb888 {
        r: darken_red_channel(color.r, y),
        g: darken_shifted_channel(color.g, y),
        b: darken_shifted_channel(color.b, y),
    }
}

/// Resolve a pixel's final displayed color from its front (topmost) layer,
/// the color-effect configuration, the window's per-pixel effect-enable
/// bit, and (if present) the layer immediately behind the front one plus the
/// backdrop as the ultimate fallback second target.
///
/// Mirrors mgba's per-pixel composite step
/// (`_compositeBlendObjwin`/`_compositeBlendNoObjwin`,
/// `software-private.h`): only the *immediately next* covering layer is ever
/// considered as a second target — mgba clears a pixel's target-1 flag the
/// moment a non-blending write passes through it, so a pixel that fails to
/// blend against its immediate neighbor never falls through to try a third,
/// deeper layer, nor the backdrop, instead `(behavioral-fidelity)`.
///
/// `front.2` (`forced_alpha`) models OBJ semi-transparency (OAM mode 1):
/// mgba's `GBAVideoSoftwareRendererPreprocessSprite` sets a semi-transparent
/// sprite's target-1 flag unconditionally, regardless of the window's
/// blend-enable bit or which effect `BLDCNT` actually selected, so it always
/// alpha-blends against a valid second target behind it — *overriding*
/// brighten/darken/none if that's what was configured
/// `(behavioral-fidelity)`.
///
/// When that forced blend finds no target2 among the *immediate* next layer,
/// mgba does not decide the fallback from that neighbor. `software-obj.c`
/// (177-192) instead consults a **global** signal: it sums whether *any*
/// target2 layer exists in the whole frame — any enabled BG that is a `BLDCNT`
/// target2, or the backdrop target2. If some target2 exists it clears the
/// sprite's brighten/darken `variant` selector and emits the raw color; only
/// when *no* target2 exists anywhere does `variant` survive, which stays on
/// precisely when the OBJ is a `BLDCNT` first target under a brighten/darken
/// effect with effects enabled here. The caller therefore passes that global
/// `any_target2_enabled` signal (which cannot be recovered from the immediate
/// `next` alone): a forced-alpha OBJ whose immediate neighbor is not a target2
/// emits its raw color when any target2 is enabled anywhere, and falls through
/// to the brighten/darken branch only when none is `(behavioral-fidelity)`.
#[must_use]
pub fn resolve_pixel_color(
    cfg: &EffectsConfig,
    effects_enabled: bool,
    any_target2_enabled: bool,
    front: (Rgb888, LayerKind, bool),
    next: Option<(Rgb888, LayerKind)>,
    backdrop: Rgb888,
) -> Rgb888 {
    let (front_color, front_kind, forced_alpha) = front;
    let is_backdrop = matches!(front_kind, LayerKind::Backdrop);

    // The backdrop has nothing behind it, so it can never itself be an
    // alpha-blend front (mgba's backdrop-brighten/darken `forceTarget1` path
    // never does alpha blending — only brighten/darken, handled below).
    let alpha_target1 = !is_backdrop
        && (forced_alpha
            || (effects_enabled
                && cfg.effect == ColorEffect::AlphaBlend
                && cfg.target1.contains(front_kind)));

    if alpha_target1 {
        match next {
            Some((next_color, next_kind)) if cfg.target2.contains(next_kind) => {
                return alpha_blend(front_color, next_color, cfg.eva, cfg.evb);
            }
            None if cfg.target2.backdrop => {
                return alpha_blend(front_color, backdrop, cfg.eva, cfg.evb);
            }
            // No target2 in the *immediate* neighbor. mgba decides the
            // fallback from the global `any_target2_enabled` signal, not this
            // neighbor (software-obj.c:177-192): when any target2 layer exists
            // in the frame it clears the brighten/darken `variant` and emits
            // the raw front color; only with no target2 anywhere does `variant`
            // survive, letting an OBJ that is a BLDCNT first target under a
            // brighten/darken effect still get that effect via the branch
            // below `(behavioral-fidelity)`.
            _ if any_target2_enabled => return front_color,
            _ => {}
        }
    }

    if effects_enabled && cfg.target1.contains(front_kind) {
        match cfg.effect {
            ColorEffect::Brighten => return brighten(front_color, cfg.evy),
            ColorEffect::Darken => return darken(front_color, cfg.evy),
            ColorEffect::None | ColorEffect::AlphaBlend => {}
        }
    }

    front_color
}

#[cfg(test)]
mod tests {
    use super::{
        alpha_blend, brighten, darken, resolve_pixel_color, ColorEffect, EffectsConfig, LayerKind,
        LayerTargets,
    };
    use crate::palette::Rgb888;

    #[test]
    fn alpha_blend_hand_computed_50_50() {
        // eva=8, evb=8, operating on the already-8-bit channels (module
        // docs' 32-bit oracle): (a*8+b*8)/16 == (a+b)/2 with truncating
        // integer division. 0 and 255: (0*8+255*8)/16 = 2040/16 = 127
        // (127.5 truncated toward zero, matching mgba's plain C `/`).
        let first = Rgb888 { r: 0, g: 0, b: 0 };
        let second = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        let blended = alpha_blend(first, second, 8, 8);
        assert_eq!(blended.r, 127);
    }

    #[test]
    fn alpha_blend_full_weight_on_first_is_identity() {
        // Use real palette-derived colors (not arbitrary 8-bit values) so
        // this exercises the same channel bytes the compositor ever
        // produces -- every one of them already 8-bit-expanded via
        // `Bgr555::to_rgb888`.
        let first = crate::palette::Bgr555::from_channels(12, 20, 31).to_rgb888();
        let second = crate::palette::Bgr555::from_channels(1, 2, 3).to_rgb888();
        assert_eq!(alpha_blend(first, second, 16, 0), first);
    }

    #[test]
    fn alpha_blend_saturates_when_weights_overflow() {
        // eva=16, evb=16 (both capped at 16, not their raw values if
        // larger): (255*16 + 255*16)/16 = 510, saturating to 255 (8-bit
        // max, not 31 -- module docs' 32-bit oracle) for a full white
        // channel.
        let first = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        let second = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        assert_eq!(alpha_blend(first, second, 31, 31), first);
    }

    #[test]
    fn alpha_blend_weights_above_16_behave_like_16() {
        let first = Rgb888 { r: 0, g: 0, b: 0 };
        let second = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        assert_eq!(
            alpha_blend(first, second, 20, 0),
            alpha_blend(first, second, 16, 0)
        );
    }

    #[test]
    fn brighten_ramps_toward_white() {
        let color = Rgb888 { r: 0, g: 0, b: 0 };
        // evy=16 (full): 0 + (255-0)*16/16 = 255 -> channel 255.
        assert_eq!(brighten(color, 16).r, 255);
        // evy=8 (half), 8-bit oracle: 0 + (255-0)*8/16 = 2040/16 = 127
        // (same truncation as the alpha-blend 50/50 case above).
        assert_eq!(brighten(color, 8).r, 127);
        // evy=0: unchanged.
        assert_eq!(brighten(color, 0), color);
    }

    #[test]
    fn brighten_weights_above_16_behave_like_16() {
        let color = Rgb888 {
            r: 10,
            g: 10,
            b: 10,
        };
        assert_eq!(brighten(color, 31), brighten(color, 16));
    }

    #[test]
    fn darken_ramps_toward_black() {
        let color = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        // evy=16 (full): 255 - 255*16/16 = 0 -> channel 0.
        assert_eq!(darken(color, 16).r, 0);
        // evy=0: unchanged.
        assert_eq!(darken(color, 0), color);
    }

    // Provenance (issue #395's operator ruling: `no-verbatim` stays
    // absolute, including in `#[cfg(test)]` code). Every expected value in
    // the tables below -- `ALPHA_CHANNEL_EXPECTED`, `ALPHA_WEIGHT_GRID_EXPECTED`,
    // `ALPHA_BOUNDARY_GRID_EXPECTED`, `BRIGHTEN_EXPECTED`, and `DARKEN_EXPECTED`
    // -- was computed offline by a throwaway generator that is not part of
    // this repository and was never committed to it. That generator
    // evaluated the pinned `mgba/` reference checkout's stock-desktop
    // (non-`COLOR_16_BIT`) 8-bit color-effect formulas directly:
    //   `mColorMix5Bit`'s 8-bit path -- `image.h:307-327`
    //   `_brighten`'s 8-bit path     -- `software-private.h:237-245`
    //   `_darken`'s 8-bit path       -- `software-private.h:270-278`
    // (the same three citations the module docs above use). Only the
    // generator's *output* -- plain integers -- is committed here as
    // literal data; the formulas themselves never enter this tree, so
    // these tables are facts about the reference's observed behavior, not
    // a transcription of its code. The tests below perform no reference
    // calculation at runtime; they only compare against these literals.
    #[rustfmt::skip]
    const EXPANDED_CHANNELS: [u8; 32] = [
        0, 8, 16, 24, 33, 41, 49, 57, 66, 74, 82, 90, 99, 107, 115, 123, 132, 140, 148, 156, 165, 173, 181, 189, 198, 206, 214, 222, 231, 239, 247, 255
    ];

    const ALPHA_WEIGHT_PAIRS: [(u8, u8); 6] =
        [(8, 8), (16, 16), (16, 0), (0, 16), (5, 11), (13, 7)];

    // Tuples pin the complement, +7 modulo 32, and +19 modulo 32 source
    // mappings in the red, green, and blue lanes respectively. Provenance:
    // see the comment above `EXPANDED_CHANNELS`.
    #[rustfmt::skip]
    const ALPHA_CHANNEL_EXPECTED: [[(u8, u8, u8); 32]; 6] = [
        [(127, 28, 78), (127, 37, 86), (127, 45, 94), (127, 53, 102), (127, 61, 111), (127, 70, 119), (127, 78, 127), (127, 86, 135), (127, 94, 144), (127, 103, 152), (127, 111, 160), (127, 119, 168), (127, 127, 177), (127, 136, 53), (127, 144, 61), (127, 152, 69), (127, 160, 78), (127, 169, 86), (127, 177, 94), (127, 185, 102), (127, 193, 111), (127, 202, 119), (127, 210, 127), (127, 218, 135), (127, 226, 144), (127, 103, 152), (127, 111, 160), (127, 119, 168), (127, 127, 177), (127, 136, 185), (127, 144, 193), (127, 152, 201)],
        [(255, 57, 156), (255, 74, 173), (255, 90, 189), (255, 106, 205), (255, 123, 222), (255, 140, 239), (255, 156, 255), (255, 172, 255), (255, 189, 255), (255, 206, 255), (255, 222, 255), (255, 238, 255), (255, 255, 255), (255, 255, 107), (255, 255, 123), (255, 255, 139), (255, 255, 156), (255, 255, 173), (255, 255, 189), (255, 255, 205), (255, 255, 222), (255, 255, 239), (255, 255, 255), (255, 255, 255), (255, 255, 255), (255, 206, 255), (255, 222, 255), (255, 238, 255), (255, 255, 255), (255, 255, 255), (255, 255, 255), (255, 255, 255)],
        [(0, 0, 0), (8, 8, 8), (16, 16, 16), (24, 24, 24), (33, 33, 33), (41, 41, 41), (49, 49, 49), (57, 57, 57), (66, 66, 66), (74, 74, 74), (82, 82, 82), (90, 90, 90), (99, 99, 99), (107, 107, 107), (115, 115, 115), (123, 123, 123), (132, 132, 132), (140, 140, 140), (148, 148, 148), (156, 156, 156), (165, 165, 165), (173, 173, 173), (181, 181, 181), (189, 189, 189), (198, 198, 198), (206, 206, 206), (214, 214, 214), (222, 222, 222), (231, 231, 231), (239, 239, 239), (247, 247, 247), (255, 255, 255)],
        [(255, 57, 156), (247, 66, 165), (239, 74, 173), (231, 82, 181), (222, 90, 189), (214, 99, 198), (206, 107, 206), (198, 115, 214), (189, 123, 222), (181, 132, 231), (173, 140, 239), (165, 148, 247), (156, 156, 255), (148, 165, 0), (140, 173, 8), (132, 181, 16), (123, 189, 24), (115, 198, 33), (107, 206, 41), (99, 214, 49), (90, 222, 57), (82, 231, 66), (74, 239, 74), (66, 247, 82), (57, 255, 90), (49, 0, 99), (41, 8, 107), (33, 16, 115), (24, 24, 123), (16, 33, 132), (8, 41, 140), (0, 49, 148)],
        [(175, 39, 107), (172, 47, 115), (169, 55, 123), (166, 63, 131), (162, 72, 140), (159, 80, 148), (156, 88, 156), (153, 96, 164), (150, 105, 173), (147, 113, 181), (144, 121, 189), (141, 129, 197), (138, 138, 206), (135, 146, 33), (132, 154, 41), (129, 162, 49), (125, 171, 57), (122, 179, 66), (119, 187, 74), (116, 195, 82), (113, 204, 90), (110, 212, 99), (107, 220, 107), (104, 228, 115), (101, 237, 123), (98, 64, 132), (95, 72, 140), (92, 80, 148), (88, 88, 156), (85, 97, 165), (82, 105, 173), (79, 113, 181)],
        [(111, 24, 68), (114, 35, 78), (117, 45, 88), (120, 55, 98), (123, 66, 109), (126, 76, 119), (129, 86, 129), (132, 96, 139), (136, 107, 150), (139, 117, 161), (142, 127, 171), (145, 137, 181), (148, 148, 192), (151, 159, 86), (154, 169, 96), (157, 179, 106), (161, 189, 117), (164, 200, 128), (167, 210, 138), (170, 220, 148), (173, 231, 159), (176, 241, 169), (179, 251, 179), (182, 255, 189), (185, 255, 200), (188, 167, 210), (191, 177, 220), (194, 187, 230), (198, 198, 241), (201, 208, 251), (204, 218, 255), (207, 228, 255)],
    ];

    // Rows are EVA 0..=16 and columns are EVB 0..=16. Each tuple is the
    // expected result for first=(0, 255, 132), second=(255, 0, 123).
    // Provenance: see the comment above `EXPANDED_CHANNELS`.
    #[rustfmt::skip]
    const ALPHA_WEIGHT_GRID_EXPECTED: [[(u8, u8, u8); 17]; 17] = [
        [(0, 0, 0), (15, 0, 7), (31, 0, 15), (47, 0, 23), (63, 0, 30), (79, 0, 38), (95, 0, 46), (111, 0, 53), (127, 0, 61), (143, 0, 69), (159, 0, 76), (175, 0, 84), (191, 0, 92), (207, 0, 99), (223, 0, 107), (239, 0, 115), (255, 0, 123)],
        [(0, 15, 8), (15, 15, 15), (31, 15, 23), (47, 15, 31), (63, 15, 39), (79, 15, 46), (95, 15, 54), (111, 15, 62), (127, 15, 69), (143, 15, 77), (159, 15, 85), (175, 15, 92), (191, 15, 100), (207, 15, 108), (223, 15, 115), (239, 15, 123), (255, 15, 131)],
        [(0, 31, 16), (15, 31, 24), (31, 31, 31), (47, 31, 39), (63, 31, 47), (79, 31, 54), (95, 31, 62), (111, 31, 70), (127, 31, 78), (143, 31, 85), (159, 31, 93), (175, 31, 101), (191, 31, 108), (207, 31, 116), (223, 31, 124), (239, 31, 131), (255, 31, 139)],
        [(0, 47, 24), (15, 47, 32), (31, 47, 40), (47, 47, 47), (63, 47, 55), (79, 47, 63), (95, 47, 70), (111, 47, 78), (127, 47, 86), (143, 47, 93), (159, 47, 101), (175, 47, 109), (191, 47, 117), (207, 47, 124), (223, 47, 132), (239, 47, 140), (255, 47, 147)],
        [(0, 63, 33), (15, 63, 40), (31, 63, 48), (47, 63, 56), (63, 63, 63), (79, 63, 71), (95, 63, 79), (111, 63, 86), (127, 63, 94), (143, 63, 102), (159, 63, 109), (175, 63, 117), (191, 63, 125), (207, 63, 132), (223, 63, 140), (239, 63, 148), (255, 63, 156)],
        [(0, 79, 41), (15, 79, 48), (31, 79, 56), (47, 79, 64), (63, 79, 72), (79, 79, 79), (95, 79, 87), (111, 79, 95), (127, 79, 102), (143, 79, 110), (159, 79, 118), (175, 79, 125), (191, 79, 133), (207, 79, 141), (223, 79, 148), (239, 79, 156), (255, 79, 164)],
        [(0, 95, 49), (15, 95, 57), (31, 95, 64), (47, 95, 72), (63, 95, 80), (79, 95, 87), (95, 95, 95), (111, 95, 103), (127, 95, 111), (143, 95, 118), (159, 95, 126), (175, 95, 134), (191, 95, 141), (207, 95, 149), (223, 95, 157), (239, 95, 164), (255, 95, 172)],
        [(0, 111, 57), (15, 111, 65), (31, 111, 73), (47, 111, 80), (63, 111, 88), (79, 111, 96), (95, 111, 103), (111, 111, 111), (127, 111, 119), (143, 111, 126), (159, 111, 134), (175, 111, 142), (191, 111, 150), (207, 111, 157), (223, 111, 165), (239, 111, 173), (255, 111, 180)],
        [(0, 127, 66), (15, 127, 73), (31, 127, 81), (47, 127, 89), (63, 127, 96), (79, 127, 104), (95, 127, 112), (111, 127, 119), (127, 127, 127), (143, 127, 135), (159, 127, 142), (175, 127, 150), (191, 127, 158), (207, 127, 165), (223, 127, 173), (239, 127, 181), (255, 127, 189)],
        [(0, 143, 74), (15, 143, 81), (31, 143, 89), (47, 143, 97), (63, 143, 105), (79, 143, 112), (95, 143, 120), (111, 143, 128), (127, 143, 135), (143, 143, 143), (159, 143, 151), (175, 143, 158), (191, 143, 166), (207, 143, 174), (223, 143, 181), (239, 143, 189), (255, 143, 197)],
        [(0, 159, 82), (15, 159, 90), (31, 159, 97), (47, 159, 105), (63, 159, 113), (79, 159, 120), (95, 159, 128), (111, 159, 136), (127, 159, 144), (143, 159, 151), (159, 159, 159), (175, 159, 167), (191, 159, 174), (207, 159, 182), (223, 159, 190), (239, 159, 197), (255, 159, 205)],
        [(0, 175, 90), (15, 175, 98), (31, 175, 106), (47, 175, 113), (63, 175, 121), (79, 175, 129), (95, 175, 136), (111, 175, 144), (127, 175, 152), (143, 175, 159), (159, 175, 167), (175, 175, 175), (191, 175, 183), (207, 175, 190), (223, 175, 198), (239, 175, 206), (255, 175, 213)],
        [(0, 191, 99), (15, 191, 106), (31, 191, 114), (47, 191, 122), (63, 191, 129), (79, 191, 137), (95, 191, 145), (111, 191, 152), (127, 191, 160), (143, 191, 168), (159, 191, 175), (175, 191, 183), (191, 191, 191), (207, 191, 198), (223, 191, 206), (239, 191, 214), (255, 191, 222)],
        [(0, 207, 107), (15, 207, 114), (31, 207, 122), (47, 207, 130), (63, 207, 138), (79, 207, 145), (95, 207, 153), (111, 207, 161), (127, 207, 168), (143, 207, 176), (159, 207, 184), (175, 207, 191), (191, 207, 199), (207, 207, 207), (223, 207, 214), (239, 207, 222), (255, 207, 230)],
        [(0, 223, 115), (15, 223, 123), (31, 223, 130), (47, 223, 138), (63, 223, 146), (79, 223, 153), (95, 223, 161), (111, 223, 169), (127, 223, 177), (143, 223, 184), (159, 223, 192), (175, 223, 200), (191, 223, 207), (207, 223, 215), (223, 223, 223), (239, 223, 230), (255, 223, 238)],
        [(0, 239, 123), (15, 239, 131), (31, 239, 139), (47, 239, 146), (63, 239, 154), (79, 239, 162), (95, 239, 169), (111, 239, 177), (127, 239, 185), (143, 239, 192), (159, 239, 200), (175, 239, 208), (191, 239, 216), (207, 239, 223), (223, 239, 231), (239, 239, 239), (255, 239, 246)],
        [(0, 255, 132), (15, 255, 139), (31, 255, 147), (47, 255, 155), (63, 255, 162), (79, 255, 170), (95, 255, 178), (111, 255, 185), (127, 255, 193), (143, 255, 201), (159, 255, 208), (175, 255, 216), (191, 255, 224), (207, 255, 231), (223, 255, 239), (239, 255, 247), (255, 255, 255)],
    ];

    const ALPHA_BOUNDARY_GRID_INPUTS: [(Rgb888, Rgb888); 5] = [
        (
            Rgb888 { r: 0, g: 0, b: 0 },
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
        ),
        (
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
            Rgb888 { r: 0, g: 0, b: 0 },
        ),
        (
            Rgb888 {
                r: 255,
                g: 0,
                b: 140,
            },
            Rgb888 {
                r: 0,
                g: 255,
                b: 115,
            },
        ),
        (
            Rgb888 {
                r: 8,
                g: 247,
                b: 16,
            },
            Rgb888 {
                r: 247,
                g: 8,
                b: 239,
            },
        ),
        (
            Rgb888 {
                r: 132,
                g: 132,
                b: 132,
            },
            Rgb888 {
                r: 123,
                g: 123,
                b: 123,
            },
        ),
    ];

    // For each input pair, rows are EVA 0..=16. Every six hex digits are
    // one exact RGB result, with the 17 chunks in EVB 0..=16 order.
    // Provenance: see the comment above `EXPANDED_CHANNELS`.
    #[rustfmt::skip]
    const ALPHA_BOUNDARY_GRID_EXPECTED: [[&str; 17]; 5] = [
        [
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
            "0000000f0f0f1f1f1f2f2f2f3f3f3f4f4f4f5f5f5f6f6f6f7f7f7f8f8f8f9f9f9fafafafbfbfbfcfcfcfdfdfdfefefefffffff",
        ],
        [
            "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f",
            "1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f",
            "2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f",
            "3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f",
            "4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f",
            "5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f",
            "6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f",
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f",
            "9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f",
            "afafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafaf",
            "bfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbfbf",
            "cfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcf",
            "dfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdf",
            "efefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        ],
        [
            "000000000f07001f0e002f15003f1c004f23005f2b006f32007f39008f40009f4700af4f00bf5600cf5d00df6400ef6b00ff73",
            "0f00080f0f0f0f1f170f2f1e0f3f250f4f2c0f5f330f6f3b0f7f420f8f490f9f500faf570fbf5f0fcf660fdf6d0fef740fff7b",
            "1f00111f0f181f1f1f1f2f271f3f2e1f4f351f5f3c1f6f431f7f4b1f8f521f9f591faf601fbf671fcf6e1fdf761fef7d1fff84",
            "2f001a2f0f212f1f282f2f2f2f3f372f4f3e2f5f452f6f4c2f7f532f8f5a2f9f622faf692fbf702fcf772fdf7e2fef862fff8d",
            "3f00233f0f2a3f1f313f2f383f3f3f3f4f463f5f4e3f6f553f7f5c3f8f633f9f6a3faf723fbf793fcf803fdf873fef8e3fff96",
            "4f002b4f0f324f1f3a4f2f414f3f484f4f4f4f5f564f6f5e4f7f654f8f6c4f9f734faf7a4fbf824fcf894fdf904fef974fff9e",
            "5f00345f0f3b5f1f425f2f4a5f3f515f4f585f5f5f5f6f665f7f6e5f8f755f9f7c5faf835fbf8a5fcf915fdf995fefa05fffa7",
            "6f003d6f0f446f1f4b6f2f526f3f5a6f4f616f5f686f6f6f6f7f766f8f7d6f9f856faf8c6fbf936fcf9a6fdfa16fefa96fffb0",
            "7f00467f0f4d7f1f547f2f5b7f3f627f4f697f5f717f6f787f7f7f7f8f867f9f8d7faf957fbf9c7fcfa37fdfaa7fefb17fffb9",
            "8f004e8f0f558f1f5d8f2f648f3f6b8f4f728f5f798f6f818f7f888f8f8f8f9f968faf9d8fbfa58fcfac8fdfb38fefba8fffc1",
            "9f00579f0f5e9f1f659f2f6d9f3f749f4f7b9f5f829f6f899f7f919f8f989f9f9f9fafa69fbfad9fcfb49fdfbc9fefc39fffca",
            "af0060af0f67af1f6eaf2f75af3f7daf4f84af5f8baf6f92af7f99af8fa0af9fa8afafafafbfb6afcfbdafdfc4afefccafffd3",
            "bf0069bf0f70bf1f77bf2f7ebf3f85bf4f8cbf5f94bf6f9bbf7fa2bf8fa9bf9fb0bfafb8bfbfbfbfcfc6bfdfcdbfefd4bfffdc",
            "cf0071cf0f78cf1f80cf2f87cf3f8ecf4f95cf5f9ccf6fa4cf7fabcf8fb2cf9fb9cfafc0cfbfc8cfcfcfcfdfd6cfefddcfffe4",
            "df007adf0f81df1f88df2f90df3f97df4f9edf5fa5df6facdf7fb4df8fbbdf9fc2dfafc9dfbfd0dfcfd7dfdfdfdfefe6dfffed",
            "ef0083ef0f8aef1f91ef2f98ef3fa0ef4fa7ef5faeef6fb5ef7fbcef8fc3ef9fcbefafd2efbfd9efcfe0efdfe7efefefeffff6",
            "ff008cff0f93ff1f9aff2fa1ff3fa8ff4fafff5fb7ff6fbeff7fc5ff8fccff9fd3ffafdbffbfe2ffcfe9ffdff0ffeff7ffffff",
        ],
        [
            "0000000f000e1e011d2e012c3d023b4d024a5c03596c03687b04778a04869a0595a905a4b906b3c806c2d807d1e707e0f708ef",
            "000f010f0f0f1f101e2e102d3e113c4d114b5d125a6c12697c13788b13879a1496aa14a5b915b4c915c3d816d2e816e1f717f0",
            "011e02101f101f1f1f2f202e3e203d4e214c5d215b6d226a7c22798b23889b2397aa24a6ba24b5c925c4d925d3e826e2f826f1",
            "012e03102e11202f202f2f2f3f303e4e304d5e315c6d316b7d327a8c32899b3398ab33a7ba34b6ca34c5d935d4e935e3f836f2",
            "023d04113e12203e21303f303f3f3f4f404e5e405d6e416c7d417b8c428a9c4299ab43a8bb43b7ca44c6da44d5e945e4f945f3",
            "024d05114d13214e22304e31404f404f4f4f5f505e6e506d7e517c8d518b9c529aac52a9bb53b8cb53c7da54d6ea54e5f955f4",
            "035c06125d14215d23315e32405e41505f505f5f5f6f606e7e607d8d618c9d619bac62aabc62b9cb63c8db63d7ea64e6fa64f5",
            "036c07126c15226d24316d33416e42506e51606f606f6f6f7f707e8e708d9d719cad71abbc72bacc72c9db73d8eb73e7fa74f6",
            "047b08137c16227c25327d34417d43517e52607e61707f707f7f7f8e808e9e809dad81acbd81bbcc82cadc82d9eb83e8fb83f7",
            "048a09138b17238b26328c35428c44518d53618d62708e71808e808f8f8f9e8f9eae90adbd90bccd91cbdc91daec92e9fb92f8",
            "059a0a149a18239b27339b36429c45529c54619d63719d72809e818f9e909f9f9fae9faebea0bdcda0ccdda1dbeca1eafca2f9",
            "05a90b14aa1924aa2833ab3743ab4652ac5562ac6471ad7381ad8290ae919faea0afafafbeafbeceb0cdddb0dcedb1ebfcb1fa",
            "06b90c15b91a24ba2934ba3843bb4753bb5662bc6572bc7481bd8390bd92a0bea1afbeb0bfbfbfcebfcedec0ddedc0ecfdc1fb",
            "06c80d15c91b25c92a34ca3944ca4853cb5763cb6672cc7582cc8491cd93a0cda2b0ceb1bfcec0cfcfcfdecfdeeed0edfdd0fc",
            "07d80e16d81c25d92b35d93a44da4954da5863db6773db7682dc8591dc94a1dda3b0ddb2c0dec1cfded0dfdfdfeedfeefee0fd",
            "07e70f16e81d26e82c35e93b45e94a54ea5964ea6873eb7783eb8692ec95a1eca4b1edb3c0edc2d0eed1dfeee0efefeffeeffe",
            "08f71017f71e26f82d36f83c45f94b55f95a64fa6974fa7883fb8792fb96a2fca5b1fcb4c1fdc3d0fdd2e0fee1effef0ffffff",
        ],
        [
            "0000000707070f0f0f1717171e1e1e2626262e2e2e3535353d3d3d4545454c4c4c5454545c5c5c6363636b6b6b7373737b7b7b",
            "0808080f0f0f1717171f1f1f2727272e2e2e3636363e3e3e4545454d4d4d5555555c5c5c6464646c6c6c7373737b7b7b838383",
            "1010101818181f1f1f2727272f2f2f3636363e3e3e4646464e4e4e5555555d5d5d6565656c6c6c7474747c7c7c8383838b8b8b",
            "1818182020202828282f2f2f3737373f3f3f4646464e4e4e5656565d5d5d6565656d6d6d7575757c7c7c8484848c8c8c939393",
            "2121212828283030303838383f3f3f4747474f4f4f5656565e5e5e6666666d6d6d7575757d7d7d8484848c8c8c9494949c9c9c",
            "2929293030303838384040404848484f4f4f5757575f5f5f6666666e6e6e7676767d7d7d8585858d8d8d9494949c9c9ca4a4a4",
            "3131313939394040404848485050505757575f5f5f6767676f6f6f7676767e7e7e8686868d8d8d9595959d9d9da4a4a4acacac",
            "3939394141414949495050505858586060606767676f6f6f7777777e7e7e8686868e8e8e9696969d9d9da5a5a5adadadb4b4b4",
            "4242424949495151515959596060606868687070707777777f7f7f8787878e8e8e9696969e9e9ea5a5a5adadadb5b5b5bdbdbd",
            "4a4a4a5151515959596161616969697070707878788080808787878f8f8f9797979e9e9ea6a6a6aeaeaeb5b5b5bdbdbdc5c5c5",
            "5252525a5a5a6161616969697171717878788080808888889090909797979f9f9fa7a7a7aeaeaeb6b6b6bebebec5c5c5cdcdcd",
            "5a5a5a6262626a6a6a7171717979798181818888889090909898989f9f9fa7a7a7afafafb7b7b7bebebec6c6c6cececed5d5d5",
            "6363636a6a6a7272727a7a7a818181898989919191989898a0a0a0a8a8a8afafafb7b7b7bfbfbfc6c6c6cececed6d6d6dedede",
            "6b6b6b7272727a7a7a8282828a8a8a919191999999a1a1a1a8a8a8b0b0b0b8b8b8bfbfbfc7c7c7cfcfcfd6d6d6dededee6e6e6",
            "7373737b7b7b8282828a8a8a929292999999a1a1a1a9a9a9b1b1b1b8b8b8c0c0c0c8c8c8cfcfcfd7d7d7dfdfdfe6e6e6eeeeee",
            "7b7b7b8383838b8b8b9292929a9a9aa2a2a2a9a9a9b1b1b1b9b9b9c0c0c0c8c8c8d0d0d0d8d8d8dfdfdfe7e7e7efefeff6f6f6",
            "8484848b8b8b9393939b9b9ba2a2a2aaaaaab2b2b2b9b9b9c1c1c1c9c9c9d0d0d0d8d8d8e0e0e0e7e7e7efefeff7f7f7ffffff",
        ],
    ];

    // Rows are EVY 0..=16; columns follow EXPANDED_CHANNELS. Provenance:
    // see the comment above `EXPANDED_CHANNELS`.
    #[rustfmt::skip]
    const BRIGHTEN_EXPECTED: [[u8; 32]; 17] = [
        [0, 8, 16, 24, 33, 41, 49, 57, 66, 74, 82, 90, 99, 107, 115, 123, 132, 140, 148, 156, 165, 173, 181, 189, 198, 206, 214, 222, 231, 239, 247, 255],
        [15, 23, 30, 38, 46, 54, 61, 69, 77, 85, 92, 100, 108, 116, 123, 131, 139, 147, 154, 162, 170, 178, 185, 193, 201, 209, 216, 224, 232, 240, 247, 255],
        [31, 38, 45, 52, 60, 67, 74, 81, 89, 96, 103, 110, 118, 125, 132, 139, 147, 154, 161, 168, 176, 183, 190, 197, 205, 212, 219, 226, 234, 241, 248, 255],
        [47, 54, 60, 67, 74, 81, 87, 94, 101, 107, 114, 120, 128, 134, 141, 147, 155, 161, 168, 174, 181, 188, 194, 201, 208, 215, 221, 228, 235, 242, 248, 255],
        [63, 69, 75, 81, 88, 94, 100, 106, 113, 119, 125, 131, 138, 144, 150, 156, 162, 168, 174, 180, 187, 193, 199, 205, 212, 218, 224, 230, 237, 243, 249, 255],
        [79, 85, 90, 96, 102, 107, 113, 118, 125, 130, 136, 141, 147, 153, 158, 164, 170, 175, 181, 186, 193, 198, 204, 209, 215, 221, 226, 232, 238, 244, 249, 255],
        [95, 100, 105, 110, 116, 121, 126, 131, 136, 141, 146, 151, 157, 162, 167, 172, 178, 183, 188, 193, 198, 203, 208, 213, 219, 224, 229, 234, 240, 245, 250, 255],
        [111, 116, 120, 125, 130, 134, 139, 143, 148, 153, 157, 162, 167, 171, 176, 180, 185, 190, 194, 199, 204, 208, 213, 217, 222, 227, 231, 236, 241, 246, 250, 255],
        [127, 131, 135, 139, 144, 148, 152, 156, 160, 164, 168, 172, 177, 181, 185, 189, 193, 197, 201, 205, 210, 214, 218, 222, 226, 230, 234, 238, 243, 247, 251, 255],
        [143, 146, 150, 153, 157, 161, 164, 168, 172, 175, 179, 182, 186, 190, 193, 197, 201, 204, 208, 211, 215, 219, 222, 226, 230, 233, 237, 240, 244, 248, 251, 255],
        [159, 162, 165, 168, 171, 174, 177, 180, 184, 187, 190, 193, 196, 199, 202, 205, 208, 211, 214, 217, 221, 224, 227, 230, 233, 236, 239, 242, 246, 249, 252, 255],
        [175, 177, 180, 182, 185, 188, 190, 193, 195, 198, 200, 203, 206, 208, 211, 213, 216, 219, 221, 224, 226, 229, 231, 234, 237, 239, 242, 244, 247, 250, 252, 255],
        [191, 193, 195, 197, 199, 201, 203, 205, 207, 209, 211, 213, 216, 218, 220, 222, 224, 226, 228, 230, 232, 234, 236, 238, 240, 242, 244, 246, 249, 251, 253, 255],
        [207, 208, 210, 211, 213, 214, 216, 217, 219, 221, 222, 224, 225, 227, 228, 230, 231, 233, 234, 236, 238, 239, 241, 242, 244, 245, 247, 248, 250, 252, 253, 255],
        [223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 252, 253, 254, 255],
        [239, 239, 240, 240, 241, 241, 242, 242, 243, 243, 244, 244, 245, 245, 246, 246, 247, 247, 248, 248, 249, 249, 250, 250, 251, 251, 252, 252, 253, 254, 254, 255],
        [255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255],
    ];

    // Each tuple is (red-lane result, shifted-lane result). Rows are EVY
    // 0..=16; columns follow EXPANDED_CHANNELS. Provenance: see the
    // comment above `EXPANDED_CHANNELS`.
    #[rustfmt::skip]
    const DARKEN_EXPECTED: [[(u8, u8); 32]; 17] = [
        [(0, 0), (8, 8), (16, 16), (24, 24), (33, 33), (41, 41), (49, 49), (57, 57), (66, 66), (74, 74), (82, 82), (90, 90), (99, 99), (107, 107), (115, 115), (123, 123), (132, 132), (140, 140), (148, 148), (156, 156), (165, 165), (173, 173), (181, 181), (189, 189), (198, 198), (206, 206), (214, 214), (222, 222), (231, 231), (239, 239), (247, 247), (255, 255)],
        [(0, 0), (8, 7), (15, 15), (23, 22), (31, 30), (39, 38), (46, 45), (54, 53), (62, 61), (70, 69), (77, 76), (85, 84), (93, 92), (101, 100), (108, 107), (116, 115), (124, 123), (132, 131), (139, 138), (147, 146), (155, 154), (163, 162), (170, 169), (178, 177), (186, 185), (194, 193), (201, 200), (209, 208), (217, 216), (225, 224), (232, 231), (240, 239)],
        [(0, 0), (7, 7), (14, 14), (21, 21), (29, 28), (36, 35), (43, 42), (50, 49), (58, 57), (65, 64), (72, 71), (79, 78), (87, 86), (94, 93), (101, 100), (108, 107), (116, 115), (123, 122), (130, 129), (137, 136), (145, 144), (152, 151), (159, 158), (166, 165), (174, 173), (181, 180), (188, 187), (195, 194), (203, 202), (210, 209), (217, 216), (224, 223)],
        [(0, 0), (7, 6), (13, 13), (20, 19), (27, 26), (34, 33), (40, 39), (47, 46), (54, 53), (61, 60), (67, 66), (74, 73), (81, 80), (87, 86), (94, 93), (100, 99), (108, 107), (114, 113), (121, 120), (127, 126), (135, 134), (141, 140), (148, 147), (154, 153), (161, 160), (168, 167), (174, 173), (181, 180), (188, 187), (195, 194), (201, 200), (208, 207)],
        [(0, 0), (6, 6), (12, 12), (18, 18), (25, 24), (31, 30), (37, 36), (43, 42), (50, 49), (56, 55), (62, 61), (68, 67), (75, 74), (81, 80), (87, 86), (93, 92), (99, 99), (105, 105), (111, 111), (117, 117), (124, 123), (130, 129), (136, 135), (142, 141), (149, 148), (155, 154), (161, 160), (167, 166), (174, 173), (180, 179), (186, 185), (192, 191)],
        [(0, 0), (6, 5), (11, 11), (17, 16), (23, 22), (29, 28), (34, 33), (40, 39), (46, 45), (51, 50), (57, 56), (62, 61), (69, 68), (74, 73), (80, 79), (85, 84), (91, 90), (97, 96), (102, 101), (108, 107), (114, 113), (119, 118), (125, 124), (130, 129), (137, 136), (142, 141), (148, 147), (153, 152), (159, 158), (165, 164), (170, 169), (176, 175)],
        [(0, 0), (5, 5), (10, 10), (15, 15), (21, 20), (26, 25), (31, 30), (36, 35), (42, 41), (47, 46), (52, 51), (57, 56), (62, 61), (67, 66), (72, 71), (77, 76), (83, 82), (88, 87), (93, 92), (98, 97), (104, 103), (109, 108), (114, 113), (119, 118), (124, 123), (129, 128), (134, 133), (139, 138), (145, 144), (150, 149), (155, 154), (160, 159)],
        [(0, 0), (5, 4), (9, 9), (14, 13), (19, 18), (24, 23), (28, 27), (33, 32), (38, 37), (42, 41), (47, 46), (51, 50), (56, 55), (61, 60), (65, 64), (70, 69), (75, 74), (79, 78), (84, 83), (88, 87), (93, 92), (98, 97), (102, 101), (107, 106), (112, 111), (116, 115), (121, 120), (125, 124), (130, 129), (135, 134), (139, 138), (144, 143)],
        [(0, 0), (4, 4), (8, 8), (12, 12), (17, 16), (21, 20), (25, 24), (29, 28), (33, 33), (37, 37), (41, 41), (45, 45), (50, 49), (54, 53), (58, 57), (62, 61), (66, 66), (70, 70), (74, 74), (78, 78), (83, 82), (87, 86), (91, 90), (95, 94), (99, 99), (103, 103), (107, 107), (111, 111), (116, 115), (120, 119), (124, 123), (128, 127)],
        [(0, 0), (4, 3), (7, 7), (11, 10), (15, 14), (18, 17), (22, 21), (25, 24), (29, 28), (33, 32), (36, 35), (40, 39), (44, 43), (47, 46), (51, 50), (54, 53), (58, 57), (62, 61), (65, 64), (69, 68), (73, 72), (76, 75), (80, 79), (83, 82), (87, 86), (91, 90), (94, 93), (98, 97), (102, 101), (105, 104), (109, 108), (112, 111)],
        [(0, 0), (3, 3), (6, 6), (9, 9), (13, 12), (16, 15), (19, 18), (22, 21), (25, 24), (28, 27), (31, 30), (34, 33), (38, 37), (41, 40), (44, 43), (47, 46), (50, 49), (53, 52), (56, 55), (59, 58), (62, 61), (65, 64), (68, 67), (71, 70), (75, 74), (78, 77), (81, 80), (84, 83), (87, 86), (90, 89), (93, 92), (96, 95)],
        [(0, 0), (3, 2), (5, 5), (8, 7), (11, 10), (13, 12), (16, 15), (18, 17), (21, 20), (24, 23), (26, 25), (29, 28), (31, 30), (34, 33), (36, 35), (39, 38), (42, 41), (44, 43), (47, 46), (49, 48), (52, 51), (55, 54), (57, 56), (60, 59), (62, 61), (65, 64), (67, 66), (70, 69), (73, 72), (75, 74), (78, 77), (80, 79)],
        [(0, 0), (2, 2), (4, 4), (6, 6), (9, 8), (11, 10), (13, 12), (15, 14), (17, 16), (19, 18), (21, 20), (23, 22), (25, 24), (27, 26), (29, 28), (31, 30), (33, 33), (35, 35), (37, 37), (39, 39), (42, 41), (44, 43), (46, 45), (48, 47), (50, 49), (52, 51), (54, 53), (56, 55), (58, 57), (60, 59), (62, 61), (64, 63)],
        [(0, 0), (2, 1), (3, 3), (5, 4), (7, 6), (8, 7), (10, 9), (11, 10), (13, 12), (14, 13), (16, 15), (17, 16), (19, 18), (21, 20), (22, 21), (24, 23), (25, 24), (27, 26), (28, 27), (30, 29), (31, 30), (33, 32), (34, 33), (36, 35), (38, 37), (39, 38), (41, 40), (42, 41), (44, 43), (45, 44), (47, 46), (48, 47)],
        [(0, 0), (1, 1), (2, 2), (3, 3), (5, 4), (6, 5), (7, 6), (8, 7), (9, 8), (10, 9), (11, 10), (12, 11), (13, 12), (14, 13), (15, 14), (16, 15), (17, 16), (18, 17), (19, 18), (20, 19), (21, 20), (22, 21), (23, 22), (24, 23), (25, 24), (26, 25), (27, 26), (28, 27), (29, 28), (30, 29), (31, 30), (32, 31)],
        [(0, 0), (1, 0), (1, 1), (2, 1), (3, 2), (3, 2), (4, 3), (4, 3), (5, 4), (5, 4), (6, 5), (6, 5), (7, 6), (7, 6), (8, 7), (8, 7), (9, 8), (9, 8), (10, 9), (10, 9), (11, 10), (11, 10), (12, 11), (12, 11), (13, 12), (13, 12), (14, 13), (14, 13), (15, 14), (15, 14), (16, 15), (16, 15)],
        [(0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
    ];

    /// Covers every expanded channel value against representative weight
    /// pairs, including identity and saturation cases, using committed
    /// fixture values rather than an executable reference implementation.
    #[test]
    fn alpha_blend_matches_committed_channel_fixtures() {
        for ((eva, evb), expected_row) in ALPHA_WEIGHT_PAIRS
            .iter()
            .copied()
            .zip(ALPHA_CHANNEL_EXPECTED.iter())
        {
            for (index, (&first_channel, &(r, g, b))) in EXPANDED_CHANNELS
                .iter()
                .zip(expected_row.iter())
                .enumerate()
            {
                let first = Rgb888 {
                    r: first_channel,
                    g: first_channel,
                    b: first_channel,
                };
                let second = Rgb888 {
                    r: EXPANDED_CHANNELS[31 - index],
                    g: EXPANDED_CHANNELS[(index + 7) % 32],
                    b: EXPANDED_CHANNELS[(index + 19) % 32],
                };
                let expected = Rgb888 { r, g, b };
                assert_eq!(
                    alpha_blend(first, second, eva, evb),
                    expected,
                    "alpha mapping fixture index {index}, weights ({eva}, {evb})"
                );
            }
        }
    }

    /// Pins every usable EVA/EVB pair over boundary-heavy source channels.
    #[test]
    fn alpha_blend_matches_committed_weight_grid() {
        let first = Rgb888 {
            r: 0,
            g: 255,
            b: 132,
        };
        let second = Rgb888 {
            r: 255,
            g: 0,
            b: 123,
        };
        for (eva, expected_row) in ALPHA_WEIGHT_GRID_EXPECTED.iter().enumerate() {
            let eva = u8::try_from(eva).expect("fixture index is at most 16");
            for (evb, &(r, g, b)) in expected_row.iter().enumerate() {
                let evb = u8::try_from(evb).expect("fixture index is at most 16");
                let expected = Rgb888 { r, g, b };
                assert_eq!(
                    alpha_blend(first, second, eva, evb),
                    expected,
                    "alpha weight-grid fixture ({eva}, {evb})"
                );
            }
        }
    }

    /// Restores the full weight grids for the five original boundary-heavy
    /// source pairs. Hex encoding keeps the exact literal fixtures compact;
    /// no expected result is calculated while the test runs.
    #[test]
    fn alpha_blend_matches_committed_boundary_weight_grids() {
        use std::fmt::Write as _;

        for (case, ((first, second), expected_rows)) in ALPHA_BOUNDARY_GRID_INPUTS
            .iter()
            .copied()
            .zip(ALPHA_BOUNDARY_GRID_EXPECTED.iter())
            .enumerate()
        {
            for (eva, expected_row) in expected_rows.iter().enumerate() {
                let eva = u8::try_from(eva).expect("fixture index is at most 16");
                let mut actual_row = String::with_capacity(17 * 6);
                for evb in 0..=16 {
                    let actual = alpha_blend(first, second, eva, evb);
                    write!(
                        actual_row,
                        "{:02x}{:02x}{:02x}",
                        actual.r, actual.g, actual.b
                    )
                    .expect("writing to a String cannot fail");
                }
                assert_eq!(
                    actual_row, *expected_row,
                    "alpha boundary-grid fixture case {case}, EVA {eva}"
                );
            }
        }
    }

    /// Pins every expanded channel value against every usable EVY.
    #[test]
    fn brighten_matches_committed_fixtures() {
        for (evy, expected_row) in BRIGHTEN_EXPECTED.iter().enumerate() {
            let evy = u8::try_from(evy).expect("fixture index is at most 16");
            for (&channel, &expected_channel) in EXPANDED_CHANNELS.iter().zip(expected_row.iter()) {
                let color = Rgb888 {
                    r: channel,
                    g: channel,
                    b: channel,
                };
                let expected = Rgb888 {
                    r: expected_channel,
                    g: expected_channel,
                    b: expected_channel,
                };
                assert_eq!(
                    brighten(color, evy),
                    expected,
                    "brighten fixture channel {channel}, EVY {evy}"
                );
            }
        }
    }

    /// Pins every expanded channel value against every usable EVY, with
    /// separate expected values for the low and shifted packed-color lanes.
    #[test]
    fn darken_matches_committed_fixtures() {
        for (evy, expected_row) in DARKEN_EXPECTED.iter().enumerate() {
            let evy = u8::try_from(evy).expect("fixture index is at most 16");
            for (&channel, &(red, shifted)) in EXPANDED_CHANNELS.iter().zip(expected_row.iter()) {
                let color = Rgb888 {
                    r: channel,
                    g: channel,
                    b: channel,
                };
                let expected = Rgb888 {
                    r: red,
                    g: shifted,
                    b: shifted,
                };
                assert_eq!(
                    darken(color, evy),
                    expected,
                    "darken fixture channel {channel}, EVY {evy}"
                );
            }
        }
    }

    #[test]
    fn darken_rounds_shifted_lanes_one_step_darker_than_red() {
        // The direct witness from the module docs: darkening white at EVY=7
        // produces red 144 but green and blue 143 because only the low
        // lane's remainder is absorbed by the division.
        let white = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        assert_eq!(
            darken(white, 7),
            Rgb888 {
                r: 144,
                g: 143,
                b: 143
            }
        );
    }

    #[test]
    fn darken_weights_above_16_behave_like_16() {
        let color = Rgb888 {
            r: 200,
            g: 200,
            b: 200,
        };
        assert_eq!(darken(color, 31), darken(color, 16));
    }

    fn opaque_bg0_target1() -> EffectsConfig {
        EffectsConfig {
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
        }
    }

    #[test]
    fn resolve_alpha_blends_front_with_a_valid_immediate_target2() {
        let cfg = opaque_bg0_target1();
        let front = (Rgb888 { r: 0, g: 0, b: 0 }, LayerKind::Bg(0), false);
        let next = Some((
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
            LayerKind::Bg(1),
        ));
        let result = resolve_pixel_color(&cfg, true, true, front, next, Rgb888::BLACK);
        // eva=evb=8 blend of 0 and 255 (8-bit oracle, module docs):
        // (0*8+255*8)/16 = 127 (see effects tests' alpha_blend_hand_computed_50_50).
        assert_eq!(result.r, 127);
    }

    #[test]
    fn resolve_does_not_blend_when_immediate_next_is_not_target2() {
        let cfg = opaque_bg0_target1();
        let front_color = Rgb888 { r: 9, g: 9, b: 9 };
        let front = (front_color, LayerKind::Bg(0), false);
        // BG2 is not configured as target2 -- must not blend, and must not
        // fall through to the backdrop either.
        let next = Some((
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
            LayerKind::Bg(2),
        ));
        let result = resolve_pixel_color(
            &cfg,
            true,
            true,
            front,
            next,
            Rgb888 {
                r: 200,
                g: 200,
                b: 200,
            },
        );
        assert_eq!(result, front_color);
    }

    #[test]
    fn resolve_blends_against_the_backdrop_when_nothing_else_is_behind() {
        let mut cfg = opaque_bg0_target1();
        cfg.target2.backdrop = true;
        let front = (Rgb888 { r: 0, g: 0, b: 0 }, LayerKind::Bg(0), false);
        let backdrop = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        let result = resolve_pixel_color(&cfg, true, true, front, None, backdrop);
        // eva=evb=8 blend of 0 and 255 (8-bit oracle, module docs):
        // (0*8+255*8)/16 = 127 (see effects tests' alpha_blend_hand_computed_50_50).
        assert_eq!(result.r, 127);
    }

    #[test]
    fn resolve_semi_transparent_obj_forces_blend_even_when_effect_is_brighten() {
        // BLDCNT selected BRIGHTEN, not alpha -- but a semi-transparent OBJ
        // (forced_alpha=true) must still alpha-blend against a valid target2
        // behind it, overriding the configured effect.
        let cfg = EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets::default(), // OBJ is *not* configured target1
            target2: LayerTargets {
                bg: [true, false, false, false],
                obj: false,
                backdrop: false,
            },
            eva: 8,
            evb: 8,
            evy: 16,
        };
        let front = (Rgb888 { r: 0, g: 0, b: 0 }, LayerKind::Obj, true);
        let next = Some((
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
            LayerKind::Bg(0),
        ));
        let result = resolve_pixel_color(&cfg, true, true, front, next, Rgb888::BLACK);
        // eva=evb=8 blend of 0 and 255 (8-bit oracle): (0*8+255*8)/16 = 127.
        assert_eq!(
            result.r, 127,
            "semi-transparency must force alpha blend regardless of the selected effect"
        );
    }

    #[test]
    fn resolve_semi_transparent_obj_forces_blend_when_window_effects_are_disabled() {
        // OAM mode 1 sets mGBA's FLAG_TARGET_1 independently of the current
        // window's BlendEnable bit (software-obj.c:159). A valid target2
        // therefore still blends when effects_enabled=false.
        let cfg = EffectsConfig {
            effect: ColorEffect::None,
            target1: LayerTargets::default(),
            target2: LayerTargets {
                bg: [true, false, false, false],
                obj: false,
                backdrop: false,
            },
            eva: 8,
            evb: 8,
            evy: 0,
        };
        let front = (Rgb888 { r: 0, g: 0, b: 0 }, LayerKind::Obj, true);
        let next = Some((
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
            LayerKind::Bg(0),
        ));

        let result = resolve_pixel_color(&cfg, false, true, front, next, Rgb888::BLACK);

        // eva=evb=8 blend of 0 and 255 (8-bit oracle): (0*8+255*8)/16 = 127.
        assert_eq!(
            result.r, 127,
            "window effect-disable must not suppress OAM mode 1 forced alpha"
        );
    }

    #[test]
    fn resolve_semi_transparent_obj_with_no_target2_stays_unblended() {
        // Default config: OBJ is *not* a BLDCNT first target and the effect is
        // None, so mgba's `variant` selector is off — with no target2 the
        // forced blend is dropped and no brighten/darken applies, leaving the
        // raw color (software-obj.c:177-192).
        let cfg = EffectsConfig::default();
        let front_color = Rgb888 { r: 7, g: 7, b: 7 };
        let front = (front_color, LayerKind::Obj, true);
        let result = resolve_pixel_color(&cfg, true, false, front, None, Rgb888::BLACK);
        assert_eq!(result, front_color);
    }

    #[test]
    fn resolve_semi_transparent_obj_target1_brighten_with_no_target2_still_brightens() {
        // A semi-transparent OBJ (forced_alpha=true) that is *also* a BLDCNT
        // first target under a BRIGHTEN effect, with nothing valid behind it.
        // mgba only zeroes its brighten/darken `variant` in the target2-present
        // branch; with no target2 it clears FLAG_TARGET_1 but keeps `variant`,
        // so the sprite is brightened rather than emitted raw
        // (software-obj.c:177-192). The pre-fix code returned the raw color on
        // this no-second-target path.
        let cfg = EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets {
                bg: [false; 4],
                obj: true, // OBJ *is* a BLDCNT first target
                backdrop: false,
            },
            target2: LayerTargets::default(), // nothing is a valid second target
            eva: 0,
            evb: 0,
            evy: 16,
        };
        let front = (Rgb888 { r: 0, g: 0, b: 0 }, LayerKind::Obj, true);
        let result = resolve_pixel_color(&cfg, true, false, front, None, Rgb888::BLACK);
        assert_eq!(
            result.r, 255,
            "no valid target2 must fall back to the selected brighten, not the raw color"
        );
    }

    #[test]
    fn resolve_semi_transparent_obj_immediate_next_not_target2_but_global_target2_emits_raw() {
        // Finding 3: a forced-alpha OBJ that is also a BLDCNT first target
        // under BRIGHTEN, whose *immediate* next layer is NOT a target2 — but
        // some other target2 layer exists in the frame
        // (any_target2_enabled=true). mgba clears the brighten/darken `variant`
        // whenever any target2 exists globally (software-obj.c:177-192), so the
        // sprite emits its raw color, not a brightened one. The pre-fix code
        // read only the immediate neighbor and wrongly brightened here.
        let cfg = EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets {
                bg: [false; 4],
                obj: true,
                backdrop: false,
            },
            target2: LayerTargets {
                bg: [false, true, false, false], // BG1 is a target2 elsewhere
                obj: false,
                backdrop: false,
            },
            eva: 8,
            evb: 8,
            evy: 16,
        };
        let front_color = Rgb888 { r: 0, g: 0, b: 0 };
        let front = (front_color, LayerKind::Obj, true);
        // The immediate neighbor is BG2, which is not a target2.
        let next = Some((
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
            LayerKind::Bg(2),
        ));
        let result = resolve_pixel_color(&cfg, true, true, front, next, Rgb888::BLACK);
        assert_eq!(
            result, front_color,
            "a global target2 clears the variant -> raw color, not brighten"
        );
    }

    #[test]
    fn resolve_semi_transparent_obj_no_target2_anywhere_falls_back_to_brighten() {
        // The other side of finding 3: the same forced-alpha OBJ + BRIGHTEN
        // first target with an immediate neighbor that is not a target2, but
        // NO target2 exists anywhere (any_target2_enabled=false). mgba keeps
        // `variant`, so the sprite is brightened rather than emitted raw.
        let cfg = EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets {
                bg: [false; 4],
                obj: true,
                backdrop: false,
            },
            target2: LayerTargets::default(),
            eva: 0,
            evb: 0,
            evy: 16,
        };
        let front = (Rgb888 { r: 0, g: 0, b: 0 }, LayerKind::Obj, true);
        let next = Some((
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
            LayerKind::Bg(0),
        ));
        let result = resolve_pixel_color(&cfg, true, false, front, next, Rgb888::BLACK);
        assert_eq!(
            result.r, 255,
            "no target2 anywhere -> variant survives -> full brighten to white"
        );
    }

    #[test]
    fn resolve_brightens_a_target1_front_with_no_target2_needed() {
        let cfg = EffectsConfig {
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
        };
        let front = (Rgb888 { r: 0, g: 0, b: 0 }, LayerKind::Bg(0), false);
        let result = resolve_pixel_color(&cfg, true, false, front, None, Rgb888::BLACK);
        assert_eq!(result.r, 255, "full brighten of black must reach white");
    }

    #[test]
    fn resolve_darkens_a_target1_front() {
        let cfg = EffectsConfig {
            effect: ColorEffect::Darken,
            target1: LayerTargets {
                bg: [false, false, false, false],
                obj: true,
                backdrop: false,
            },
            target2: LayerTargets::default(),
            eva: 0,
            evb: 0,
            evy: 16,
        };
        let front = (
            Rgb888 {
                r: 255,
                g: 255,
                b: 255,
            },
            LayerKind::Obj,
            false,
        );
        let result = resolve_pixel_color(&cfg, true, false, front, None, Rgb888::BLACK);
        assert_eq!(result.r, 0, "full darken of white must reach black");
    }

    #[test]
    fn resolve_window_effects_disabled_suppresses_brighten() {
        let cfg = EffectsConfig {
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
        };
        let front_color = Rgb888 {
            r: 10,
            g: 10,
            b: 10,
        };
        let front = (front_color, LayerKind::Bg(0), false);
        let result = resolve_pixel_color(&cfg, false, false, front, None, Rgb888::BLACK);
        assert_eq!(
            result, front_color,
            "window's effect-enable bit must gate brighten"
        );
    }

    #[test]
    fn resolve_backdrop_as_front_can_be_brightened() {
        let cfg = EffectsConfig {
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
        };
        let backdrop = Rgb888 { r: 0, g: 0, b: 0 };
        let front = (backdrop, LayerKind::Backdrop, false);
        let result = resolve_pixel_color(&cfg, true, false, front, None, backdrop);
        assert_eq!(result.r, 255);
    }

    #[test]
    fn resolve_backdrop_as_front_never_alpha_blends_with_itself() {
        // A contrived config marking the backdrop as both target1 and
        // target2 under ALPHA must not blend the backdrop against itself --
        // there is nothing behind it (module docs).
        let cfg = EffectsConfig {
            effect: ColorEffect::AlphaBlend,
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
            eva: 16,
            evb: 16,
            evy: 0,
        };
        let backdrop = Rgb888 {
            r: 42,
            g: 42,
            b: 42,
        };
        let front = (backdrop, LayerKind::Backdrop, false);
        let result = resolve_pixel_color(&cfg, true, true, front, None, backdrop);
        assert_eq!(result, backdrop);
    }
}
