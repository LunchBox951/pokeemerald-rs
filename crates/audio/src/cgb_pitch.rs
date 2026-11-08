//! CGB square, wave, and noise channel pitch lookup.

const PITCHED_KEY_OFFSET: u8 = 36;
const MAX_PITCHED_TABLE_KEY: u8 = 130;
const NOISE_KEY_OFFSET: u8 = 21;
const MAX_NOISE_TABLE_KEY: u8 = 59;
const FINE_SCALE_BITS: u32 = 8;
const FREQUENCY_REGISTER_LIMIT: i32 = 2048;
const FREQUENCY_REGISTER_MAX: i32 = FREQUENCY_REGISTER_LIMIT - 1;
const NOISE_CLOCK_SHIFT_BITS: u32 = 4;

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

/// Semitone and octave shifts for the 132 CGB pitch-table keys.
#[rustfmt::skip]
const CGB_SCALE_TABLE: [PitchScale; 132] = [
    scale(0, 0), scale(1, 0), scale(2, 0), scale(3, 0), scale(4, 0), scale(5, 0), scale(6, 0), scale(7, 0), scale(8, 0), scale(9, 0), scale(10, 0), scale(11, 0),
    scale(0, 1), scale(1, 1), scale(2, 1), scale(3, 1), scale(4, 1), scale(5, 1), scale(6, 1), scale(7, 1), scale(8, 1), scale(9, 1), scale(10, 1), scale(11, 1),
    scale(0, 2), scale(1, 2), scale(2, 2), scale(3, 2), scale(4, 2), scale(5, 2), scale(6, 2), scale(7, 2), scale(8, 2), scale(9, 2), scale(10, 2), scale(11, 2),
    scale(0, 3), scale(1, 3), scale(2, 3), scale(3, 3), scale(4, 3), scale(5, 3), scale(6, 3), scale(7, 3), scale(8, 3), scale(9, 3), scale(10, 3), scale(11, 3),
    scale(0, 4), scale(1, 4), scale(2, 4), scale(3, 4), scale(4, 4), scale(5, 4), scale(6, 4), scale(7, 4), scale(8, 4), scale(9, 4), scale(10, 4), scale(11, 4),
    scale(0, 5), scale(1, 5), scale(2, 5), scale(3, 5), scale(4, 5), scale(5, 5), scale(6, 5), scale(7, 5), scale(8, 5), scale(9, 5), scale(10, 5), scale(11, 5),
    scale(0, 6), scale(1, 6), scale(2, 6), scale(3, 6), scale(4, 6), scale(5, 6), scale(6, 6), scale(7, 6), scale(8, 6), scale(9, 6), scale(10, 6), scale(11, 6),
    scale(0, 7), scale(1, 7), scale(2, 7), scale(3, 7), scale(4, 7), scale(5, 7), scale(6, 7), scale(7, 7), scale(8, 7), scale(9, 7), scale(10, 7), scale(11, 7),
    scale(0, 8), scale(1, 8), scale(2, 8), scale(3, 8), scale(4, 8), scale(5, 8), scale(6, 8), scale(7, 8), scale(8, 8), scale(9, 8), scale(10, 8), scale(11, 8),
    scale(0, 9), scale(1, 9), scale(2, 9), scale(3, 9), scale(4, 9), scale(5, 9), scale(6, 9), scale(7, 9), scale(8, 9), scale(9, 9), scale(10, 9), scale(11, 9),
    scale(0, 10), scale(1, 10), scale(2, 10), scale(3, 10), scale(4, 10), scale(5, 10), scale(6, 10), scale(7, 10), scale(8, 10), scale(9, 10), scale(10, 10), scale(11, 10),
];

/// One octave of frequency-register deltas from [`FREQUENCY_REGISTER_LIMIT`].
const CGB_FREQ_TABLE: [i32; 12] = [
    -2004, -1891, -1785, -1685, -1591, -1501, -1417, -1337, -1262, -1192, -1125, -1062,
];

const fn noise_control(clock_shift: u8, divisor_code: u8) -> u8 {
    (clock_shift << NOISE_CLOCK_SHIFT_BITS) | divisor_code
}

