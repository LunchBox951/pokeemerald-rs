//! Front-end normal CPU palette fades (S-2).
//!
//! Models `BeginNormalPaletteFade` and `UpdatePaletteFade`'s `NORMAL_FADE`
//! scheduling, fixed to `PALETTES_ALL`, zero delay, coefficient 0 to 16, and
//! a black or white target (`pokeemerald/src/palette.c:156-199` for
//! `BeginNormalPaletteFade`, `:406-490` for `UpdateNormalPaletteFade`,
//! `:807-828` for `IsSoftwarePaletteFadeFinishing`). The blend itself is
//! `BlendPalette`'s per-channel signed delta, `base + (((target - base) *
//! coeff) >> 4)` (`pokeemerald/src/util.c:264-278`), applied directly to the
//! retained composed [`Framebuffer`]'s 8-bit RGB channels rather than to a
//! raw 5-bit palette-RAM buffer: upstream's `PlttData` channels are 5-bit,
//! but this primitive's only retained state is the crate's 8-bit
//! [`Framebuffer`], and rounding the blend through a 5-bit round-trip would
//! destroy precision the composited frame actually has (an alpha-blended
//! pixel is not generally 5-bit-aligned) and would make coefficient 0 a
//! lossy no-op instead of the identity upstream's own coefficient-0 blend
//! is. Scaling `BlendPalette`'s formula to the 8-bit domain (target 0 or
//! 255 instead of 0 or 0x1F) keeps coefficient 0 the identity and
//! coefficient 16 exactly the target, with the same rounding direction.
//!
//! This is a CPU blend, not a hardware register effect, and it is
//! implemented independently of [`crate::effects::brighten`]/
//! [`crate::effects::darken`], which model the GBA's `BLDY` packed-lane
//! rounding for the color-special-effect hardware path. Blending toward
//! white is mathematically the same non-negative floor-divide-by-16
//! weighted lerp `brighten` uses (both floor a non-negative product, so
//! they agree exactly); blending toward black diverges from `darken`'s
//! green/blue channels specifically, because those round the fractional
//! remainder up (`div_ceil`) while this blend (like `darken`'s own red
//! channel) rounds down via the arithmetic right shift.
//!
//! Known representation limit: upstream blends separate BG and OBJ palette
//! banks on alternating calls, so for one call at a time the two banks can
//! sit at different coefficients before the next call catches the other up,
//! and that blend happens before layer composition and any hardware color
//! effect. This primitive instead blends the single already-composed
//! [`Framebuffer`] uniformly at the current coefficient on every call — the
//! BG/OBJ alternation still governs exactly when that coefficient steps and
//! when finishing is polled, matched call-for-call against upstream
//! (see [`NormalPaletteFade`]'s tests), but the crate has no per-layer
//! palette buffer to blend separately, and no scene wires this primitive in
//! yet. Reproducing hardware's exact BG/OBJ pixel staggering, or ordering
//! this blend correctly against a simultaneously active hardware color
//! effect, is a concern for whichever caller later composes a scene through
//! this fade, not for this standalone primitive.
//!
//! Out of scope for this primitive: palette masks other than all, nonzero
//! delay, other start/target coefficients, `FAST_FADE`/`HARDWARE_FADE`,
//! palette-buffer mutation, grayscale/inversion, every other `palette.c`
//! caller, and all flow-level wiring (holding a scene through the fade,
//! dispatching once it completes).

use crate::framebuffer::Framebuffer;
use crate::palette::Rgb888;

/// The blend coefficient upstream calls `deltaY`: the fixed per-pair step
/// (`pokeemerald/src/palette.c:167`).
const DELTA_Y: u8 = 2;

/// The fixed target coefficient for this slice's contract (`targetY` in
/// `BeginNormalPaletteFade(PALETTES_ALL, 0, 0, 16, ...)`).
const TARGET_Y: u8 = 16;

/// The number of `ACTIVE` finishing polls `IsSoftwarePaletteFadeFinishing`
/// reports before the fifth call reports done
/// (`pokeemerald/src/palette.c:807-828`).
const FINISHING_POLLS: u8 = 4;

/// The two blend targets the front-end contract uses: `RGB_WHITEALPHA` for
/// the title-to-menu fade and `RGB_BLACK` for the menu-confirm fade
/// (`pokeemerald/include/constants/rgb.h:15-23`). Both have identical red,
/// green, and blue channels, so only that shared channel value is needed;
/// `RGB_WHITEALPHA`'s alpha bit lies outside `BlendPalette`'s per-channel
/// `PlttData` view and never reaches the blend
/// (`pokeemerald/include/gba/types.h:47-53`). Expressed here at the
/// retained [`Framebuffer`]'s 8-bit precision (0 or 255), not upstream's
/// native 5-bit palette precision (0 or 0x1F) — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteFadeTarget {
    /// Fades toward `RGB_BLACK`.
    Black,
    /// Fades toward `RGB_WHITEALPHA`.
    White,
}

