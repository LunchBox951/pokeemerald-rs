//! Byte cursor and string writer for the wire format.

use super::errors::EventScriptError;

pub(super) struct Cursor<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) pos: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn take(
        &mut self,
        n: usize,
        section: &'static str,
    ) -> Result<&'a [u8], EventScriptError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.bytes.len())
            .ok_or(EventScriptError::Truncated { section })?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub(super) fn u8(&mut self, section: &'static str) -> Result<u8, EventScriptError> {
        Ok(self.take(1, section)?[0])
    }

    pub(super) fn u16(&mut self, section: &'static str) -> Result<u16, EventScriptError> {
        let b = self.take(2, section)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(super) fn u32(&mut self, section: &'static str) -> Result<u32, EventScriptError> {
        let b = self.take(4, section)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(super) fn string(&mut self, section: &'static str) -> Result<String, EventScriptError> {
        let len = usize::from(self.u16(section)?);
        let raw = self.take(len, section)?;
        match std::str::from_utf8(raw) {
            Ok(s) if !s.is_empty() => Ok(s.to_owned()),
            _ => Err(EventScriptError::InvalidString { section }),
        }
    }
}

pub(super) fn put_string(out: &mut Vec<u8>, s: &str) {
    let len = u16::try_from(s.len()).expect("validated names fit u16");
    out.extend(len.to_le_bytes());
    out.extend(s.as_bytes());
}
