//! [`MidiError`]: every way parsing `mus_title.mid` (or its
//! `sound/songs/midi/midi.cfg` compile-flag entry) can fail. Split out of
//! [`super`] to keep that file under this crate's ~600-line-per-file
//! guideline (`oop-boundaries`), mirroring `super::super::voicegroups::error`'s
//! own split.

use std::fmt;

/// An error compiling a `.mid` source into the normalized [`super::event::SongEvent`]
/// stream, or reading its `midi.cfg` compile-flag entry. The outer
/// `ExtractError::Midi`/`ExtractError::MidiCfg` wrap a value of this type
/// with the offending path (see `super`'s module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MidiError {
    /// The file ran out of bytes where a fixed-size field or declared
    /// chunk/event body was expected.
    Truncated,
    /// Scaling a raw MIDI tick onto this compiler's 24-clocks-per-beat
    /// timebase (`24 * raw / division` — `tools/mid2agb/midi.cpp:635`/`:641`'s
    /// `ConvertTimes`) produced a value a `u32` cannot hold. Upstream runs
    /// that multiply in a `std::uint32_t` and silently wraps; this compiler
    /// evaluates it in `u64` and fails closed instead
    /// ([`super::translate::convert_ticks`]'s docs). Only reachable for a
    /// tick count no real `.mid` file carries (`raw > u32::MAX / 24` at a
    /// small `division`). Carries the offending raw tick.
    TickOverflow(u32),
    /// The compiled song had more playable tracks than the song schema's
    /// `u8` track-count field can describe — this side's mirror of
    /// `crates/assets::audio::AudioError::TooManyTracks`, duplicated the
    /// same way the encoder itself is (`super::encode`'s module docs).
    /// Carries the track count found.
    TooManyTracks(usize),
    /// The file did not start with the 4-byte `MThd` signature.
    BadHeaderMagic,
    /// `MThd`'s declared header length was not the fixed value `6` every
    /// standard MIDI file uses.
    HeaderLengthMismatch(u32),
    /// `MThd`'s format field was `2` or greater (`tools/mid2agb/midi.cpp:145-146`
    /// only accepts format `0`/`1`, and `mus_title.mid` is format `1` — see
    /// `super`'s module docs).
    UnsupportedFormat(u16),
    /// `MThd`'s division field had its top bit set (SMPTE frame-rate
    /// division), which `tools/mid2agb/midi.cpp:152-153` also rejects. This
    /// compiler only understands ticks-per-quarter-note division.
    NegativeDivision(i16),
    /// A chunk this parser expected to be `MTrk` was not.
    BadTrackMagic,
    /// A channel-voice status byte (running or explicit) was not a
    /// recognized MIDI status — either a bare data byte with no running
    /// status in effect, or a channel-voice nibble outside `0x8`..`0xE`
    /// (unreachable in practice: every nibble in that range is a defined
    /// message type).
    InvalidStatusByte(u8),
    /// A channel-voice operand had its status bit set. Standard MIDI data
    /// bytes are restricted to 7 bits (`0..=127`); carries the offending
    /// byte.
    InvalidDataByte(u8),
    /// A tempo (`0xFF 0x51`) meta event's declared length was not the fixed
    /// `3` bytes every tempo event carries
    /// (`tools/mid2agb/midi.cpp:309-310`).
    BadTempoLength(u32),
    /// A tempo (`0xFF 0x51`) meta event declared zero microseconds per
    /// quarter note, which would make the BPM conversion divide by zero.
    ZeroTempo,
    /// A tempo (`0xFF 0x51`) meta event's `round(60_000_000.0f32 /
    /// microseconds)` BPM (`tools/mid2agb/agb.cpp:505-507`) does not fit a
    /// `u16`. Upstream computes it into a 32-bit `int` and never
    /// range-checks it before formatting; this compiler's
    /// [`super::event::SongEvent::Tempo`] field is a `u16`, and a Rust
    /// `as u16` cast on an out-of-range `f32` would saturate to
    /// `u16::MAX` instead of erroring, silently turning every microseconds
    /// value in `1..=915` into the same wrong tempo
    /// ([`super::translate::bpm_from_microseconds`]'s docs). Carries the
    /// offending microseconds-per-quarter-note value.
    TempoOverflow(u32),
    /// A time-signature (`0xFF 0x58`) meta event's declared length was not
    /// the fixed `4` bytes every time-signature event carries
    /// (`tools/mid2agb/midi.cpp:318-319`'s `RaiseError("invalid time
    /// signature size")`).
    BadTimeSignatureLength(u32),
    /// A time-signature (`0xFF 0x58`) meta event's denominator exponent was
    /// `16` or more (`tools/mid2agb/midi.cpp:324-325`'s `RaiseError("invalid
    /// time signature denominator")`). Carries the offending exponent.
    BadTimeSignatureDenominator(u8),
    /// A time-signature meta event's whole-note grid period
    /// (`96 * numerator * clocks_per_beat / denominator` —
    /// `tools/mid2agb/midi.cpp:329-334`) worked out to zero ticks, which
    /// would stall the timing-mark walk (`super::compile`'s `emit_track`).
    /// Upstream's own `timeSig <= 0` guard, reachable via e.g. a `1/128`
    /// signature.
    ZeroTimeSignature,
    /// A `NoteOn` (velocity != 0) on this channel had no later matching
    /// `NoteOff`/`NoteOn`-velocity-`0` for the same key before the track's
    /// `EndOfTrack` — `tools/mid2agb/midi.cpp:425-426`'s own
    /// `RaiseError("note doesn't end")`, translated to a recoverable error
    /// here rather than a process abort. Carries the channel and key.
    UnterminatedNote { channel: u8, key: u8 },
    /// A `LoopEnd`/`LoopEndBegin` text marker (`]`/`][`) appeared with no
    /// preceding `LoopBegin`/`LoopEndBegin` (`[`/`][`) open on this track —
    /// `tools/mid2agb` has no such guard (`s_blockNum`/`loopEndBlockNum`
    /// start at `0`, so a stray `]` would emit `GOTO <label>_<n>_B0`, a
    /// dangling reference the assembler itself would then reject); this
    /// compiler fails closed at compile time instead.
    DanglingLoopEnd,
    /// A `MEMACC`-family controller (CC `0x0C`/`0x0D`/`0x0E`/`0x0F`/`0x10`/`0x11`
    /// — `tools/mid2agb/agb.cpp:349-415`'s `PrintMemAcc` family and its
    /// `_L<n>:` branch-target label) appeared. Deliberately unimplemented —
    /// see `super`'s module docs, "Deliberately out of scope: `MEMACC`".
    /// Carries the raw controller number.
    UnsupportedMemAccController(u8),
    /// `midi.cfg` requested something other than the default 24
    /// clocks-per-beat (`tools/mid2agb`'s `-X` flag, 48 clocks/beat).
    /// Deliberately unimplemented — see `super`'s module docs, "Pinned to
    /// `mus_title`'s own compile flags". Carries the requested value.
    UnsupportedClocksPerBeat(u8),
    /// `midi.cfg` did not request exact gate time (`tools/mid2agb`'s `-E`
    /// flag). Deliberately unimplemented — see `super`'s module docs,
    /// "Pinned to `mus_title`'s own compile flags".
    NonExactGateTime,
    /// No `MTrk` chunk was present at all.
    NoTracks,
    /// `MThd`'s division field was `0` (zero ticks per quarter note), which
    /// would make every tick-scaling division in [`super::compile`]
    /// undefined. Not a case `tools/mid2agb` itself guards explicitly, but
    /// nonsensical for any real file — a fail-closed check rather than a
    /// silent divide-by-zero.
    ZeroTimeDivision,
    /// `midi.cfg` had no entry for the requested filename. Carries the
    /// filename.
    CfgEntryMissing(String),
    /// `midi.cfg`'s entry for the requested filename carried a flag this
    /// parser does not recognize, or a recognized flag with a malformed
    /// operand. Carries the offending token.
    CfgMalformedFlag(String),
    /// `midi.cfg`'s entry for the requested filename had no `-G` (voice
    /// group) flag — every real entry does, but a hand-edited or corrupt
    /// checkout could omit it.
    CfgMissingVoiceGroup,
}

