//! Pitch math for the DirectSound mixer: the MIDI-key → playback-frequency
//! lookup and the fixed-point resampling step derived from it.
//!
//! Ported behaviourally from `pokeemerald`'s `MidiKeyToFreq` (`src/m4a.c:23`)
//! and `SampleFreqSet` (`src/m4a.c:400`) plus the mixer's inner step
//! (`src/m4a_1.s`, `SoundMainRAM` — the `mul r4, divFreq, frequency` /
//! `add r9, r9, r4` pair). The tables are reproduced verbatim as data
//! `(ported)`; the surrounding logic is re-implemented `(no-verbatim)`.

/// The PCM render rate the whole crate mixes at, in Hz.
///
/// Upstream selects `SOUND_MODE_FREQ_13379` (`m4a.c:79`, `:395`); the
/// derivation of the concrete `13379` value lives in
/// `platform::AudioOutput::M4A_MIXER_RATE`, which this must equal (a unit test
/// in `lib.rs` pins the two together). Kept as a local constant so the crate
/// does not take a non-dev dependency on `platform`.
pub const MIXER_RATE: u32 = 13_379;

/// Samples the hardware renders per V-blank frame at [`MIXER_RATE`].
///
/// `gPcmSamplesPerVBlankTable[SOUND_MODE_FREQ_13379 - 1]` = `224`
/// (`m4a_tables.c:107`). The envelope advances exactly once per frame, so this
/// is also the granularity at which per-sample volume is held constant.
pub const SAMPLES_PER_FRAME: usize = 224;

/// The `.23` fixed-point scale used by the resampler's phase accumulator.
///
/// The mixer keeps a per-voice fractional read position `fw` where the low 23
/// bits are the fraction (`bic r9, r9, 0x3F800000` keeps bits 0..=22) and the
/// remaining bits are whole source samples to advance
/// (`movs lr, r9, lsr 23`).
pub const FRAC_BITS: u32 = 23;

const FRAC_ONE: u32 = 1 << FRAC_BITS;

/// Mask selecting the fractional part of a `.23` phase accumulator.
pub const FRAC_MASK: u32 = FRAC_ONE - 1;

/// `divFreq` for [`MIXER_RATE`]: the per-output-sample multiplier applied to a
/// voice's frequency to obtain its `.23` phase step.
///
/// `SampleFreqSet` computes `pcmFreq = (597275 * 224 + 5000) / 10000 = 13379`
/// then `divFreq = (16777216 / pcmFreq + 1) >> 1` (`m4a.c:410`, `:413`)
/// `= (16777216 / 13379 + 1) >> 1 = (1254 + 1) >> 1 = 627`.
pub const DIV_FREQ: u32 = 627;

/// High 32 bits of the 64-bit product `a * b`.
///
/// Behavioural port of `umul3232H32` (`m4a_1.s:9`, an `umull` returning the
/// high word), used by [`midi_key_to_freq`].
#[must_use]
pub fn umul3232_hi(a: u32, b: u32) -> u32 {
    // Widening to u64 and taking the high word is exactly `umull … ; mov r0,
    // r3`; no truncation is lost because the shift discards only the low 32
    // bits that the hardware also discards.
    #[allow(clippy::cast_possible_truncation)]
    {
        ((u64::from(a) * u64::from(b)) >> 32) as u32
    }
}

/// Table mapping a clamped MIDI key to a `(octave-shift << 4) | note-index`
/// byte; `gScaleTable` (`m4a_tables.c:67`), 180 entries (keys `0..=179`).
#[rustfmt::skip]
const SCALE_TABLE: [u8; 180] = [
    0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xEB,
    0xD0, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xDB,
    0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB,
    0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB,
    0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB,
    0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0x9B,
    0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B,
    0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B,
    0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B,
    0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B,
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
];

/// One octave of `Q32` frequency ratios; `gFreqTable` (`m4a_tables.c:86`).
#[rustfmt::skip]
const FREQ_TABLE: [u32; 12] = [
    2_147_483_648, 2_275_179_671, 2_410_468_894, 2_553_802_834,
    2_705_659_852, 2_866_546_760, 3_037_000_500, 3_217_589_947,
    3_408_917_802, 3_611_622_603, 3_826_380_858, 4_053_909_305,
];

