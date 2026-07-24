//! Hardware color special effects: alpha blend, brightness increase/decrease
//! (S-2 slice 4, issue #99).
//!
//! Ports `BLDCNT`/`BLDALPHA`/`BLDY`'s per-pixel color math, verified against
//! `mgba/include/mgba-util/image.h`'s `mColorMix5Bit` and
//! `mgba/src/gba/renderers/software-private.h`'s `_brighten`/`_darken`. GBA
//! color math happens on 5-bit-per-channel values, so [`alpha_blend`],
//! [`brighten`], and [`darken`] round-trip an [`Rgb888`] through its
//! originating 5-bit channel (see [`crate::palette::compress_8_to_5`])
//! before doing the hardware arithmetic and re-expanding, rather than mixing
//! already-8-bit-expanded channels directly, which would round differently
//! `(behavioral-fidelity)`.
//!
//! `EVA`/`EVB`/`EVY` are 5-bit register fields (`0..=31`) but hardware caps
//! every one of them at 16 (100%) before using it — `mgba`'s
//! `video-software.c:325-344` clamps `blda`/`bldb`/`bldy` to `0x10` right
//! when the register is written; values `17..=31` behave identically to 16.

use crate::palette::{compress_8_to_5, expand_5_to_8, Rgb888};

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

/// Mix one already-expanded 8-bit channel the way `mColorMix5Bit` mixes one
/// 5-bit channel: `(a5*weight_a + b5*weight_b) / 16`, saturating at 31
/// (5-bit max) before re-expanding to 8 bits. `weight_a`/`weight_b` are
/// assumed already capped (module docs).
#[allow(clippy::cast_possible_truncation)] // `mixed` is clamped to `0..=31` just above the cast.
const fn mix_channel(a: u8, b: u8, weight_a: u32, weight_b: u32) -> u8 {
    let a5 = compress_8_to_5(a) as u32;
    let b5 = compress_8_to_5(b) as u32;
    let mixed = (a5 * weight_a + b5 * weight_b) / 16;
    let clamped = if mixed > 31 { 31 } else { mixed as u8 };
    expand_5_to_8(clamped)
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

/// Brighten one channel toward white (`_brighten`): `c + (31-c)*y/16`,
/// `y` already capped.
#[allow(clippy::cast_possible_truncation)] // `r` is provably `<=31` (c5<=31, y<=16 => (31-c5)*y/16<=31).
const fn brighten_channel(c: u8, y: u32) -> u8 {
    let c5 = compress_8_to_5(c) as u32;
    let r = c5 + ((31 - c5) * y) / 16;
    let clamped = if r > 31 { 31 } else { r as u8 };
    expand_5_to_8(clamped)
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

/// Darken one channel toward black (`_darken`): `c - c*y/16`, `y` already
/// capped.
#[allow(clippy::cast_possible_truncation)] // c5*y/16 <= c5 <= 31, so the subtraction result is `<=31`.
const fn darken_channel(c: u8, y: u32) -> u8 {
    let c5 = compress_8_to_5(c) as u32;
    let r = c5 - (c5 * y) / 16;
    expand_5_to_8(r as u8)
}

/// Darken `color` toward black by `evy` sixteenths — `BLDY`'s darken mode
/// (`_darken`), `evy` capped at 16.
#[must_use]
pub const fn darken(color: Rgb888, evy: u8) -> Rgb888 {
    let y = cap_weight(evy);
    Rgb888 {
        r: darken_channel(color.r, y),
        g: darken_channel(color.g, y),
        b: darken_channel(color.b, y),
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
#[must_use]
pub fn resolve_pixel_color(
    cfg: &EffectsConfig,
    effects_enabled: bool,
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
        return match next {
            Some((next_color, next_kind)) if cfg.target2.contains(next_kind) => {
                alpha_blend(front_color, next_color, cfg.eva, cfg.evb)
            }
            None if cfg.target2.backdrop => alpha_blend(front_color, backdrop, cfg.eva, cfg.evb),
            Some(_) | None => front_color,
        };
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
        // eva=8, evb=8: (a*8+b*8)/16 == (a+b)/2 in 5-bit space.
        // 5-bit 0 and 5-bit 31 (channel bytes 0 and 255): (0*8+31*8)/16 = 15.
        let first = Rgb888 { r: 0, g: 0, b: 0 };
        let second = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        let blended = alpha_blend(first, second, 8, 8);
        assert_eq!(
            blended.r,
            crate::palette::Bgr555::from_channels(15, 0, 0)
                .to_rgb888()
                .r
        );
    }

    #[test]
    fn alpha_blend_full_weight_on_first_is_identity() {
        // Both colors must actually round-trip through a 5-bit channel
        // (module docs) -- use real palette-derived colors, not arbitrary
        // 8-bit values.
        let first = crate::palette::Bgr555::from_channels(12, 20, 31).to_rgb888();
        let second = crate::palette::Bgr555::from_channels(1, 2, 3).to_rgb888();
        assert_eq!(alpha_blend(first, second, 16, 0), first);
    }

    #[test]
    fn alpha_blend_saturates_when_weights_overflow() {
        // eva=16, evb=16 (both capped at 16, not their raw values if larger):
        // (31*16 + 31*16)/16 = 62, saturating to 31 (full white channel).
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
        // evy=16 (full): 0 + (31-0)*16/16 = 31 -> channel 255.
        assert_eq!(brighten(color, 16).r, 255);
        // evy=8 (half): 0 + 31*8/16 = 15.
        assert_eq!(
            brighten(color, 8).r,
            crate::palette::Bgr555::from_channels(15, 0, 0)
                .to_rgb888()
                .r
        );
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
        // evy=16 (full): 31 - 31*16/16 = 0 -> channel 0.
        assert_eq!(darken(color, 16).r, 0);
        // evy=0: unchanged.
        assert_eq!(darken(color, 0), color);
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
        let result = resolve_pixel_color(&cfg, true, front, next, Rgb888::BLACK);
        assert_eq!(
            result.r,
            crate::palette::Bgr555::from_channels(15, 0, 0)
                .to_rgb888()
                .r
        );
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
        let result = resolve_pixel_color(&cfg, true, front, None, backdrop);
        assert_eq!(
            result.r,
            crate::palette::Bgr555::from_channels(15, 0, 0)
                .to_rgb888()
                .r
        );
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
        let result = resolve_pixel_color(&cfg, true, front, next, Rgb888::BLACK);
        assert_eq!(
            result.r,
            crate::palette::Bgr555::from_channels(15, 0, 0)
                .to_rgb888()
                .r,
            "semi-transparency must force alpha blend regardless of the selected effect"
        );
    }

    #[test]
    fn resolve_semi_transparent_obj_with_no_target2_stays_unblended() {
        let cfg = EffectsConfig::default();
        let front_color = Rgb888 { r: 7, g: 7, b: 7 };
        let front = (front_color, LayerKind::Obj, true);
        let result = resolve_pixel_color(&cfg, true, front, None, Rgb888::BLACK);
        assert_eq!(result, front_color);
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
        let result = resolve_pixel_color(&cfg, true, front, None, Rgb888::BLACK);
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
        let result = resolve_pixel_color(&cfg, true, front, None, Rgb888::BLACK);
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
        let result = resolve_pixel_color(&cfg, false, front, None, Rgb888::BLACK);
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
        let result = resolve_pixel_color(&cfg, true, front, None, backdrop);
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
        let result = resolve_pixel_color(&cfg, true, front, None, backdrop);
        assert_eq!(result, backdrop);
    }
}
