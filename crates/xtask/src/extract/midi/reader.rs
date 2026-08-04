//! A hand-rolled standard-MIDI-file byte reader: big-endian fixed-width
//! integers, variable-length quantities, and the `MThd`/`MTrk` chunk framing
//! (Standard MIDI File 1.0 — the same shape `tools/mid2agb/midi.cpp`'s
//! `ReadInt8`/`ReadInt16`/`ReadInt24`/`ReadInt32`/`ReadVLQ`/
//! `ReadMidiFileHeader`/`ReadMidiTrackHeader` read, re-expressed
//! idiomatically over an in-memory byte slice rather than a C `FILE*`
//! (`no-verbatim`) — this pipeline reads the whole `.mid` file into memory
//! up front (like every other extractor in this crate — see e.g.
//! `super::wav`), so there is no streaming/seeking concern to replicate.

use super::error::MidiError;

/// A cursor over `&[u8]`, erroring (never panicking) on truncation —
/// mirrors `super::super::wav`'s hand-rolled chunk reader and
/// `crates/assets/src/audio/cursor.rs`'s `Reader`.
pub(super) struct MidiReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> MidiReader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], MidiError> {
        let end = self.pos.checked_add(len).ok_or(MidiError::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(MidiError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    pub(super) fn u8(&mut self) -> Result<u8, MidiError> {
        Ok(self.take(1)?[0])
    }

    /// The next byte without consuming it — used to peek a channel-voice
    /// status byte before deciding whether it's a real status byte or a
    /// data byte implying running status (`tools/mid2agb/midi.cpp:184-190`'s
    /// `ungetc`, re-expressed as a non-consuming peek since this reader is
    /// over an in-memory slice rather than a `FILE*`).
    pub(super) fn peek_u8(&self) -> Result<u8, MidiError> {
        self.bytes
            .get(self.pos)
            .copied()
            .ok_or(MidiError::Truncated)
    }

    pub(super) fn u16_be(&mut self) -> Result<u16, MidiError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    /// A big-endian 24-bit integer (the tempo meta event's microseconds-per-
    /// quarter-note field — `tools/mid2agb/midi.cpp:97-104`'s `ReadInt24`).
    pub(super) fn u24_be(&mut self) -> Result<u32, MidiError> {
        let b = self.take(3)?;
        Ok(u32::from_be_bytes([0, b[0], b[1], b[2]]))
    }

    pub(super) fn u32_be(&mut self) -> Result<u32, MidiError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A standard-MIDI-file variable-length quantity: 7 bits per byte,
    /// continuation signalled by the top bit, big-endian bit order
    /// (`tools/mid2agb/midi.cpp:116-129`'s `ReadVLQ`).
    ///
    /// # Over-long quantities: upstream's tolerance, on purpose
    ///
    /// A standard MIDI file's VLQs are at most 4 bytes (28 bits) by
    /// construction, but nothing in the byte stream enforces that, and
    /// `ReadVLQ` carries no malformed-VLQ diagnostic at all: it accumulates
    /// with a plain `val <<= 7` (`midi.cpp:124`) into a `std::uint32_t`, so
    /// a fifth-or-later continuation byte silently pushes the high bits off
    /// the top. This reader does the same — Rust's shift-overflow check is
    /// on the shift *amount* (`7` is always in range for a `u32`), not on
    /// discarded bits, so `<<` drops them in debug and release alike, byte
    /// for byte as the C++ does.
    ///
    /// An earlier revision claimed to reject ">5 continuation bytes" via
    /// [`u32::checked_shl`]. That guard never existed (`checked_shl` only
    /// fails for a shift `>= 32`, never for lost bits), so rather than
    /// inventing a rejection upstream does not perform — behavioural
    /// fidelity is the tie-breaker for the real pipeline — the claim is
    /// gone and upstream's tolerance is pinned by test
    /// (`over_long_vlq_wraps_like_upstreams_shift`).
    ///
    /// This fixes the module's fail-open/fail-closed line: the *reader*
    /// mirrors upstream's tolerance, and the tick arithmetic downstream
    /// ([`super::translate::convert_ticks`], which evaluates in `u64` and
    /// returns [`MidiError::TickOverflow`]) is where an out-of-range tick
    /// becomes a real error instead of a silent wrap. A hostile file can
    /// make this reader return a nonsense `u32`; it cannot make the
    /// compiler panic, and it cannot make it emit silently-wrapped timing.
    ///
    /// # Errors
    ///
    /// [`MidiError::Truncated`] if the chunk's bytes run out before a byte
    /// with its continuation bit clear — the only way this can fail.
    pub(super) fn vlq(&mut self) -> Result<u32, MidiError> {
        let mut val: u32 = 0;
        loop {
            let c = self.u8()?;
            val = (val << 7) | u32::from(c & 0x7F); // midi.cpp:124's `val <<= 7`
            if c & 0x80 == 0 {
                return Ok(val);
            }
        }
    }

    pub(super) fn skip(&mut self, len: usize) -> Result<(), MidiError> {
        self.take(len)?;
        Ok(())
    }

    pub(super) fn bytes(&mut self, len: usize) -> Result<&'a [u8], MidiError> {
        self.take(len)
    }
}

/// `MThd`'s content fields this compiler needs, after the fixed 6-byte-length
/// and format checks (`tools/mid2agb/midi.cpp:131-154`'s
/// `ReadMidiFileHeader`). The format field itself (`0`/`1` — checked but not
/// carried forward: nothing downstream branches on it, since
/// [`super::compile`]'s nested `MTrk`-chunk/channel scan handles both
/// uniformly — see `super::parse`'s module docs, "One pass, not sixteen").
#[derive(Debug)]
pub(super) struct MidiHeader {
    pub(super) track_count: u16,
    /// Ticks per quarter note. Always non-negative — a negative division
    /// (SMPTE frame-rate division, top bit set when read as `i16`) is
    /// [`MidiError::NegativeDivision`] before this struct is built.
    pub(super) division: u16,
}

/// Parse the fixed 14-byte `MThd` chunk at the start of `bytes`.
///
/// # Errors
///
/// [`MidiError::BadHeaderMagic`], [`MidiError::HeaderLengthMismatch`],
/// [`MidiError::UnsupportedFormat`] (format `2` or greater —
/// `tools/mid2agb/midi.cpp:145-146` only accepts `0`/`1`),
/// [`MidiError::NegativeDivision`] (SMPTE division).
pub(super) fn read_header(bytes: &[u8]) -> Result<MidiHeader, MidiError> {
    let mut r = MidiReader::new(bytes);
    if r.bytes(4)? != b"MThd" {
        return Err(MidiError::BadHeaderMagic);
    }
    let header_len = r.u32_be()?;
    if header_len != 6 {
        return Err(MidiError::HeaderLengthMismatch(header_len));
    }
    let format = r.u16_be()?;
    if format >= 2 {
        return Err(MidiError::UnsupportedFormat(format));
    }
    let track_count = r.u16_be()?;
    let division_raw = r.u16_be()?;
    #[allow(clippy::cast_possible_wrap)]
    let division_signed = division_raw as i16;
    if division_signed < 0 {
        return Err(MidiError::NegativeDivision(division_signed));
    }
    Ok(MidiHeader {
        track_count,
        division: division_raw,
    })
}

/// Split `bytes` (the whole `.mid` file) into `track_count` `MTrk` chunk
/// bodies, in file order, starting right after the 14-byte `MThd` chunk.
/// Mirrors `tools/mid2agb/midi.cpp:156-168`'s `ReadMidiTrackHeader`, called
/// in sequence by `ReadMidiTracks` (`:916-927`).
///
/// # Errors
///
/// [`MidiError::NoTracks`] if `track_count` is `0`; [`MidiError::BadTrackMagic`]
/// if a chunk's signature is not `MTrk`; [`MidiError::Truncated`] if a
/// chunk's declared length runs past the end of `bytes`.
pub(super) fn split_tracks(bytes: &[u8], track_count: u16) -> Result<Vec<&[u8]>, MidiError> {
    if track_count == 0 {
        return Err(MidiError::NoTracks);
    }
    let mut r = MidiReader::new(bytes);
    r.skip(14)?; // the fixed MThd chunk this function's caller already validated
    let mut tracks = Vec::with_capacity(usize::from(track_count));
    for _ in 0..track_count {
        if r.bytes(4)? != b"MTrk" {
            return Err(MidiError::BadTrackMagic);
        }
        let len = r.u32_be()?;
        let body = r.bytes(usize::try_from(len).map_err(|_| MidiError::Truncated)?)?;
        tracks.push(body);
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests;