impl fmt::Display for MidiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "unexpected end of file"),
            Self::TickOverflow(raw) => write!(
                f,
                "MIDI tick {raw} overflows a u32 once scaled to 24 clocks per beat"
            ),
            Self::TooManyTracks(count) => write!(
                f,
                "compiled song has {count} tracks, more than the schema's u8 track count allows"
            ),
            Self::BadHeaderMagic => write!(f, "not a standard MIDI file (missing MThd magic)"),
            Self::HeaderLengthMismatch(len) => {
                write!(f, "MThd header length {len} is not the standard 6")
            }
            Self::UnsupportedFormat(fmt_id) => {
                write!(f, "unsupported MIDI format {fmt_id} (only 0 and 1 supported)")
            }
            Self::NegativeDivision(div) => write!(
                f,
                "unsupported MIDI time division {div} (SMPTE frame-rate division not supported)"
            ),
            Self::BadTrackMagic => write!(f, "expected an MTrk chunk"),
            Self::InvalidStatusByte(byte) => write!(f, "invalid MIDI status byte 0x{byte:02X}"),
            Self::InvalidDataByte(byte) => write!(f, "invalid MIDI data byte 0x{byte:02X}"),
            Self::BadTempoLength(len) => write!(f, "tempo meta event length {len} is not 3"),
            Self::ZeroTempo => write!(f, "tempo meta event is 0 microseconds per quarter note"),
            Self::BadTimeSignatureLength(len) => {
                write!(f, "time signature meta event length {len} is not 4")
            }
            Self::BadTimeSignatureDenominator(exponent) => write!(
                f,
                "time signature denominator exponent {exponent} is 16 or more"
            ),
            Self::ZeroTimeSignature => {
                write!(f, "time signature works out to a zero-tick whole-note grid")
            }
            Self::TempoOverflow(microseconds) => write!(
                f,
                "tempo {microseconds} microseconds per quarter note computes to a BPM that overflows a u16"
            ),
            Self::UnterminatedNote { channel, key } => write!(
                f,
                "note {key} on channel {channel} has no matching note-off before end of track"
            ),
            Self::DanglingLoopEnd => {
                write!(f, "loop-end marker (`]`/`][`) with no preceding loop-begin (`[`/`][`)")
            }
            Self::UnsupportedMemAccController(cc) => write!(
                f,
                "controller {cc} is part of the MEMACC family, which this compiler does not support"
            ),
            Self::UnsupportedClocksPerBeat(value) => write!(
                f,
                "clocks-per-beat {value} is not supported (only the default, 1, is)"
            ),
            Self::NonExactGateTime => write!(
                f,
                "midi.cfg entry does not request exact gate time (`-E`), which this compiler requires"
            ),
            Self::NoTracks => write!(f, "MIDI file has no MTrk chunks"),
            Self::ZeroTimeDivision => write!(f, "MThd time division is 0"),
            Self::CfgEntryMissing(name) => write!(f, "midi.cfg has no entry for `{name}`"),
            Self::CfgMalformedFlag(token) => write!(f, "midi.cfg entry has a malformed flag `{token}`"),
            Self::CfgMissingVoiceGroup => {
                write!(f, "midi.cfg entry has no `-G` (voice group) flag")
            }
        }
    }
}

impl std::error::Error for MidiError {}
