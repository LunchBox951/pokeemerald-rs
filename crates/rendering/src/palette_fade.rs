//! Front-end normal CPU palette fades (S-2).
//!
//! Models `BeginNormalPaletteFade` and `UpdatePaletteFade`'s `NORMAL_FADE`
//! scheduling, fixed to `PALETTES_ALL`, zero delay, coefficient 0 to 16, and
//! a black or white target (`pokeemerald/src/palette.c:156-199` for
//! `BeginNormalPaletteFade`, `:406-490` for `UpdateNormalPaletteFade`,
//! `:807-828` for `IsSoftwarePaletteFadeFinishing`). The blend itself is
//! `BlendPalette`'s per-channel signed delta, `base + (((target - base) *
//! coeff) >> 4)`, computed over 5-bit `PlttData` channels
//! (`pokeemerald/src/util.c:264-278`), upstream's native palette-RAM
//! precision: a nonzero coefficient compresses the retained composed
//! [`Framebuffer`]'s 8-bit channel to 5 bits, blends, and expands the result
//! back (`crate::palette::compress_8_to_5`/`expand_5_to_8`). Coefficient 0
//! is the identity on the 8-bit channel directly, without that round-trip,
//! because upstream's own coefficient-0 `BlendPalette` call leaves an
//! unfaded palette entry untouched rather than rounding it onto the nearest
//! 5-bit value, and this primitive's only retained state is the crate's
//! 8-bit [`Framebuffer`], which can hold precision (an alpha-blended pixel
//! is not generally 5-bit-aligned) that upstream never had to represent at
//! coefficient 0. Coefficient 16 lands exactly on the target.
//!
//! This is a CPU blend, not a hardware register effect, and it is
//! implemented independently of [`crate::effects::brighten`]/
//! [`crate::effects::darken`], which model the GBA's `BLDY` packed-lane
//! rounding for the color-special-effect hardware path at full 8-bit
//! precision; both diverge from this blend's 5-bit round-trip once the
//! coefficient is nonzero (see this module's tests).
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
//! palette buffer to blend separately. Reproducing hardware's exact BG/OBJ
//! pixel staggering, or ordering this blend against a simultaneously active
//! hardware color effect, is outside this primitive's representation.
//!
//! Out of scope for this primitive: palette masks other than all, nonzero
//! delay, other start/target coefficients, `FAST_FADE`/`HARDWARE_FADE`,
//! palette-buffer mutation, grayscale/inversion, every other `palette.c`
//! caller, and all flow-level wiring (holding a scene through the fade,
//! dispatching once it completes).

use crate::framebuffer::Framebuffer;
use crate::palette::{compress_8_to_5, expand_5_to_8, Rgb888};

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
/// (`pokeemerald/include/gba/types.h:47-53`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteFadeTarget {
    /// Fades toward `RGB_BLACK`.
    Black,
    /// Fades toward `RGB_WHITEALPHA`.
    White,
}

impl PaletteFadeTarget {
    /// The shared 5-bit red/green/blue channel value this target blends
    /// toward, at `BlendPalette`'s native `PlttData` precision.
    const fn channel(self) -> u8 {
        match self {
            Self::Black => 0,
            Self::White => 0x1F,
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
/// (`pokeemerald/src/util.c:264-278`), applied to every pixel of `source`
/// (see the module docs for the coefficient-0 identity and the 5-bit
/// round-trip every nonzero coefficient blends through).
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

/// Blends one pixel, matching `BlendPalette`'s own coefficient-0 shortcut:
/// the caller never darkens/lightens the unfaded buffer at `y == 0`
/// (`pokeemerald/src/util.c:269-274`), so this returns `pixel` untouched
/// rather than rounding it through the 5-bit compress/expand round-trip
/// every nonzero coefficient uses.
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

/// `base + (((target - base) * coeff) >> 4)` (`pokeemerald/src/util.c:271-
/// 277`), on 5-bit `PlttData` channels. `coeff` in `0..=16` and
/// `base`/`target` in `0..=0x1F` keep the result in `0..=0x1F`, matching
/// upstream's unclamped math.
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
    use crate::framebuffer::Framebuffer;
    use crate::palette::{Bgr555, Rgb888};

    fn framebuffer_of(color: Rgb888) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(color);
        fb
    }

    fn gray(v: u8) -> Rgb888 {
        Rgb888 { r: v, g: v, b: v }
    }

    /// A pixel built from 5-bit BGR555 channels, exactly representable at
    /// `BlendPalette`'s native precision (unlike an arbitrary 8-bit
    /// [`Rgb888`] literal).
    fn pixel5(r: u8, g: u8, b: u8) -> Rgb888 {
        Bgr555::from_channels(r, g, b).to_rgb888()
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
        // An effect-produced composited value (e.g. an alpha-blend output)
        // is not generally aligned to a 5-bit palette grid. Coefficient 0
        // must leave it untouched exactly the way upstream's own
        // coefficient-0 BlendPalette call leaves an unfaded palette entry
        // untouched, rather than rounding it onto the nearest 5-bit value.
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
        // base=20, target=0, coeff=1: delta=-20, -20>>4 == -2 (not -1, which
        // truncation-toward-zero would give), so 20 + -2 == 18.
        assert_eq!(blend_channel(20, 0, 1), 18);
        // base=1, target=0, coeff=2: delta=-2, -2>>4 == -1, so 1 + -1 == 0.
        assert_eq!(blend_channel(1, 0, 2), 0);
    }

    #[test]
    fn palette_derived_pixel_fades_like_blend_palette_in_5_bit_domain() {
        // Black toward white at coefficient 2: BlendPalette's native 5-bit
        // math gives five-bit 3 (0 + ((0x1F - 0) * 2) >> 4 == 3), displayed
        // through the crate's 5-to-8-bit expansion as 24, not the 31 an
        // 8-bit-domain blend would give.
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
    fn expanded_rgb888_oracle_for_a_mixed_representative_pixel() {
        // A mixed-channel pixel exactly representable in 5-bit palette RAM,
        // through the full compress/blend/expand `blend_pixel` uses, with
        // expected literals computed independently from the per-channel
        // oracle above (base channels 15, 30, 1 at coeff 8: white blends to
        // 5-bit 23/30/16, black to 7/15/0).
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

    // --- independent of effects::{brighten, darken}: this blend is computed
    // by its own implementation, never by delegating to the hardware
    // BLDY-effect path, and its 5-bit round-trip genuinely diverges from
    // their full 8-bit precision (pokeemerald/src/util.c:264-278). ---

    #[test]
    fn cpu_darken_toward_black_genuinely_differs_from_the_hardware_bldy_darken() {
        // 5-bit boundary channel 31 (== 255 expanded) darkened at
        // coefficient 2: the CPU blend keeps every channel uniform at 222,
        // while the hardware BLDY path's shifted-lane rounding splits red
        // from green/blue.
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
    fn cpu_brighten_toward_white_genuinely_differs_from_the_hardware_bldy_brighten() {
        // 5-bit boundary channel 30 (== 247 expanded) brightened at
        // coefficient 8: the CPU blend stays at 247 (30 blended toward 31 at
        // coeff 8 rounds down to 30, the identity), while the hardware
        // path's full 8-bit distance-to-white weighting moves it further.
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
