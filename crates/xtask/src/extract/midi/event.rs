//! Normalized MIDI commands ready for pack encoding.
//!
//! Extraction keeps its own model because `assets` is optional;
//! [`super::encode`] maps it to the shared wire schema.

/// One normalized track command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SongEvent {
    Wait(u8),
    Note {
        key: u8,
        velocity: u8,
        gate: u8,
    },
    /// Always carries a key; this representation does not elide operands.
    EndOfTie {
        key: u8,
    },
    Voice(u8),
    Volume(u8),
    Pan(i8),
    Bend(i8),
    BendRange(u8),
    Tune(i8),
    KeyShift(i8),
    Tempo(u16),
    Priority(u8),
    LfoSpeed(u8),
    LfoDelay(u8),
    Modulation(u8),
    ModType(u8),
    PseudoEchoVolume(u8),
    PseudoEchoLength(u8),
    /// Targets an event index in the same track.
    Goto(u32),
    Fine,
}