impl PaletteFadeTarget {
    /// The shared 8-bit red/green/blue channel value this target blends
    /// toward.
    const fn channel(self) -> u8 {
        match self {
            Self::Black => 0,
            Self::White => u8::MAX,
        }
    }
}

/// The `PALETTE_FADE_STATUS_*` result of one [`NormalPaletteFade::update`]
/// call, narrowed to the two values this slice's `NORMAL_FADE` contract can
/// reach: `PALETTE_FADE_STATUS_ACTIVE` and `PALETTE_FADE_STATUS_DONE`
/// (`pokeemerald/include/palette.h:11-14`). `PALETTE_FADE_STATUS_DELAY` and
/// `PALETTE_FADE_STATUS_LOADING` never occur here because delay is always
/// zero and there is no hardware palette-RAM transfer to await.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteFadeStatus {
    /// The fade is still running; call `update` again.
    Active,
    /// The fade has finished; its final frame is fully blended.
    Done,
}

/// Which palette half upstream's scheduler processes on the next call:
/// background palettes, then object palettes, mirroring
/// `gPaletteFade.objPaletteToggle` (`pokeemerald/src/palette.c:430-454`).
/// `PALETTES_ALL` selects every bank in both halves, so both halves blend
/// with the same current coefficient; only the alternation cadence (and,
/// once both halves of a pair have run, the coefficient step) is
/// observable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Half {
    Background,
    Object,
}

/// A normal CPU palette fade over a retained composed [`Framebuffer`].
///
/// Construct with [`NormalPaletteFade::begin`], which performs the
/// immediate first update `BeginNormalPaletteFade` runs before returning
/// (`pokeemerald/src/palette.c:189`), then advance with
/// [`NormalPaletteFade::update`] once per frame until it reports
/// [`PaletteFadeStatus::Done`].
#[derive(Debug, Clone)]
pub struct NormalPaletteFade {
    /// The unfaded composed frame every update blends from, mirroring
    /// `gPlttBufferUnfaded` (`pokeemerald/src/util.c:269-274`).
    source: Framebuffer,
    /// The current faded frame, mirroring `gPlttBufferFaded`.
    output: Framebuffer,
    target: PaletteFadeTarget,
    /// The current blend coefficient, `gPaletteFade.y`.
    coefficient: u8,
    /// The half the next processing call handles.
    next_half: Half,
    /// Set once the coefficient reaches [`TARGET_Y`], mirroring
    /// `gPaletteFade.softwareFadeFinishing`.
    finishing: bool,
    /// Mirrors `gPaletteFade.softwareFadeFinishingCounter`.
    finishing_counter: u8,
    /// Mirrors `gPaletteFade.active`.
    active: bool,
}

impl NormalPaletteFade {
    /// Begin a normal palette fade of `framebuffer` toward `target`,
    /// equivalent to `BeginNormalPaletteFade(PALETTES_ALL, 0, 0, 16,
    /// blendColor)` (`pokeemerald/src/palette.c:156-199`).
    ///
    /// Performs the immediate first update before returning, exactly as
    /// upstream's `BeginNormalPaletteFade` calls `UpdatePaletteFade` once
    /// before it returns (`pokeemerald/src/palette.c:189`). Because the
    /// blend's coefficient starts at 0 and 0 is this blend's identity (see
    /// the module docs), that immediate call leaves `framebuffer`'s pixels
    /// unchanged.
    #[must_use]
    pub fn begin(framebuffer: Framebuffer, target: PaletteFadeTarget) -> Self {
        let mut fade = Self {
            source: framebuffer.clone(),
            output: framebuffer,
            target,
            coefficient: 0,
            next_half: Half::Background,
            finishing: false,
            finishing_counter: 0,
            active: true,
        };
        fade.advance();
        fade
    }

    /// Advance the fade by one call, equivalent to one `UpdatePaletteFade`
    /// call while `gPaletteFade.mode == NORMAL_FADE`
    /// (`pokeemerald/src/palette.c:114-131, 406-490`).
    pub fn update(&mut self) -> PaletteFadeStatus {
        self.advance()
    }

    /// The current faded frame, ready for presentation.
    #[must_use]
    pub fn framebuffer(&self) -> &Framebuffer {
        &self.output
    }

