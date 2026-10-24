//! MIDI note-velocity quantization.
//!
//! The top bucket clamps to 127; all other nonzero values round up to a
//! multiple of four, matching `tools/mid2agb/tables.cpp:124-254`.
pub(super) const NOTE_VELOCITY_LUT: [u8; 128] = [
    0, 4, 4, 4, 4, 8, 8, 8, 8, 12, 12, 12, 12, 16, 16, 16, 16, 20, 20, 20, 20, 24, 24, 24, 24, 28,
    28, 28, 28, 32, 32, 32, 32, 36, 36, 36, 36, 40, 40, 40, 40, 44, 44, 44, 44, 48, 48, 48, 48, 52,
    52, 52, 52, 56, 56, 56, 56, 60, 60, 60, 60, 64, 64, 64, 64, 68, 68, 68, 68, 72, 72, 72, 72, 76,
    76, 76, 76, 80, 80, 80, 80, 84, 84, 84, 84, 88, 88, 88, 88, 92, 92, 92, 92, 96, 96, 96, 96,
    100, 100, 100, 100, 104, 104, 104, 104, 108, 108, 108, 108, 112, 112, 112, 112, 116, 116, 116,
    116, 120, 120, 120, 120, 124, 124, 124, 124, 127, 127, 127,
];

#[cfg(test)]
mod tests {
    use super::NOTE_VELOCITY_LUT;

    #[test]
    fn quantizing_an_already_quantized_velocity_is_idempotent() {
        for &v in &NOTE_VELOCITY_LUT {
            assert_eq!(NOTE_VELOCITY_LUT[usize::from(v)], v);
        }
    }

    #[test]
    fn endpoints_and_a_few_hand_checked_values() {
        assert_eq!(NOTE_VELOCITY_LUT[0], 0);
        assert_eq!(NOTE_VELOCITY_LUT[1], 4);
        assert_eq!(NOTE_VELOCITY_LUT[93], 96);
        assert_eq!(NOTE_VELOCITY_LUT[112], 112);
        assert_eq!(NOTE_VELOCITY_LUT[120], 120);
        assert_eq!(NOTE_VELOCITY_LUT[124], 124);
        assert_eq!(NOTE_VELOCITY_LUT[125], 127);
        assert_eq!(NOTE_VELOCITY_LUT[127], 127);
    }
}
