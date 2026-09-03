//! MIDI configuration, parsing, compilation, and encoding errors.

use std::fmt;

/// An error produced while extracting a MIDI song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MidiError {
    /// Input is incomplete, a byte length does not fit `usize`, or event-time
    /// arithmetic overflows or underflows.
    Truncated,
    /// A raw tick cannot be scaled into a `u32`.
    TickOverflow(u32),
    /// The compiled track count does not fit the song schema.
    TooManyTracks(usize),
    /// The encoded voicegroup pack id (the `audio/voicegroup/` prefix plus
    /// the configured label) does not fit the song schema's `u16`
    /// string-length field. Carries the id's UTF-8 byte length.
    VoiceGroupPackIdTooLong(usize),
    /// The file does not start with an `MThd` chunk.
    BadHeaderMagic,
    /// The `MThd` chunk does not declare the standard body length.
    HeaderLengthMismatch(u32),
    /// The MIDI format is not 0 or 1.
    UnsupportedFormat(u16),
    /// The time division uses unsupported SMPTE framing.
    NegativeDivision(i16),
    /// An expected track chunk does not start with `MTrk`.
    BadTrackMagic,
    /// A status byte is unsupported, or a data byte appears without running
    /// status.
    InvalidStatusByte(u8),
    /// A channel-voice operand is not a seven-bit data byte.
    InvalidDataByte(u8),
    /// A tempo event does not contain three bytes.
    BadTempoLength(u32),
    /// A tempo event declares zero microseconds per quarter note.
    ZeroTempo,
    /// Tempo conversion produces a BPM that does not fit a `u16`.
    TempoOverflow(u32),
    /// A time-signature event does not contain four bytes.
    BadTimeSignatureLength(u32),
    /// A time-signature denominator exponent is 16 or greater.
    BadTimeSignatureDenominator(u8),
    /// A time signature produces a zero-tick whole-note grid.
    ZeroTimeSignature,
    /// A note has no matching note-off before the track ends.
    UnterminatedNote { channel: u8, key: u8 },
    /// A loop end has no matching open marker.
    DanglingLoopEnd,
    /// A controller belongs to the unsupported `MEMACC` family.
    UnsupportedMemAccController(u8),
    /// The configuration requests unsupported clocks per beat.
    UnsupportedClocksPerBeat(u8),
    /// The configuration does not request exact gate time.
    NonExactGateTime,
    /// The file declares no track chunks.
    NoTracks,
    /// The time division is zero.
    ZeroTimeDivision,
    /// The requested filename has no `midi.cfg` entry.
    CfgEntryMissing(String),
    /// A `midi.cfg` flag is unknown or has a malformed operand.
    CfgMalformedFlag(String),
    /// A `midi.cfg` entry has no voicegroup flag.
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
            Self::VoiceGroupPackIdTooLong(byte_len) => write!(
                f,
                "compiled song's voicegroup pack id is {byte_len} bytes, more than the schema's \
                 u16 string-length field allows"
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
