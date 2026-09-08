//! FIPS 180-4 SHA-1, hand-rolled.
//!
//! ROM identity is a whole-file hash, so the importer needs SHA-1 and the
//! project takes no new dependency to get one `(minimal-deps)`. SHA-1 is used
//! here only to name a known ROM revision, never as a security primitive.
//!
//! [`Sha1`] is the streaming state; [`sha1`] is the one-shot convenience.
//! [`Digest`] renders as lowercase hex and parses back from it, so a profile
//! can carry its expected hash as a `const` built straight from the hex
//! string.

use std::fmt;
use std::str::FromStr;

/// A 20-byte SHA-1 digest.
///
/// [`fmt::Display`] renders lowercase hex, the form used by every published
/// ROM hash list and by [`FromStr`]. [`fmt::Debug`] renders the same text so
/// test failures stay readable.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 20]);

impl Digest {
    /// The digest's raw bytes, most significant first.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Build a digest from raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Build a digest from a 40-character lowercase or uppercase hex string.
    ///
    /// This is `const` so a [`RevisionProfile`](crate::RevisionProfile) can
    /// name its expected hash inline and have any typo caught at compile
    /// time rather than at import time.
    ///
    /// # Panics
    ///
    /// Panics if `hex` is not exactly 40 characters or contains a non-hex
    /// character. In a `const` context that panic is a compile error.
    #[must_use]
    pub const fn from_hex(hex: &str) -> Self {
        let text = hex.as_bytes();
        assert!(
            text.len() == 40,
            "a SHA-1 digest is exactly 40 hex characters"
        );
        let mut bytes = [0u8; 20];
        let mut i = 0;
        while i < 20 {
            bytes[i] = (nibble(text[i * 2]) << 4) | nibble(text[i * 2 + 1]);
            i += 1;
        }
        Self(bytes)
    }
}

/// Decode one hex character. Panics on anything else, which in a `const`
/// context is a compile error at the profile's definition site.
const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("a SHA-1 digest contains only hex characters"),
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({self})")
    }
}

/// Why a string could not be parsed as a [`Digest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestParseError {
    /// The string was not exactly 40 characters. Carries the length seen.
    WrongLength(usize),
    /// The string held a character outside `0-9`, `a-f`, `A-F`.
    NotHex,
}

impl fmt::Display for DigestParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength(len) => {
                write!(f, "a SHA-1 digest is 40 hex characters, got {len}")
            }
            Self::NotHex => f.write_str("a SHA-1 digest holds only hex characters"),
        }
    }
}

impl std::error::Error for DigestParseError {}

impl FromStr for Digest {
    type Err = DigestParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let text = s.as_bytes();
        if text.len() != 40 {
            return Err(DigestParseError::WrongLength(text.len()));
        }
        let mut bytes = [0u8; 20];
        for (byte, pair) in bytes.iter_mut().zip(text.chunks_exact(2)) {
            let hi = checked_nibble(pair[0]).ok_or(DigestParseError::NotHex)?;
            let lo = checked_nibble(pair[1]).ok_or(DigestParseError::NotHex)?;
            *byte = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }
}

/// Decode one hex character, or `None` if it is not one.
const fn checked_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Hash `data` in one call.
#[must_use]
pub fn sha1(data: &[u8]) -> Digest {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher.finalize()
}

/// The initial chaining state, FIPS 180-4 section 5.3.1.
const INITIAL_STATE: [u32; 5] = [
    0x6745_2301,
    0xEFCD_AB89,
    0x98BA_DCFE,
    0x1032_5476,
    0xC3D2_E1F0,
];

/// A streaming SHA-1 hasher.
///
/// A 16 MiB ROM is hashed in one [`update`](Sha1::update) call in practice,
/// but the streaming shape keeps the caller free to hash a file without
/// buffering it whole.
#[derive(Clone)]
pub struct Sha1 {
    state: [u32; 5],
    /// Bytes not yet part of a full 64-byte block.
    buffer: [u8; 64],
    buffered: usize,
    /// Total message length, needed for the length-suffix padding.
    len_bytes: u64,
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha1 {
    /// A hasher over the empty message.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            buffer: [0u8; 64],
            buffered: 0,
            len_bytes: 0,
        }
    }

    /// Feed more message bytes.
    pub fn update(&mut self, data: &[u8]) {
        // `usize` never exceeds 64 bits on a supported target, so `try_from`
        // is total here; saturating keeps it that way without a cast lint
        // exception.
        let added = u64::try_from(data.len()).unwrap_or(u64::MAX);
        self.len_bytes = self.len_bytes.saturating_add(added);
        self.absorb(data);
    }

    /// Buffer and compress `data` without counting it toward the message
    /// length. Padding reuses this so it does not inflate the length suffix.
    fn absorb(&mut self, data: &[u8]) {
        let mut rest = data;

        if self.buffered > 0 {
            let take = (64 - self.buffered).min(rest.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&rest[..take]);
            self.buffered += take;
            rest = &rest[take..];
            if self.buffered < 64 {
                // The buffer is still short, so `rest` must be empty.
                return;
            }
            let block = self.buffer;
            compress(&mut self.state, &block);
            self.buffered = 0;
        }

        let mut at = 0;
        while at + 64 <= rest.len() {
            let mut block = [0u8; 64];
            block.copy_from_slice(&rest[at..at + 64]);
            compress(&mut self.state, &block);
            at += 64;
        }

        let tail = &rest[at..];
        self.buffer[..tail.len()].copy_from_slice(tail);
        self.buffered = tail.len();
    }

    /// Pad the message and produce the digest.
    #[must_use]
    pub fn finalize(mut self) -> Digest {
        let len_bits = self.len_bytes.wrapping_mul(8);

        self.absorb(&[0x80]);
        // Pad with zeros until 8 bytes short of a block boundary. `buffered`
        // is 0..=63 here, so the modulo keeps the count in range.
        let zeros = (120 - self.buffered) % 64;
        self.absorb(&[0u8; 64][..zeros]);
        self.absorb(&len_bits.to_be_bytes());

        let mut out = [0u8; 20];
        for (chunk, word) in out.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        Digest(out)
    }
}

