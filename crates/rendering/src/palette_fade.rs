//! Front-end normal CPU palette fades (S-2).
//!
//! Models `NORMAL_FADE` as `BeginNormalPaletteFade` and `UpdatePaletteFade`
//! run it for `PALETTES_ALL`: zero delay, coefficient 0 to 16, black or white
//! target. The fade drives palette state, not pixels, so it applies to
//! palette colours before composition and any hardware color effect.
//! [`NormalPaletteFade`] owns the scheduling contract and
//! [`PaletteFadeBlend`] the blend.
//!
//! Out of scope: other palette masks, delay, start/target coefficients,
//! `FAST_FADE`/`HARDWARE_FADE`, palette-buffer transfers, and flow wiring.

use crate::compositor::{PaletteColorTransform, PaletteStage};
use crate::palette::{compress_8_to_5, expand_5_to_8, Rgb888};

const DELTA_Y: u8 = 2;

const TARGET_Y: u8 = 16;

const FINISHING_POLLS: u8 = 4;

/// The blend target: `RGB_BLACK` or `RGB_WHITEALPHA`
/// (`pokeemerald/include/constants/rgb.h:15-23`). The alpha bit lies outside
/// the 5-bit channels the blend sees (`pokeemerald/include/gba/types.h:47-53`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteFadeTarget {
    /// Fades toward `RGB_BLACK`.
    Black,
    /// Fades toward `RGB_WHITEALPHA`.
    White,
}

impl PaletteFadeTarget {
    const fn channel(self) -> u8 {
        match self {
            Self::Black => 0,
            Self::White => 0x1F,
        }
    }
}

/// The result of one [`NormalPaletteFade::update`]: the only
/// `PALETTE_FADE_STATUS_*` values reachable with zero delay and no palette
/// transfer (`pokeemerald/include/palette.h:11-14`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteFadeStatus {
    /// The fade is still running; call `update` again.
    Active,
    /// The fade has finished; both halves rest at the target.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Half {
    Background,
    Object,
}

/// One palette half's target and coefficient (0 unfaded to 16 at the target).
///
/// As a [`PaletteColorTransform`] it applies `BlendPalette`'s signed 5-bit
/// per-channel blend (`pokeemerald/src/util.c:264-278`): a nonzero
/// coefficient compresses each RGB888 channel to 5 bits, blends, and expands
/// back; coefficient 0 returns the colour exactly; 16 lands on the target.
/// This is independent of the hardware `BLDY` transforms
/// ([`crate::effects::brighten`], [`crate::effects::darken`]), which round
/// at 8-bit precision and so diverge once the coefficient is nonzero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaletteFadeBlend {
    target: PaletteFadeTarget,
    coefficient: u8,
}

impl PaletteFadeBlend {
    const fn new(target: PaletteFadeTarget) -> Self {
        Self {
            target,
            coefficient: 0,
        }
    }

    /// The colour this blend fades toward.
    #[must_use]
    pub const fn target(self) -> PaletteFadeTarget {
        self.target
    }

    /// The blend coefficient, 0 (unfaded) to 16 (the target).
    #[must_use]
    pub const fn coefficient(self) -> u8 {
        self.coefficient
    }
}

impl PaletteColorTransform for PaletteFadeBlend {
    fn transform(&self, color: Rgb888) -> Rgb888 {
        blend_pixel(color, self.target, self.coefficient)
    }
}

/// A normal CPU palette fade driving independent BG and OBJ palette blends.
///
/// Upstream blends BG then OBJ on alternating calls at each coefficient
/// 0, 2, ..., 16, stepping only after both halves ran
/// (`pokeemerald/src/palette.c:167,432-473`), so between calls the two blends
/// can sit at different coefficients.
#[derive(Debug, Clone)]
pub struct NormalPaletteFade {
    bg: PaletteFadeBlend,
    obj: PaletteFadeBlend,
    coefficient: u8,
    next_half: Half,
    finishing: bool,
    finishing_counter: u8,
    active: bool,
}

