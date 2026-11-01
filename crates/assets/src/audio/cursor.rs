//! Bounded little-endian reading and writing for audio-pack schema payloads.

use super::error::AudioError;

const MAX_ID_BYTE_LEN: usize = u16::MAX as usize;

/// Rejects ids whose byte length cannot fit the wire format's `u16` prefix.
pub(super) fn check_id_len(id: &str) -> Result<(), AudioError> {
    if id.len() > MAX_ID_BYTE_LEN {
        return Err(AudioError::IdTooLong(id.len()));
    }
    Ok(())
}

/// A cursor that rejects reads extending beyond its input.
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take_exact(&mut self, len: usize) -> Result<&'a [u8], AudioError> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(AudioError::Truncated)?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(AudioError::Truncated)?;
        self.position = end;
        Ok(bytes)
    }

    pub(super) fn u8(&mut self) -> Result<u8, AudioError> {
        Ok(self.take_exact(1)?[0])
    }

    /// Rejects payloads with unread bytes.
    pub(super) fn expect_eof(&self) -> Result<(), AudioError> {
        let remaining = self.bytes.len() - self.position;
        if remaining != 0 {
            return Err(AudioError::TrailingBytes(remaining));
        }
        Ok(())
    }

    pub(super) fn i8(&mut self) -> Result<i8, AudioError> {
        Ok(i8::from_le_bytes([self.u8()?]))
    }

    pub(super) fn u16(&mut self) -> Result<u16, AudioError> {
        let bytes = self.take_exact(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(super) fn u32(&mut self) -> Result<u32, AudioError> {
        let bytes = self.take_exact(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Reads a `0` or `1` byte, rejecting every other value as malformed.
    pub(super) fn bool(&mut self) -> Result<bool, AudioError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(AudioError::Truncated),
        }
    }

    /// Reads a UTF-8 string prefixed by its `u16` byte length.
    pub(super) fn string(&mut self) -> Result<String, AudioError> {
        let len = usize::from(self.u16()?);
        let bytes = self.take_exact(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| AudioError::InvalidString)
    }

    /// Copies `count` bytes from the input.
    pub(super) fn bytes(&mut self, count: usize) -> Result<Vec<u8>, AudioError> {
        Ok(self.take_exact(count)?.to_vec())
    }
}

/// An append-only buffer for audio-pack schema payloads.
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
        self.bytes.push(value.to_le_bytes()[0]);
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

    /// Writes a UTF-8 string prefixed by its `u16` byte length.
    ///
    /// # Panics
    ///
    /// Panics if `value` exceeds the wire format's maximum id byte length.
    pub(super) fn string(&mut self, value: &str) {
        debug_assert!(
            value.len() <= MAX_ID_BYTE_LEN,
            "id length checked at construction"
        );
        let byte_len =
            u16::try_from(value.len()).expect("audio-pack string id fits in a u16 length");
        self.u16(byte_len);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    pub(super) fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
}
