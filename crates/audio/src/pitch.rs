//! DirectSound MIDI pitch lookup and fixed-point resampling math.

const LCD_REFRESH_RATE_X_10_000: u32 = 597_275;
const HZ_SCALE: u32 = 10_000;
const PCM_SAMPLES_PER_FRAME: u32 = 224;
const GBA_CPU_FREQUENCY_HZ: u32 = 16_777_216;
const MAX_INTERPOLATED_KEY: u8 = 178;
const FINE_SCALE_BITS: u32 = 24;

const fn sample_rate(samples_per_frame: u32) -> u32 {
    (LCD_REFRESH_RATE_X_10_000 * samples_per_frame + HZ_SCALE / 2) / HZ_SCALE
}

/// The PCM render rate in samples per second.
pub const MIXER_RATE: u32 = sample_rate(PCM_SAMPLES_PER_FRAME);

/// The number of output samples rendered per frame.
pub const SAMPLES_PER_FRAME: usize = PCM_SAMPLES_PER_FRAME as usize;

/// Fractional bits in each source-sample phase position.
pub const FRAC_BITS: u32 = 23;

/// One source sample in fixed-point phase units.
pub const FRAC_ONE: u32 = 1 << FRAC_BITS;

/// Mask for the fractional part of a source-sample phase position.
pub const FRAC_MASK: u32 = FRAC_ONE - 1;

/// Fixed-point phase units per hertz at [`MIXER_RATE`].
pub const DIV_FREQ: u32 = (GBA_CPU_FREQUENCY_HZ / MIXER_RATE).div_ceil(2);

/// Return the high 32 bits of a 32-by-32-bit unsigned product.
#[must_use]
pub fn umul3232_hi(a: u32, b: u32) -> u32 {
    let product = u64::from(a) * u64::from(b);
    u32::try_from(product >> u32::BITS).unwrap_or(u32::MAX)
}

#[derive(Clone, Copy)]
struct PitchScale {
    semitone: usize,
    right_shift: u32,
}

const fn scale(semitone: usize, right_shift: u32) -> PitchScale {
    PitchScale {
        semitone,
        right_shift,
    }
}