/// One 64-byte block of the FIPS 180-4 section 6.1.2 compression function.
fn compress(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for (word, chunk) in w.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }

    let [mut ha, mut hb, mut hc, mut hd, mut he] = *state;
    for (round, &word) in w.iter().enumerate() {
        let (mixed, round_const) = match round {
            0..=19 => ((hb & hc) | (!hb & hd), 0x5A82_7999),
            20..=39 => (hb ^ hc ^ hd, 0x6ED9_EBA1),
            40..=59 => ((hb & hc) | (hb & hd) | (hc & hd), 0x8F1B_BCDC),
            _ => (hb ^ hc ^ hd, 0xCA62_C1D6),
        };
        let next = ha
            .rotate_left(5)
            .wrapping_add(mixed)
            .wrapping_add(he)
            .wrapping_add(round_const)
            .wrapping_add(word);
        he = hd;
        hd = hc;
        hc = hb.rotate_left(30);
        hb = ha;
        ha = next;
    }

    state[0] = state[0].wrapping_add(ha);
    state[1] = state[1].wrapping_add(hb);
    state[2] = state[2].wrapping_add(hc);
    state[3] = state[3].wrapping_add(hd);
    state[4] = state[4].wrapping_add(he);
}

#[cfg(test)]
mod tests {
    use super::{compress, sha1, Digest, DigestParseError, Sha1, INITIAL_STATE};

    /// An independent one-shot path: build the whole padded message in a
    /// single buffer, then run every block. It shares only [`compress`] with
    /// the streaming hasher, so it pins the buffering and padding logic that
    /// [`Sha1`] does incrementally.
    fn naive_sha1(message: &[u8]) -> Digest {
        let mut padded = message.to_vec();
        padded.push(0x80);
        while padded.len() % 64 != 56 {
            padded.push(0);
        }
        let bits = u64::try_from(message.len()).unwrap() * 8;
        padded.extend_from_slice(&bits.to_be_bytes());

        let mut state = INITIAL_STATE;
        for chunk in padded.chunks_exact(64) {
            let mut block = [0u8; 64];
            block.copy_from_slice(chunk);
            compress(&mut state, &block);
        }

        let mut out = [0u8; 20];
        for (slot, word) in out.chunks_exact_mut(4).zip(state) {
            slot.copy_from_slice(&word.to_be_bytes());
        }
        Digest::from_bytes(out)
    }

    #[test]
    fn rfc3174_abc() {
        assert_eq!(
            sha1(b"abc").to_string(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn rfc3174_two_block() {
        let message = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(
            sha1(message).to_string(),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn empty_message() {
        assert_eq!(
            sha1(b"").to_string(),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn one_million_a() {
        let mut hasher = Sha1::new();
        // Fed in uneven chunks so the buffer path carries data across calls.
        let chunk = vec![b'a'; 1000];
        for _ in 0..1000 {
            hasher.update(&chunk);
        }
        assert_eq!(
            hasher.finalize().to_string(),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    #[test]
    fn block_boundary_lengths_match_naive() {
        for len in [0usize, 1, 55, 56, 63, 64, 65, 127, 128, 129] {
            let message: Vec<u8> = (0..len).map(|i| u8::try_from(i % 251).unwrap()).collect();
            assert_eq!(sha1(&message), naive_sha1(&message), "one-shot, len {len}");

            // The same message fed one byte at a time must agree too.
            let mut hasher = Sha1::new();
            for byte in &message {
                hasher.update(&[*byte]);
            }
            assert_eq!(
                hasher.finalize(),
                naive_sha1(&message),
                "streamed, len {len}"
            );

            // And in 7-byte chunks, which never align to a block.
            let mut hasher = Sha1::new();
            for part in message.chunks(7) {
                hasher.update(part);
            }
            assert_eq!(
                hasher.finalize(),
                naive_sha1(&message),
                "chunked, len {len}"
            );
        }
    }

    #[test]
    fn display_is_lowercase_hex_and_stable() {
        let digest = Digest::from_bytes([0x0a; 20]);
        assert_eq!(digest.to_string(), "0a".repeat(20));
        assert_eq!(digest.to_string(), digest.to_string());
        assert_eq!(format!("{digest:?}"), format!("Digest({digest})"));
    }

    #[test]
    fn hex_round_trips() {
        let text = "f3ae088181bf583e55daf962a92bb46f4f1d07b7";
        let parsed: Digest = text.parse().unwrap();
        assert_eq!(parsed, Digest::from_hex(text));
        assert_eq!(parsed.to_string(), text);
        assert_eq!(Digest::from_hex(&text.to_uppercase()), parsed);
    }

    #[test]
    fn hex_rejects_bad_input() {
        assert_eq!(
            "abc".parse::<Digest>(),
            Err(DigestParseError::WrongLength(3))
        );
        let bad = "z3ae088181bf583e55daf962a92bb46f4f1d07b7";
        assert_eq!(bad.parse::<Digest>(), Err(DigestParseError::NotHex));
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(Sha1::default().finalize(), Sha1::new().finalize());
    }
}