    /// Consume the fade, returning its current (or final) frame.
    #[must_use]
    pub fn into_framebuffer(self) -> Framebuffer {
        self.output
    }

    /// Whether the fade has finished (`!gPaletteFade.active`).
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

    /// `UpdateNormalPaletteFade`'s per-call palette-half processing
    /// (`pokeemerald/src/palette.c:420-486`): blend at the current
    /// coefficient, then, only once both halves of the pair have run, arm
    /// finishing at the target coefficient or step toward it.
    fn process_half(&mut self) {
        self.output = blend_framebuffer(&self.source, self.target, self.coefficient);
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

    /// `IsSoftwarePaletteFadeFinishing` (`pokeemerald/src/palette.c:807-828`):
    /// reports `ACTIVE` for [`FINISHING_POLLS`] calls, then clears `active`
    /// and reports `DONE` on the next.
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

/// `BlendPalette`'s per-channel signed delta
/// (`pokeemerald/src/util.c:264-278`), applied directly to every 8-bit
/// pixel of `source` (see the module docs for why no 5-bit round-trip
/// happens here).
fn blend_framebuffer(source: &Framebuffer, target: PaletteFadeTarget, coeff: u8) -> Framebuffer {
    let mut blended = source.clone();
    for y in 0..source.height() {
        for x in 0..source.width() {
            let Some(pixel) = source.pixel(x, y) else {
                continue;
            };
            blended.set_pixel(x, y, blend_pixel(pixel, target, coeff));
        }
    }
    blended
}

fn blend_pixel(pixel: Rgb888, target: PaletteFadeTarget, coeff: u8) -> Rgb888 {
    let target_channel = target.channel();
    Rgb888 {
        r: blend_channel(pixel.r, target_channel, coeff),
        g: blend_channel(pixel.g, target_channel, coeff),
        b: blend_channel(pixel.b, target_channel, coeff),
    }
}

/// `base + (((target - base) * coeff) >> 4)` (`pokeemerald/src/util.c:271-
/// 277`), scaled to 8-bit channels (`base`/`target` in `0..=255`) instead of
/// upstream's native 5-bit palette channels, so it applies directly to the
/// retained [`Framebuffer`]'s `Rgb888` pixels (see the module docs). `coeff`
/// in `0..=16` keeps the result in `0..=255`: at `coeff == 0` the shifted
/// term is 0 (identity), and at `coeff == 16` `(target - base) * 16 >> 4 ==
/// target - base` exactly, so the result is exactly `target`; every
/// intermediate coefficient interpolates monotonically between the two.
#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "base, target, and coeff's ranges keep the signed result in 0..=255"
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
    use crate::framebuffer::Framebuffer;
    use crate::palette::Rgb888;

    fn framebuffer_of(color: Rgb888) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(color);
        fb
    }

    fn gray(v: u8) -> Rgb888 {
        Rgb888 { r: v, g: v, b: v }
    }

    // --- begin/update status trace: BG/OBJ cadence, coefficient steps, four
    // finishing polls, done (pokeemerald/src/palette.c:156-199, 406-490,
    // 807-828). ---

    #[test]
    fn begin_performs_the_immediate_first_background_update() {
        let fade = NormalPaletteFade::begin(framebuffer_of(gray(20)), PaletteFadeTarget::White);
        // The immediate call inside `begin` processes the background half at
        // coefficient 0, then hands off to the object half without moving
        // the coefficient yet.
        assert_eq!(fade.next_half, Half::Object);
        assert_eq!(fade.coefficient, 0);
        assert!(!fade.is_done());
    }

    #[test]
    fn begin_at_coefficient_zero_is_the_identity_even_for_a_non_palette_aligned_value() {
        // The reviewer's failing-test case: an effect-produced composited
        // value (e.g. an alpha-blend output) is not generally aligned to a
        // 5-bit palette grid. Coefficient 0 must leave it untouched exactly
        // the way upstream's own coefficient-0 BlendPalette call leaves an
        // unfaded palette entry untouched, rather than rounding it onto the
        // nearest 5-bit value.
        let fade = NormalPaletteFade::begin(
            framebuffer_of(Rgb888 { r: 127, g: 0, b: 0 }),
            PaletteFadeTarget::White,
        );
        assert_eq!(
            fade.framebuffer().pixel(0, 0),
            Some(Rgb888 { r: 127, g: 0, b: 0 })
        );
    }

