//! [`SongEvent`]: this pipeline's own copy of
//! `crates/assets::audio::song::SongEvent`'s shape — duplicated, not
//! imported, for the same reason `xtask::extract::voicegroups::resolve`'s
//! `VoiceSlot` duplicates `crates/assets::audio::voicegroup::VoiceEntry`: this
//! crate never depends on `crates/assets` (`pack_format`'s module
//! docs). [`super::encode`] serializes this to that schema's exact wire
//! shape.
//!
//! Only the variants [`super::compile`] can actually produce are here —
//! `MemAcc`/`MemAccBranch` are deliberately absent (see `super`'s module
//! docs, "Deliberately out of scope: `MEMACC`"); the wire format still
//! reserves their tag bytes (`super::encode`'s docs), this compiler simply
//! never writes them.

/// One normalized track command — mirrors
/// `crates/assets::audio::song::SongEvent`'s variants (minus `MemAcc`/
/// `MemAccBranch`; see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SongEvent {
    Wait(u8),
    Note {
        key: u8,
        velocity: u8,
        gate: u8,
    },
    /// Always carries an explicit key: this compiler never elides it the
    /// way `tools/mid2agb`'s own note/velocity-name compression can
    /// (`agb.cpp:222-244`'s `PrintEndOfTieOp`) -- see `super::compile`'s
    /// module docs, "No operand elision".
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
    /// An event-index target into this same track's own event list (see
    /// `crates/assets::audio::song`'s module docs, "Looping").
    Goto(u32),
    Fine,
}
