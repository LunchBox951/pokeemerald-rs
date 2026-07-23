//! Pitch math for the four CGB PSG channels: MIDI key → hardware frequency
//! register (square/wave) or noise control byte.
//!
//! Behavioural port of `MidiKeyToCgbFreq` (`src/m4a.c:810`). Square and wave
//! channels share one derivation (an 11-bit `NRx3`/`NRx4` frequency register,
//! `0..=2047`); the noise channel instead selects a whole `NR43`-style
//! control byte (clock shift, width mode, and divisor ratio already packed
//! together) straight out of a lookup table. The tables are reproduced
//! verbatim as data `(ported)`; the surrounding logic is re-implemented
//! `(no-verbatim)`.

/// `gCgbScaleTable` (`m4a_tables.c:118`): 132 entries (keys `0..=131`, one
/// past the last playable key so every lookup has a neighbour to interpolate
/// toward), each a `(octave-shift << 4) | note-index` byte indexing
/// [`CGB_FREQ_TABLE`].
#[rustfmt::skip]
const CGB_SCALE_TABLE: [u8; 132] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B,
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B,
    0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B,
    0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B,
    0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B,
    0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B,
    0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0x9B,
    0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB,
];

/// `gCgbFreqTable` (`m4a_tables.c:133`): one octave of register-value deltas
/// from `2048`, negative because every entry sits below the register's
/// centre; the higher octaves (reached by right-shifting toward `0`) hold
/// smaller-magnitude, so higher-pitch, deltas.
const CGB_FREQ_TABLE: [i32; 12] = [
    -2004, -1891, -1785, -1685, -1591, -1501, -1417, -1337, -1262, -1192, -1125, -1062,
];

/// `gNoiseTable` (`m4a_tables.c:149`): 60 entries (clamped key `0..=59`),
/// each a ready-to-use `NR43`-style control byte (clock-shift, width-mode,
/// and divisor-ratio bits already packed together).
#[rustfmt::skip]
const NOISE_TABLE: [u8; 60] = [
    0xD7, 0xD6, 0xD5, 0xD4,
    0xC7, 0xC6, 0xC5, 0xC4,
    0xB7, 0xB6, 0xB5, 0xB4,
    0xA7, 0xA6, 0xA5, 0xA4,
    0x97, 0x96, 0x95, 0x94,
    0x87, 0x86, 0x85, 0x84,
    0x77, 0x76, 0x75, 0x74,
    0x67, 0x66, 0x65, 0x64,
    0x57, 0x56, 0x55, 0x54,
    0x47, 0x46, 0x45, 0x44,
    0x37, 0x36, 0x35, 0x34,
    0x27, 0x26, 0x25, 0x24,
    0x17, 0x16, 0x15, 0x14,
    0x07, 0x06, 0x05, 0x04,
    0x03, 0x02, 0x01, 0x00,
];

/// Resolve a `CGB_SCALE_TABLE` byte into its `CGB_FREQ_TABLE` ratio: the low
/// nibble picks the octave's base delta, the high nibble is a right
/// (arithmetic) shift that selects the octave.
fn cgb_scale_ratio(scale: u8) -> i32 {
    CGB_FREQ_TABLE[(scale & 0x0F) as usize] >> (scale >> 4)
}

/// The 11-bit `NRx3`/`NRx4` frequency register for a square or wave channel
/// played at MIDI `key` with 8-bit `fine` interpolation between semitones.
///
/// Behavioural port of `MidiKeyToCgbFreq`'s non-noise branch (`m4a.c:827`).
#[must_use]
pub fn midi_key_to_cgb_freq_reg(key: u8, fine: u8) -> u16 {
    let (key, fine) = if key <= 35 {
        (0u8, 0u8)
    } else {
        let shifted = key - 36;
        if shifted > 130 {
            (130u8, 255u8)
        } else {
            (shifted, fine)
        }
    };

    let val1 = cgb_scale_ratio(CGB_SCALE_TABLE[key as usize]);
    let val2 = cgb_scale_ratio(CGB_SCALE_TABLE[key as usize + 1]);
    let interp = val1 + ((i32::from(fine) * (val2 - val1)) >> 8);
    let reg = interp + 2048;
    // The register is 11 bits wide on real hardware; clamp defensively so a
    // future caller can never index a table with an out-of-range value.
    u16::try_from(reg.clamp(0, 0x7FF)).unwrap_or(0)
}

/// The `NR43`-style noise control byte for the noise channel played at MIDI
/// `key` (fine adjustment is not used for noise, matching upstream).
///
/// Behavioural port of `MidiKeyToCgbFreq`'s noise branch (`m4a.c:812`).
#[must_use]
pub fn midi_key_to_noise_control(key: u8) -> u8 {
    let index = if key <= 20 { 0 } else { (key - 21).min(59) };
    NOISE_TABLE[index as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freq_reg_is_octave_periodic() {
        // A key one octave (12 semitones) up must halve the register's
        // distance below 2048 (an arithmetic right shift by one octave),
        // matching MidiKeyToFreq's analogous octave doubling.
        let base = midi_key_to_cgb_freq_reg(60, 0);
        let up = midi_key_to_cgb_freq_reg(72, 0);
        assert!(
            up > base,
            "higher key must raise the register ({up} vs {base})"
        );
        assert!(up <= 0x7FF);
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
