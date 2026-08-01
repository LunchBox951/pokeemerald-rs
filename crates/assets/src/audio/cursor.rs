//! Shared little-endian byte reader/writer for the audio-pack schemas'
//! `encode`/`decode` pairs.
//!
//! Mirrors `crate::pack::format`'s `Cursor` (same "erroring rather than
//! panicking on truncation" discipline over untrusted input), extended with
//! a length-prefixed string helper (voicegroup/song entries reference other
//! pack entries by their normalized string id) and a matching write-side
//! [`Writer`]. `crate::pack::format` has no write side of its own — packs
//! are written by `xtask::extract`, a separate, deliberately decoupled crate
//! (see `crate::pack`'s module docs) — but these three schemas are
//! `encode`d by *this* crate directly (both extraction backends call into
//! it, per `super`'s module docs), so, unlike `pack::format`, this cursor
//! needs both directions.

use super::error::AudioError;

/// The longest pack id [`Writer::string`] can length-prefix — the `u16`
/// length field's ceiling.
pub(super) const MAX_ID_LEN: usize = u16::MAX as usize;

/// Reject a pack id too long for [`Writer::string`]'s `u16` length prefix.
///
/// Called by the schemas' checked constructors
/// ([`super::voicegroup::VoiceGroup::new`], [`super::song::Song::new`]) so
/// that an over-long id surfaces as [`AudioError::IdTooLong`] at
/// construction instead of as a panic inside `encode`. Every id that
/// reaches [`Writer::string`] has passed through one of those.
pub(super) fn check_id_len(id: &str) -> Result<(), AudioError> {
    if id.len() > MAX_ID_LEN {
        return Err(AudioError::IdTooLong(id.len()));
    }
    Ok(())
}

/// A minimal cursor over `&[u8]`, erroring (never panicking) on truncation
/// or malformed content.
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], AudioError> {
        let end = self.pos.checked_add(len).ok_or(AudioError::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(AudioError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    pub(super) fn u8(&mut self) -> Result<u8, AudioError> {
        Ok(self.take(1)?[0])
    }

    /// A byte reinterpreted as signed (the C `(s8)` cast).
    pub(super) fn i8(&mut self) -> Result<i8, AudioError> {
        Ok(i8::from_ne_bytes([self.u8()?]))
    }

    pub(super) fn u16(&mut self) -> Result<u16, AudioError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(super) fn u32(&mut self) -> Result<u32, AudioError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A `bool` stored as a single `0`/`1` byte. Any other byte value is
    /// [`AudioError::Truncated`] — treated the same as running off the end
    /// of the buffer, since either way the bytes at this position are not
    /// this format's shape.
    pub(super) fn bool(&mut self) -> Result<bool, AudioError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(AudioError::Truncated),
        }
    }

    /// A `u16`-length-prefixed UTF-8 string (voicegroup/song entries'
    /// references to other pack entries by normalized id).
    pub(super) fn string(&mut self) -> Result<String, AudioError> {
        let len = usize::from(self.u16()?);
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| AudioError::InvalidString)
    }

    /// `count` raw bytes, owned.
    pub(super) fn bytes(&mut self, count: usize) -> Result<Vec<u8>, AudioError> {
        Ok(self.take(count)?.to_vec())
    }
}

/// The write side of [`Reader`]: an append-only little-endian byte buffer.
#[derive(Default)]
pub(super) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(super) fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(super) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(super) fn i8(&mut self, value: i8) {
        self.bytes.push(value.to_ne_bytes()[0]);
    }

    pub(super) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    /// A `u16`-length-prefixed UTF-8 string.
    ///
    /// # Panics
    ///
    /// Unreachable from this crate's public API: the only way to reach an
    /// `encode` is through a checked constructor
    /// ([`super::voicegroup::VoiceGroup::new`], [`super::song::Song::new`]),
    /// and each one runs [`check_id_len`] over every id it will later write,
    /// returning [`AudioError::IdTooLong`] rather than deferring the failure
    /// to here.
    pub(super) fn string(&mut self, value: &str) {
        debug_assert!(
            value.len() <= MAX_ID_LEN,
            "id length checked at construction"
        );
        let len = u16::try_from(value.len()).expect("audio-pack string id fits in a u16 length");
        self.u16(len);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    pub(super) fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
}
