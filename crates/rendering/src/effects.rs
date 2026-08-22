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
//! truncate a second time. `mColorMix5Bit` and `_brighten` are unaffected: a
//! shifted lane's numerator is an exact multiple of the lane's shift, so the
//! mask has nothing left to discard. `_darken` is the exception — red
//! computes `c - (c * y) / 16`, while green and blue lose their remainder to
//! the mask instead of to the division and so compute
//! `c - ceil(c * y / 16)`, one darker wherever `c * y` is not a multiple of
//! 16. `_darken(0xFFFFFF, 7)` is `(144, 143, 143)`, not a uniform 144. Hence
//! the split into `darken_red_channel` and `darken_shifted_channel` below,
//! pinned lane by lane by `darken_matches_the_32bit_mgba_oracle`
//! `(behavioral-fidelity)`.
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

    /// mGBA's 32-bit `_darken` (`software-private.h:270-278`), transcribed
    /// lane-by-lane from the C so this file's expectations never route
    /// through the production helpers they are checking.
    fn mgba_darken(color: u32, y: u32) -> u32 {
        let mut c = 0;
        let a = color & 0x0000_00FF;
        c |= (a - (a * y) / 16) & 0x0000_00FF;
        let a = color & 0x0000_FF00;
        c |= (a - (a * y) / 16) & 0x0000_FF00;
        let a = color & 0x00FF_0000;
        c |= (a - (a * y) / 16) & 0x00FF_0000;
        c
    }

    /// mGBA's 32-bit `_brighten` (`software-private.h:237-245`), transcribed
    /// the same way.
    fn mgba_brighten(color: u32, y: u32) -> u32 {
        let mut c = 0;
        let a = color & 0x0000_00FF;
        c |= (a + ((0x0000_00FF - a) * y) / 16) & 0x0000_00FF;
        let a = color & 0x0000_FF00;
        c |= (a + ((0x0000_FF00 - a) * y) / 16) & 0x0000_FF00;
        let a = color & 0x00FF_0000;
        c |= (a + ((0x00FF_0000 - a) * y) / 16) & 0x00FF_0000;
        c
    }

    /// Pack an [`Rgb888`] into mGBA's non-`COLOR_16_BIT` `mColor` layout:
    /// red in the low lane, then green, then blue (`image.h:37-39`).
    fn pack(color: Rgb888) -> u32 {
        u32::from(color.r) | (u32::from(color.g) << 8) | (u32::from(color.b) << 16)
    }

    fn unpack(packed: u32) -> Rgb888 {
        Rgb888 {
            r: u8::try_from(packed & 0xFF).expect("masked to one byte"),
            g: u8::try_from((packed >> 8) & 0xFF).expect("masked to one byte"),
            b: u8::try_from((packed >> 16) & 0xFF).expect("masked to one byte"),
        }
    }

    #[test]
    fn darken_matches_the_32bit_mgba_oracle() {
        // Every color the compositor can ever darken is a palette entry
        // expanded by `Bgr555::to_rgb888`, so sweep all 32 5-bit values in
        // each of the three channels independently against every usable
        // `EVY` (0..=16 after capping). Each lane is compared separately, so
        // the red/green/blue rounding asymmetry documented in the module docs
        // cannot be satisfied by a single uniform per-channel formula.
        for r5 in 0..32 {
            for g5 in 0..32 {
                for b5 in 0..32 {
                    let color = crate::palette::Bgr555::from_channels(r5, g5, b5).to_rgb888();
                    for evy in 0..=16u8 {
                        let expected = unpack(mgba_darken(pack(color), u32::from(evy)));
                        assert_eq!(
                            darken(color, evy),
                            expected,
                            "darken({color:?}, {evy}) must match mGBA's 32-bit _darken"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn brighten_matches_the_32bit_mgba_oracle() {
        for r5 in 0..32 {
            for g5 in 0..32 {
                for b5 in 0..32 {
                    let color = crate::palette::Bgr555::from_channels(r5, g5, b5).to_rgb888();
                    for evy in 0..=16u8 {
                        let expected = unpack(mgba_brighten(pack(color), u32::from(evy)));
                        assert_eq!(
                            brighten(color, evy),
                            expected,
                            "brighten({color:?}, {evy}) must match mGBA's 32-bit _brighten"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn darken_rounds_shifted_lanes_one_step_darker_than_red() {
        // The hand-checked witness from the module docs: mGBA's
        // `_darken(0xFFFFFF, 7)` is 0x8F8F90, i.e. red 144 but green and blue
        // 143, because 255*7 = 1785 is not a multiple of 16 and only the low
        // lane's remainder is absorbed by the `/ 16`.
        let white = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        assert_eq!(mgba_darken(0x00FF_FFFF, 7), 0x008F_8F90);
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