    #[test]
    fn update_alternates_background_and_object_halves_before_stepping_the_coefficient() {
        let mut fade = NormalPaletteFade::begin(framebuffer_of(gray(0)), PaletteFadeTarget::White);
        // begin() already ran the first (background) half.
        let mut halves_and_coefficients = vec![(fade.next_half, fade.coefficient)];
        for _ in 0..17 {
            fade.update();
            halves_and_coefficients.push((fade.next_half, fade.coefficient));
        }
        let expected: Vec<(Half, u8)> = [
            (Half::Object, 0),
            (Half::Background, 2),
            (Half::Object, 2),
            (Half::Background, 4),
            (Half::Object, 4),
            (Half::Background, 6),
            (Half::Object, 6),
            (Half::Background, 8),
            (Half::Object, 8),
            (Half::Background, 10),
            (Half::Object, 10),
            (Half::Background, 12),
            (Half::Object, 12),
            (Half::Background, 14),
            (Half::Object, 14),
            (Half::Background, 16),
            (Half::Object, 16),
            (Half::Background, 16),
        ]
        .to_vec();
        assert_eq!(halves_and_coefficients, expected);
        assert_eq!(TARGET_Y, 16);
        assert_eq!(DELTA_Y, 2);
    }

    #[test]
    fn full_trace_from_begin_is_22_actives_then_done() {
        let mut fade = NormalPaletteFade::begin(framebuffer_of(gray(10)), PaletteFadeTarget::Black);
        // `begin` already consumed the first of the 22 active calls.
        let mut statuses = Vec::new();
        loop {
            let status = fade.update();
            let done = status == PaletteFadeStatus::Done;
            statuses.push(status);
            if done {
                break;
            }
        }
        // `begin` already ran effective call 1 (the immediate background
        // update); the 22 calls collected here are effective calls 2..=23:
        // 21 active, then done on the 22nd.
        assert_eq!(statuses.len(), 22);
        assert!(statuses[..21]
            .iter()
            .all(|&s| s == PaletteFadeStatus::Active));
        assert_eq!(statuses[21], PaletteFadeStatus::Done);
        assert!(fade.is_done());
    }

    #[test]
    fn updates_after_done_stay_done() {
        let mut fade = NormalPaletteFade::begin(framebuffer_of(gray(0)), PaletteFadeTarget::White);
        while fade.update() != PaletteFadeStatus::Done {}
        assert_eq!(fade.update(), PaletteFadeStatus::Done);
        assert_eq!(fade.update(), PaletteFadeStatus::Done);
    }

    #[test]
    fn finished_fade_reaches_the_exact_target_color() {
        let mut fade = NormalPaletteFade::begin(
            framebuffer_of(Rgb888 { r: 10, g: 20, b: 5 }),
            PaletteFadeTarget::White,
        );
        while fade.update() != PaletteFadeStatus::Done {}
        assert!(fade.framebuffer().pixels().iter().all(|&p| p
            == Rgb888 {
                r: 255,
                g: 255,
                b: 255
            }));

        let mut fade = NormalPaletteFade::begin(
            framebuffer_of(Rgb888 { r: 10, g: 20, b: 5 }),
            PaletteFadeTarget::Black,
        );
        while fade.update() != PaletteFadeStatus::Done {}
        assert!(fade
            .framebuffer()
            .pixels()
            .iter()
            .all(|&p| p == Rgb888 { r: 0, g: 0, b: 0 }));
    }

    #[test]
    fn source_is_retained_so_repeated_coefficients_do_not_compound() {
        // The background and object halves of one pair both blend at the
        // *same* coefficient from the retained source, so the framebuffer
        // must be identical after each half of a pair, never doubled.
        let mut fade = NormalPaletteFade::begin(framebuffer_of(gray(16)), PaletteFadeTarget::White);
        let after_background = fade.framebuffer().pixel(0, 0);
        fade.update(); // the paired object half, same coefficient
        let after_object = fade.framebuffer().pixel(0, 0);
        assert_eq!(after_background, after_object);
    }

    #[test]
    fn into_framebuffer_returns_the_current_frame() {
        let fade = NormalPaletteFade::begin(framebuffer_of(gray(1)), PaletteFadeTarget::Black);
        let expected = fade.framebuffer().clone();
        let fb = fade.into_framebuffer();
        assert_eq!(fb.pixels(), expected.pixels());
    }

    // --- BlendPalette oracle: white and black targets, representative and
    // boundary 8-bit channels, proven apart from effects::{brighten, darken}
    // (pokeemerald/src/util.c:264-278). ---