impl NormalPaletteFade {
    /// Begin a fade toward `target`, as
    /// `BeginNormalPaletteFade(PALETTES_ALL, 0, 0, 16, blendColor)`
    /// (`pokeemerald/src/palette.c:156-199`). Like upstream it updates once
    /// before returning (`:189`): BG is blended at coefficient 0 and OBJ is next.
    #[must_use]
    pub fn begin(target: PaletteFadeTarget) -> Self {
        let blend = PaletteFadeBlend::new(target);
        let mut fade = Self {
            bg: blend,
            obj: blend,
            coefficient: 0,
            next_half: Half::Background,
            finishing: false,
            finishing_counter: 0,
            active: true,
        };
        fade.advance();
        fade
    }

    /// Advance by one call per frame. After the target pair, four finishing
    /// calls report `Active` and the fifth `Done`, which then stays `Done`
    /// (`pokeemerald/src/palette.c:411-416,807-822`).
    pub fn update(&mut self) -> PaletteFadeStatus {
        self.advance()
    }

    /// The BG half's current blend.
    #[must_use]
    pub const fn bg_blend(&self) -> PaletteFadeBlend {
        self.bg
    }

    /// The OBJ half's current blend.
    #[must_use]
    pub const fn obj_blend(&self) -> PaletteFadeBlend {
        self.obj
    }

    /// The current blends as the compositor's palette stage, applied to
    /// palette colours before composition and any hardware color effect.
    #[must_use]
    pub fn palette_stage(&self) -> PaletteStage<'_> {
        PaletteStage {
            bg: Some(&self.bg),
            obj: Some(&self.obj),
        }
    }

    /// Whether the fade has finished.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        !self.active
    }

    fn advance(&mut self) -> PaletteFadeStatus {
        if !self.active {
            return PaletteFadeStatus::Done;
        }
        if self.finishing {
            return self.poll_finishing();
        }
        self.process_half();
        PaletteFadeStatus::Active
    }

    fn process_half(&mut self) {
        let half = match self.next_half {
            Half::Background => &mut self.bg,
            Half::Object => &mut self.obj,
        };
        half.coefficient = self.coefficient;
        self.next_half = match self.next_half {
            Half::Background => Half::Object,
            Half::Object => {
                if self.coefficient == TARGET_Y {
                    self.finishing = true;
                } else {
                    self.coefficient = self.coefficient.saturating_add(DELTA_Y).min(TARGET_Y);
                }
                Half::Background
            }
        };
    }

    fn poll_finishing(&mut self) -> PaletteFadeStatus {
        if self.finishing_counter == FINISHING_POLLS {
            self.active = false;
            self.finishing = false;
            self.finishing_counter = 0;
            PaletteFadeStatus::Done
        } else {
            self.finishing_counter += 1;
            PaletteFadeStatus::Active
        }
    }
}

fn blend_pixel(pixel: Rgb888, target: PaletteFadeTarget, coeff: u8) -> Rgb888 {
    if coeff == 0 {
        return pixel;
    }
    let target_channel = target.channel();
    Rgb888 {
        r: expand_5_to_8(blend_channel(
            compress_8_to_5(pixel.r),
            target_channel,
            coeff,
        )),
        g: expand_5_to_8(blend_channel(
            compress_8_to_5(pixel.g),
            target_channel,
            coeff,
        )),
        b: expand_5_to_8(blend_channel(
            compress_8_to_5(pixel.b),
            target_channel,
            coeff,
        )),
    }
}

#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "base, target, and coeff's ranges keep the signed result in 0..=0x1F"
)]
const fn blend_channel(base: u8, target: u8, coeff: u8) -> u8 {
    let base = base as i16;
    let target = target as i16;
    let coeff = coeff as i16;
    let delta = target - base;
    (base + ((delta * coeff) >> 4)) as u8
}