/// Packed `NR43` clock-shift and divisor codes for the 60 noise-table keys.
#[rustfmt::skip]
const NOISE_TABLE: [u8; 60] = [
    noise_control(13, 7), noise_control(13, 6), noise_control(13, 5), noise_control(13, 4),
    noise_control(12, 7), noise_control(12, 6), noise_control(12, 5), noise_control(12, 4),
    noise_control(11, 7), noise_control(11, 6), noise_control(11, 5), noise_control(11, 4),
    noise_control(10, 7), noise_control(10, 6), noise_control(10, 5), noise_control(10, 4),
    noise_control(9, 7), noise_control(9, 6), noise_control(9, 5), noise_control(9, 4),
    noise_control(8, 7), noise_control(8, 6), noise_control(8, 5), noise_control(8, 4),
    noise_control(7, 7), noise_control(7, 6), noise_control(7, 5), noise_control(7, 4),
    noise_control(6, 7), noise_control(6, 6), noise_control(6, 5), noise_control(6, 4),
    noise_control(5, 7), noise_control(5, 6), noise_control(5, 5), noise_control(5, 4),
    noise_control(4, 7), noise_control(4, 6), noise_control(4, 5), noise_control(4, 4),
    noise_control(3, 7), noise_control(3, 6), noise_control(3, 5), noise_control(3, 4),
    noise_control(2, 7), noise_control(2, 6), noise_control(2, 5), noise_control(2, 4),
    noise_control(1, 7), noise_control(1, 6), noise_control(1, 5), noise_control(1, 4),
    noise_control(0, 7), noise_control(0, 6), noise_control(0, 5), noise_control(0, 4),
    noise_control(0, 3), noise_control(0, 2), noise_control(0, 1), noise_control(0, 0),
];

fn cgb_scale_ratio(scale: PitchScale) -> i32 {
    CGB_FREQ_TABLE[scale.semitone] >> scale.right_shift
}

fn pitched_table_position(key: u8, fine: u8) -> (usize, u8) {
    if key < PITCHED_KEY_OFFSET {
        (0, 0)
    } else {
        let table_key = key - PITCHED_KEY_OFFSET;
        if table_key > MAX_PITCHED_TABLE_KEY {
            (usize::from(MAX_PITCHED_TABLE_KEY), u8::MAX)
        } else {
            (usize::from(table_key), fine)
        }
    }
}

/// Return the square or wave channel's 11-bit frequency-register value.
/// Key clamping and interpolation match `MidiKeyToCgbFreq`
/// (`m4a.c:827..853`).
#[must_use]
pub fn midi_key_to_cgb_freq_reg(key: u8, fine: u8) -> u16 {
    let (table_key, fine) = pitched_table_position(key, fine);
    let lower_delta = cgb_scale_ratio(CGB_SCALE_TABLE[table_key]);
    let upper_delta = cgb_scale_ratio(CGB_SCALE_TABLE[table_key + 1]);
    let interpolated_delta =
        lower_delta + ((i32::from(fine) * (upper_delta - lower_delta)) >> FINE_SCALE_BITS);
    let register = FREQUENCY_REGISTER_LIMIT + interpolated_delta;
    u16::try_from(register.clamp(0, FREQUENCY_REGISTER_MAX)).unwrap_or(0)
}

/// Return the noise channel's packed `NR43` clock and divisor control.
/// Key offset and clamping match `MidiKeyToCgbFreq` (`m4a.c:810..825`).
#[must_use]
pub fn midi_key_to_noise_control(key: u8) -> u8 {
    let table_key = key
        .saturating_sub(NOISE_KEY_OFFSET)
        .min(MAX_NOISE_TABLE_KEY);
    NOISE_TABLE[usize::from(table_key)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freq_reg_is_octave_periodic() {
        let base = midi_key_to_cgb_freq_reg(60, 0);
        let up = midi_key_to_cgb_freq_reg(72, 0);
        assert!(
            up > base,
            "higher key must raise the register ({up} vs {base})"
        );
        assert!(i32::from(up) <= FREQUENCY_REGISTER_MAX);
    }

    #[test]
    fn freq_reg_clamps_low_and_high_keys() {
        let low = midi_key_to_cgb_freq_reg(35, 5);
        assert_eq!(low, midi_key_to_cgb_freq_reg(0, 0));
        let high = midi_key_to_cgb_freq_reg(200, 7);
        assert_eq!(high, midi_key_to_cgb_freq_reg(166, 255));
    }

    #[test]
    fn fine_adjust_interpolates_between_semitones() {
        let low = midi_key_to_cgb_freq_reg(60, 0);
        let high = midi_key_to_cgb_freq_reg(61, 0);
        let mid = midi_key_to_cgb_freq_reg(60, 128);
        assert!(low <= mid && mid <= high, "{low} <= {mid} <= {high}");
    }

    #[test]
    fn noise_control_clamps_and_indexes_the_table() {
        assert_eq!(midi_key_to_noise_control(0), NOISE_TABLE[0]);
        assert_eq!(midi_key_to_noise_control(20), NOISE_TABLE[0]);
        assert_eq!(midi_key_to_noise_control(21), NOISE_TABLE[0]);
        assert_eq!(midi_key_to_noise_control(200), NOISE_TABLE[59]);
    }
}