/// Resolve a `SCALE_TABLE` byte into its `gFreqTable` ratio: the low nibble
/// picks the note within the octave, the high nibble is a right shift that
/// selects the octave.
fn scale_ratio(scale: u8) -> u32 {
    FREQ_TABLE[(scale & 0x0F) as usize] >> (scale >> 4)
}

/// Playback frequency for a wave played at MIDI `key` with 8-bit `fine`
/// interpolation between semitones.
///
/// Behavioural port of `MidiKeyToFreq` (`m4a.c:23`). `wav_freq` is the wave's
/// pre-scaled base constant (`WaveData::freq`). The result is the voice's
/// `frequency`, later multiplied by [`DIV_FREQ`] to get the `.23` phase step.
#[must_use]
pub fn midi_key_to_freq(wav_freq: u32, key: u8, fine: u8) -> u32 {
    let mut key = key;
    let mut fine_shifted = u32::from(fine) << 24;
    if key > 178 {
        key = 178;
        fine_shifted = 255 << 24;
    }

    let val1 = scale_ratio(SCALE_TABLE[key as usize]);
    let val2 = scale_ratio(SCALE_TABLE[key as usize + 1]);

    // `val2 - val1` is an unsigned wrap in the original `u32` arithmetic; the
    // interpolation `val1 + umul3232H32(val2 - val1, fine)` reproduces that.
    let interp = val1.wrapping_add(umul3232_hi(val2.wrapping_sub(val1), fine_shifted));
    umul3232_hi(wav_freq, interp)
}

/// The `.23` fixed-point source-read step for one output sample of a voice at
/// `frequency`. `step / 2^23` source samples are consumed per output sample.
///
/// `mul r4, r12, r1` (`r12 = divFreq`, `r1 = frequency`) is a 32-bit multiply,
/// so this wraps like the hardware.
#[must_use]
pub fn phase_step(frequency: u32) -> u32 {
    DIV_FREQ.wrapping_mul(frequency)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div_freq_matches_upstream_derivation() {
        // pcmFreq = (597275 * 224 + 5000) / 10000
        let samples = u32::try_from(SAMPLES_PER_FRAME).unwrap();
        let pcm_freq = (597_275 * samples + 5_000) / 10_000;
        assert_eq!(pcm_freq, MIXER_RATE);
        let div = (16_777_216 / pcm_freq + 1) >> 1;
        assert_eq!(div, DIV_FREQ);
    }

    #[test]
    fn umul_high_word_matches_manual_widening() {
        assert_eq!(umul3232_hi(0, 12345), 0);
        assert_eq!(umul3232_hi(u32::MAX, 2), 1);
        assert_eq!(umul3232_hi(0x8000_0000, 0x8000_0000), 0x4000_0000);
    }

    #[test]
    fn midi_key_to_freq_is_octave_periodic() {
        // A key exactly one octave (12 semitones) up doubles the frequency,
        // because the scale byte's high nibble drops by one (one fewer right
        // shift of the same base ratio).
        let base = midi_key_to_freq(1 << 20, 60, 0);
        let octave_up = midi_key_to_freq(1 << 20, 72, 0);
        assert_eq!(octave_up, base * 2);
    }

    #[test]
    fn midi_key_to_freq_clamps_high_keys() {
        // Any key above 178 collapses to the clamped key with full fine.
        let clamped = midi_key_to_freq(1 << 20, 178, 255);
        assert_eq!(midi_key_to_freq(1 << 20, 200, 7), clamped);
        assert_eq!(midi_key_to_freq(1 << 20, 255, 200), clamped);
    }

    #[test]
    fn fine_adjust_interpolates_between_semitones() {
        let low = midi_key_to_freq(1 << 20, 60, 0);
        let high = midi_key_to_freq(1 << 20, 61, 0);
        let mid = midi_key_to_freq(1 << 20, 60, 128);
        assert!(low < mid && mid < high, "{low} < {mid} < {high}");
    }

    #[test]
    fn phase_step_scales_with_frequency() {
        assert_eq!(phase_step(0), 0);
        assert_eq!(phase_step(1000), DIV_FREQ * 1000);
    }
}