#[cfg(test)]
mod tests {
    use super::{
        blend_channel, blend_pixel, Half, NormalPaletteFade, PaletteFadeStatus, PaletteFadeTarget,
        DELTA_Y, TARGET_Y,
    };
    use crate::compositor::PaletteColorTransform;
    use crate::palette::{Bgr555, Rgb888};

    fn pixel5(r: u8, g: u8, b: u8) -> Rgb888 {
        Bgr555::from_channels(r, g, b).to_rgb888()
    }

    fn bg_obj_coefficients(fade: &NormalPaletteFade) -> (u8, u8) {
        (
            fade.bg_blend().coefficient(),
            fade.obj_blend().coefficient(),
        )
    }

    #[test]
    fn begin_performs_the_immediate_first_background_update() {
        let fade = NormalPaletteFade::begin(PaletteFadeTarget::White);
        assert_eq!(fade.next_half, Half::Object);
        assert_eq!(fade.coefficient, 0);
        assert!(!fade.is_done());
        assert_eq!(bg_obj_coefficients(&fade), (0, 0));
    }

    #[test]
    fn begin_bank_states_are_the_identity_on_palette_colours() {
        for target in [PaletteFadeTarget::White, PaletteFadeTarget::Black] {
            let fade = NormalPaletteFade::begin(target);
            assert_eq!(fade.bg_blend().target(), target);
            assert_eq!(fade.obj_blend().target(), target);
            assert_eq!(bg_obj_coefficients(&fade), (0, 0));
            for r in 0..32 {
                let color = pixel5(r, 31 - r, r / 2);
                assert_eq!(fade.bg_blend().transform(color), color);
                assert_eq!(fade.obj_blend().transform(color), color);
            }
        }
    }

    #[test]
    fn coefficient_zero_leaves_a_non_palette_aligned_colour_untouched() {
        let fade = NormalPaletteFade::begin(PaletteFadeTarget::White);
        let color = Rgb888 { r: 127, g: 0, b: 0 };
        assert_eq!(fade.bg_blend().transform(color), color);
    }