/// Semitone and octave shifts for all 180 pitch-table keys.
#[rustfmt::skip]
const SCALE_TABLE: [PitchScale; 180] = [
    scale(0, 14), scale(1, 14), scale(2, 14), scale(3, 14), scale(4, 14), scale(5, 14), scale(6, 14), scale(7, 14), scale(8, 14), scale(9, 14), scale(10, 14), scale(11, 14),
    scale(0, 13), scale(1, 13), scale(2, 13), scale(3, 13), scale(4, 13), scale(5, 13), scale(6, 13), scale(7, 13), scale(8, 13), scale(9, 13), scale(10, 13), scale(11, 13),
    scale(0, 12), scale(1, 12), scale(2, 12), scale(3, 12), scale(4, 12), scale(5, 12), scale(6, 12), scale(7, 12), scale(8, 12), scale(9, 12), scale(10, 12), scale(11, 12),
    scale(0, 11), scale(1, 11), scale(2, 11), scale(3, 11), scale(4, 11), scale(5, 11), scale(6, 11), scale(7, 11), scale(8, 11), scale(9, 11), scale(10, 11), scale(11, 11),
    scale(0, 10), scale(1, 10), scale(2, 10), scale(3, 10), scale(4, 10), scale(5, 10), scale(6, 10), scale(7, 10), scale(8, 10), scale(9, 10), scale(10, 10), scale(11, 10),
    scale(0, 9), scale(1, 9), scale(2, 9), scale(3, 9), scale(4, 9), scale(5, 9), scale(6, 9), scale(7, 9), scale(8, 9), scale(9, 9), scale(10, 9), scale(11, 9),
    scale(0, 8), scale(1, 8), scale(2, 8), scale(3, 8), scale(4, 8), scale(5, 8), scale(6, 8), scale(7, 8), scale(8, 8), scale(9, 8), scale(10, 8), scale(11, 8),
    scale(0, 7), scale(1, 7), scale(2, 7), scale(3, 7), scale(4, 7), scale(5, 7), scale(6, 7), scale(7, 7), scale(8, 7), scale(9, 7), scale(10, 7), scale(11, 7),
    scale(0, 6), scale(1, 6), scale(2, 6), scale(3, 6), scale(4, 6), scale(5, 6), scale(6, 6), scale(7, 6), scale(8, 6), scale(9, 6), scale(10, 6), scale(11, 6),
    scale(0, 5), scale(1, 5), scale(2, 5), scale(3, 5), scale(4, 5), scale(5, 5), scale(6, 5), scale(7, 5), scale(8, 5), scale(9, 5), scale(10, 5), scale(11, 5),
    scale(0, 4), scale(1, 4), scale(2, 4), scale(3, 4), scale(4, 4), scale(5, 4), scale(6, 4), scale(7, 4), scale(8, 4), scale(9, 4), scale(10, 4), scale(11, 4),
    scale(0, 3), scale(1, 3), scale(2, 3), scale(3, 3), scale(4, 3), scale(5, 3), scale(6, 3), scale(7, 3), scale(8, 3), scale(9, 3), scale(10, 3), scale(11, 3),
    scale(0, 2), scale(1, 2), scale(2, 2), scale(3, 2), scale(4, 2), scale(5, 2), scale(6, 2), scale(7, 2), scale(8, 2), scale(9, 2), scale(10, 2), scale(11, 2),
    scale(0, 1), scale(1, 1), scale(2, 1), scale(3, 1), scale(4, 1), scale(5, 1), scale(6, 1), scale(7, 1), scale(8, 1), scale(9, 1), scale(10, 1), scale(11, 1),
    scale(0, 0), scale(1, 0), scale(2, 0), scale(3, 0), scale(4, 0), scale(5, 0), scale(6, 0), scale(7, 0), scale(8, 0), scale(9, 0), scale(10, 0), scale(11, 0),
];

/// One octave of Q32 semitone frequency ratios.
#[rustfmt::skip]
const FREQ_TABLE: [u32; 12] = [
    2_147_483_648, 2_275_179_671, 2_410_468_894, 2_553_802_834,
    2_705_659_852, 2_866_546_760, 3_037_000_500, 3_217_589_947,
    3_408_917_802, 3_611_622_603, 3_826_380_858, 4_053_909_305,
];

fn scale_ratio(scale: PitchScale) -> u32 {
    FREQ_TABLE[scale.semitone] >> scale.right_shift
}

/// Return the playback frequency for a MIDI key and 8-bit fine adjustment.
#[must_use]
pub fn midi_key_to_freq(wav_freq: u32, key: u8, fine: u8) -> u32 {
    let (key, fine) = if key > MAX_INTERPOLATED_KEY {
        (MAX_INTERPOLATED_KEY, u8::MAX)
    } else {
        (key, fine)
    };

    let lower_ratio = scale_ratio(SCALE_TABLE[usize::from(key)]);
    let upper_ratio = scale_ratio(SCALE_TABLE[usize::from(key) + 1]);
    let fine_fraction = u32::from(fine) << FINE_SCALE_BITS;
    let interpolated_ratio = lower_ratio.wrapping_add(umul3232_hi(
        upper_ratio.wrapping_sub(lower_ratio),
        fine_fraction,
    ));
    umul3232_hi(wav_freq, interpolated_ratio)
}

/// Return the wrapping fixed-point source phase step for a frequency.
#[must_use]
pub fn phase_step(frequency: u32) -> u32 {
    DIV_FREQ.wrapping_mul(frequency)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div_freq_matches_upstream_derivation() {
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
        let base = midi_key_to_freq(1 << 20, 60, 0);
        let octave_up = midi_key_to_freq(1 << 20, 72, 0);
        assert_eq!(octave_up, base * 2);
    }

    #[test]
    fn midi_key_to_freq_clamps_high_keys() {
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