    #[test]
    fn blend_channel_matches_the_blend_palette_oracle_toward_white() {
        const WHITE: u8 = u8::MAX;
        let cases: [(u8, u8, u8); 30] = [
            (0, 0, 0),
            (0, 2, 31),
            (0, 4, 63),
            (0, 8, 127),
            (0, 16, 255),
            (1, 0, 1),
            (1, 2, 32),
            (1, 4, 64),
            (1, 8, 128),
            (1, 16, 255),
            (127, 0, 127),
            (127, 2, 143),
            (127, 4, 159),
            (127, 8, 191),
            (127, 16, 255),
            (128, 0, 128),
            (128, 2, 143),
            (128, 4, 159),
            (128, 8, 191),
            (128, 16, 255),
            (254, 0, 254),
            (254, 2, 254),
            (254, 4, 254),
            (254, 8, 254),
            (254, 16, 255),
            (255, 0, 255),
            (255, 2, 255),
            (255, 4, 255),
            (255, 8, 255),
            (255, 16, 255),
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
            (127, 0, 127),
            (127, 2, 111),
            (127, 4, 95),
            (127, 8, 63),
            (127, 16, 0),
            (128, 0, 128),
            (128, 2, 112),
            (128, 4, 96),
            (128, 8, 64),
            (128, 16, 0),
            (254, 0, 254),
            (254, 2, 222),
            (254, 4, 190),
            (254, 8, 127),
            (254, 16, 0),
            (255, 0, 255),
            (255, 2, 223),
            (255, 4, 191),
            (255, 8, 127),
            (255, 16, 0),
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
        // base=20, target=0, coeff=1: delta=-20, -20>>4 == -2 (not -1, which
        // truncation-toward-zero would give), so 20 + -2 == 18.
        assert_eq!(blend_channel(20, 0, 1), 18);
        // base=1, target=0, coeff=2: delta=-2, -2>>4 == -1, so 1 + -1 == 0.
        assert_eq!(blend_channel(1, 0, 2), 0);
    }

    #[test]
    fn expanded_rgb888_oracle_for_a_mixed_representative_pixel() {
        // A mixed-channel, non-5-bit-aligned pixel through `blend_pixel`,
        // with expected literals computed independently from the
        // per-channel oracle above (base channels 15, 200, 1 at coeff 8).
        let pixel = Rgb888 {
            r: 15,
            g: 200,
            b: 1,
        };

        let white = blend_pixel(pixel, PaletteFadeTarget::White, 8);
        assert_eq!(
            white,
            Rgb888 {
                r: blend_channel(15, 255, 8),
                g: blend_channel(200, 255, 8),
                b: blend_channel(1, 255, 8),
            }
        );
        assert_eq!(
            white,
            Rgb888 {
                r: 135,
                g: 227,
                b: 128
            }
        );

        let black = blend_pixel(pixel, PaletteFadeTarget::Black, 8);
        assert_eq!(
            black,
            Rgb888 {
                r: blend_channel(15, 0, 8),
                g: blend_channel(200, 0, 8),
                b: blend_channel(1, 0, 8),
            }
        );
        assert_eq!(black, Rgb888 { r: 7, g: 100, b: 0 });
    }

    // --- independent of effects::{brighten, darken}: this blend is computed
    // by its own implementation, never by delegating to the hardware
    // BLDY-effect path (pokeemerald/src/util.c:264-278). ---

    #[test]
    fn cpu_darken_toward_black_genuinely_differs_from_the_hardware_bldy_darken() {
        // Darken's green/blue channels round their fractional remainder up
        // (`div_ceil`) while this blend rounds down via the arithmetic right
        // shift, so at a coefficient/channel pair with a fractional
        // intermediate (255*2/16 = 31.875) the two disagree: the CPU blend
        // stays uniform across channels, while the hardware path's
        // shifted-lane rounding splits red from green/blue.
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
                r: 223,
                g: 223,
                b: 223
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
    fn cpu_blend_toward_white_is_computed_independently_of_brighten() {
        // Unlike darken, brighten's weighted lerp toward white is the same
        // non-negative floor-divide-by-16 formula this blend uses toward
        // white (both floor a non-negative product), so the two agree
        // numerically here -- this pins that the palette-fade blend is its
        // own implementation (not a call into `effects::brighten`) that
        // happens to compute upstream's same documented formula
        // (BlendPalette and the BLDALPHA/BLDY hardware path share one
        // per-channel weighted-lerp-over-16 shape), rather than asserting a
        // divergence that would not exist for this target.
        use crate::effects::brighten;
        let base = gray(200);
        let cpu = blend_pixel(base, PaletteFadeTarget::White, 7);
        let hardware = brighten(base, 7);
        assert_eq!(cpu, hardware);
        assert_eq!(cpu, gray(224));
    }
}