    #[test]
    fn update_alternates_background_and_object_halves_before_stepping_the_coefficient() {
        let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::White);
        let mut trace = vec![(fade.next_half, fade.coefficient, bg_obj_coefficients(&fade))];
        for _ in 0..17 {
            fade.update();
            trace.push((fade.next_half, fade.coefficient, bg_obj_coefficients(&fade)));
        }
        let expected: Vec<(Half, u8, (u8, u8))> = [
            (Half::Object, 0, (0, 0)),
            (Half::Background, 2, (0, 0)),
            (Half::Object, 2, (2, 0)),
            (Half::Background, 4, (2, 2)),
            (Half::Object, 4, (4, 2)),
            (Half::Background, 6, (4, 4)),
            (Half::Object, 6, (6, 4)),
            (Half::Background, 8, (6, 6)),
            (Half::Object, 8, (8, 6)),
            (Half::Background, 10, (8, 8)),
            (Half::Object, 10, (10, 8)),
            (Half::Background, 12, (10, 10)),
            (Half::Object, 12, (12, 10)),
            (Half::Background, 14, (12, 12)),
            (Half::Object, 14, (14, 12)),
            (Half::Background, 16, (14, 14)),
            (Half::Object, 16, (16, 14)),
            (Half::Background, 16, (16, 16)),
        ]
        .to_vec();
        assert_eq!(trace, expected);
        assert_eq!(TARGET_Y, 16);
        assert_eq!(DELTA_Y, 2);
    }

    #[test]
    fn updates_after_begin_are_21_active_calls_then_done() {
        let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::Black);
        let mut statuses = Vec::new();
        loop {
            let status = fade.update();
            let done = status == PaletteFadeStatus::Done;
            statuses.push(status);
            if done {
                break;
            }
        }
        assert_eq!(statuses.len(), 22);
        assert!(statuses[..21]
            .iter()
            .all(|&s| s == PaletteFadeStatus::Active));
        assert_eq!(statuses[21], PaletteFadeStatus::Done);
        assert!(fade.is_done());
    }

    #[test]
    fn four_finishing_updates_are_active_then_done_with_both_halves_at_target() {
        let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::White);
        while !fade.finishing {
            fade.update();
        }
        assert_eq!(bg_obj_coefficients(&fade), (16, 16));
        for _ in 0..4 {
            assert_eq!(fade.update(), PaletteFadeStatus::Active);
            assert_eq!(bg_obj_coefficients(&fade), (16, 16));
        }
        assert_eq!(fade.update(), PaletteFadeStatus::Done);
        assert_eq!(bg_obj_coefficients(&fade), (16, 16));
    }

    #[test]
    fn updates_after_done_stay_done() {
        let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::White);
        while fade.update() != PaletteFadeStatus::Done {}
        assert_eq!(fade.update(), PaletteFadeStatus::Done);
        assert_eq!(fade.update(), PaletteFadeStatus::Done);
        assert_eq!(bg_obj_coefficients(&fade), (16, 16));
    }

    #[test]
    fn finished_bank_states_reach_the_exact_target_colour() {
        for (target, expected) in [
            (
                PaletteFadeTarget::White,
                Rgb888 {
                    r: 255,
                    g: 255,
                    b: 255,
                },
            ),
            (PaletteFadeTarget::Black, Rgb888 { r: 0, g: 0, b: 0 }),
        ] {
            let mut fade = NormalPaletteFade::begin(target);
            while fade.update() != PaletteFadeStatus::Done {}
            for color in [pixel5(1, 4, 0), pixel5(31, 0, 17), Rgb888::BLACK] {
                assert_eq!(fade.bg_blend().transform(color), expected);
                assert_eq!(fade.obj_blend().transform(color), expected);
            }
        }
    }

    #[test]
    fn bank_transforms_are_stateless_and_do_not_compound() {
        let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::White);
        fade.update();
        fade.update();
        assert_eq!(bg_obj_coefficients(&fade), (2, 0));
        let color = pixel5(16, 16, 16);
        let once = fade.bg_blend().transform(color);
        assert_eq!(once, pixel5(17, 17, 17));
        assert_eq!(fade.bg_blend().transform(color), once);
        assert_eq!(fade.obj_blend().transform(color), color);
    }

    #[test]
    fn palette_stage_exposes_the_current_independent_bank_states() {
        let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::White);
        fade.update();
        fade.update();
        let color = pixel5(16, 16, 16);
        let copied_bg_blend = fade.bg_blend();
        {
            let stage = fade.palette_stage();
            assert_eq!(
                stage.bg.map(|t| t.transform(color)),
                Some(fade.bg_blend().transform(color))
            );
            assert_eq!(stage.obj.map(|t| t.transform(color)), Some(color));
        }
        fade.update();
        assert_eq!(bg_obj_coefficients(&fade), (2, 2));
        let stage = fade.palette_stage();
        assert_eq!(
            stage.obj.map(|t| t.transform(color)),
            Some(pixel5(17, 17, 17))
        );
        assert_eq!(copied_bg_blend.coefficient(), 2);
    }

    #[test]
    fn blend_channel_matches_the_blend_palette_oracle_toward_white() {
        const WHITE: u8 = 0x1F;
        let cases: [(u8, u8, u8); 30] = [
            (0, 0, 0),
            (0, 2, 3),
            (0, 4, 7),
            (0, 8, 15),
            (0, 16, 31),
            (1, 0, 1),
            (1, 2, 4),
            (1, 4, 8),
            (1, 8, 16),
            (1, 16, 31),
            (15, 0, 15),
            (15, 2, 17),
            (15, 4, 19),
            (15, 8, 23),
            (15, 16, 31),
            (16, 0, 16),
            (16, 2, 17),
            (16, 4, 19),
            (16, 8, 23),
            (16, 16, 31),
            (30, 0, 30),
            (30, 2, 30),
            (30, 4, 30),
            (30, 8, 30),
            (30, 16, 31),
            (31, 0, 31),
            (31, 2, 31),
            (31, 4, 31),
            (31, 8, 31),
            (31, 16, 31),
        ];
        for (base, coeff, expected) in cases {
            assert_eq!(
                blend_channel(base, WHITE, coeff),
                expected,
                "base={base} coeff={coeff}"
            );
        }
    }

    #[test]
    fn blend_channel_matches_the_blend_palette_oracle_toward_black() {
        const BLACK: u8 = 0;
        let cases: [(u8, u8, u8); 30] = [
            (0, 0, 0),
            (0, 2, 0),
            (0, 4, 0),
            (0, 8, 0),
            (0, 16, 0),
            (1, 0, 1),
            (1, 2, 0),
            (1, 4, 0),
            (1, 8, 0),
            (1, 16, 0),
            (15, 0, 15),
            (15, 2, 13),
            (15, 4, 11),
            (15, 8, 7),
            (15, 16, 0),
            (16, 0, 16),
            (16, 2, 14),
            (16, 4, 12),
            (16, 8, 8),
            (16, 16, 0),
            (30, 0, 30),
            (30, 2, 26),
            (30, 4, 22),
            (30, 8, 15),
            (30, 16, 0),
            (31, 0, 31),
            (31, 2, 27),
            (31, 4, 23),
            (31, 8, 15),
            (31, 16, 0),
        ];
        for (base, coeff, expected) in cases {
            assert_eq!(
                blend_channel(base, BLACK, coeff),
                expected,
                "base={base} coeff={coeff}"
            );
        }
    }

    #[test]
    fn negative_deltas_round_toward_negative_infinity_like_the_c_arithmetic_shift() {
        assert_eq!(blend_channel(20, 0, 1), 18);
        assert_eq!(blend_channel(1, 0, 2), 0);
    }

    #[test]
    fn black_toward_white_at_coefficient_two_expands_to_rgb888_24() {
        let faded = blend_pixel(Rgb888::BLACK, PaletteFadeTarget::White, 2);
        assert_eq!(
            faded,
            Rgb888 {
                r: 24,
                g: 24,
                b: 24
            }
        );
    }

    #[test]
    fn mixed_palette_pixel_blends_to_expected_rgb888_for_both_targets() {
        let pixel = pixel5(15, 30, 1);

        let white = blend_pixel(pixel, PaletteFadeTarget::White, 8);
        assert_eq!(
            white,
            Rgb888 {
                r: 189,
                g: 247,
                b: 132
            }
        );

        let black = blend_pixel(pixel, PaletteFadeTarget::Black, 8);
        assert_eq!(
            black,
            Rgb888 {
                r: 57,
                g: 123,
                b: 0
            }
        );
    }

    #[test]
    fn cpu_black_fade_differs_from_hardware_bldy_darken() {
        use crate::effects::darken;
        let white = Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        };
        let cpu = blend_pixel(white, PaletteFadeTarget::Black, 2);
        assert_eq!(
            cpu,
            Rgb888 {
                r: 222,
                g: 222,
                b: 222
            }
        );
        let hardware = darken(white, 2);
        assert_eq!(
            hardware,
            Rgb888 {
                r: 224,
                g: 223,
                b: 223
            }
        );
        assert_ne!(cpu, hardware);
    }

    #[test]
    fn cpu_white_fade_differs_from_hardware_bldy_brighten() {
        use crate::effects::brighten;
        let base = pixel5(30, 30, 30);
        let cpu = blend_pixel(base, PaletteFadeTarget::White, 8);
        assert_eq!(
            cpu,
            Rgb888 {
                r: 247,
                g: 247,
                b: 247
            }
        );
        let hardware = brighten(base, 8);
        assert_eq!(
            hardware,
            Rgb888 {
                r: 251,
                g: 251,
                b: 251
            }
        );
        assert_ne!(cpu, hardware);
    }
}
