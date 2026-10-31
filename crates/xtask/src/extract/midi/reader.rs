//! Standard MIDI file byte reads and chunk framing.

use super::error::MidiError;

const MIDI_HEADER_MAGIC: &[u8; 4] = b"MThd";
const MIDI_TRACK_MAGIC: &[u8; 4] = b"MTrk";
const MIDI_HEADER_BODY_LEN: u32 = 6;
const MIDI_FILE_HEADER_LEN: usize = 14;
const MAX_SUPPORTED_FORMAT: u16 = 1;
const VLQ_DATA_BITS: u32 = 7;
const VLQ_DATA_MASK: u8 = 0x7F;
const VLQ_CONTINUATION_BIT: u8 = 0x80;

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

    pub(super) fn u24_be(&mut self) -> Result<u32, MidiError> {
        let b = self.take(3)?;
        Ok(u32::from_be_bytes([0, b[0], b[1], b[2]]))
    }

    pub(super) fn u32_be(&mut self) -> Result<u32, MidiError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Reads a big-endian, seven-bit-group MIDI variable-length quantity.
    ///
    /// Over-long values discard high bits like `mid2agb`'s unsigned
    /// accumulator (`tools/mid2agb/midi.cpp:116-129`).
    ///
    /// # Errors
    ///
    /// Returns [`MidiError::Truncated`] when no terminating byte remains.
    pub(super) fn vlq(&mut self) -> Result<u32, MidiError> {
        let mut val: u32 = 0;
        loop {
            let c = self.u8()?;
            val = val.wrapping_shl(VLQ_DATA_BITS) | u32::from(c & VLQ_DATA_MASK);
            if c & VLQ_CONTINUATION_BIT == 0 {
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

/// Validated MIDI header fields used by compilation.
#[derive(Debug)]
pub(super) struct MidiHeader {
    pub(super) track_count: u16,
    /// Ticks per quarter note; SMPTE division is rejected during parsing.
    pub(super) division: u16,
}

/// Parses the fixed MIDI file header.
///
/// # Errors
///
/// Returns [`MidiError::Truncated`], [`MidiError::BadHeaderMagic`],
/// [`MidiError::HeaderLengthMismatch`], [`MidiError::UnsupportedFormat`], or
/// [`MidiError::NegativeDivision`].
pub(super) fn read_header(bytes: &[u8]) -> Result<MidiHeader, MidiError> {
    let mut r = MidiReader::new(bytes);
    if r.bytes(MIDI_HEADER_MAGIC.len())? != MIDI_HEADER_MAGIC {
        return Err(MidiError::BadHeaderMagic);
    }
    let header_len = r.u32_be()?;
    if header_len != MIDI_HEADER_BODY_LEN {
        return Err(MidiError::HeaderLengthMismatch(header_len));
    }
    let format = r.u16_be()?;
    if format > MAX_SUPPORTED_FORMAT {
        return Err(MidiError::UnsupportedFormat(format));
    }
    let track_count = r.u16_be()?;
    let division = r.u16_be()?;
    let division_signed = i16::from_be_bytes(division.to_be_bytes());
    if division_signed < 0 {
        return Err(MidiError::NegativeDivision(division_signed));
    }
    Ok(MidiHeader {
        track_count,
        division,
    })
}

/// Returns the declared track bodies in file order.
///
/// # Errors
///
/// Returns [`MidiError::NoTracks`], [`MidiError::BadTrackMagic`], or
/// [`MidiError::Truncated`].
pub(super) fn split_tracks(bytes: &[u8], track_count: u16) -> Result<Vec<&[u8]>, MidiError> {
    if track_count == 0 {
        return Err(MidiError::NoTracks);
    }
    let mut r = MidiReader::new(bytes);
    r.skip(MIDI_FILE_HEADER_LEN)?;
    let mut tracks = Vec::with_capacity(usize::from(track_count));
    for _ in 0..track_count {
        if r.bytes(MIDI_TRACK_MAGIC.len())? != MIDI_TRACK_MAGIC {
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
