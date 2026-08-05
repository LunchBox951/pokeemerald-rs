//! The note-velocity quantization table `tools/mid2agb` applies to every
//! note's raw MIDI velocity.
//!
//! # Why a table, not a formula
//!
//! `no-verbatim` blesses translating a table of constants (it is data, not
//! control flow), so [`NOTE_VELOCITY_LUT`] is transcribed directly from
//! `tools/mid2agb/tables.cpp:124-254`'s `g_noteVelocityLUT` rather than
//! reverse-engineered into a formula: the table is *almost* "round up to the
//! next multiple of 4", but the top three entries (indices `125..=127`) clamp
//! to `127` instead of the formula's `128`, since a MIDI velocity byte only
//! goes up to `127`. Keeping the literal table sidesteps having to
//! special-case that clamp.
//!
//! # Where this applies, and where it doesn't
//!
//! `tools/mid2agb/midi.cpp:639`'s `ConvertTimes` applies this table once,
//! unconditionally, to every note's velocity — regardless of `-E`/`-X`/`-N`.
//! `agb.cpp:149`'s `PrintNote` applies it *again* to that already-quantized
//! value, but this is a no-op in practice: every value this table ever
//! *produces* is already one of its own fixed points (each output value's
//! own index re-maps to itself — e.g. `NOTE_VELOCITY_LUT[4] == 4`,
//! `NOTE_VELOCITY_LUT[124] == 124`, `NOTE_VELOCITY_LUT[127] == 127`), so a
//! second application changes nothing. This compiler therefore applies the
//! table exactly once (in [`super::compile`]'s note handling), matching the
//! *effective* upstream behaviour without reproducing the redundant second
//! lookup.
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
    fn every_entry_is_its_own_fixed_point() {
        // Pins the module docs' "apply once, not twice" claim: re-indexing
        // by any table *output* must return that same output.
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
