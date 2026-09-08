//! The GBA BIOS LZ77 decompressor (compression type `0x10`).
//!
//! Most bulk data in the ROM is stored compressed and unpacked by a BIOS
//! call, so nearly every domain reader lands here. The format:
//!
//! ```text
//! byte 0      compression type, 0x10
//! bytes 1..4  decompressed size, 24-bit little-endian
//! then repeating:
//!   1 flag byte, one bit per token, most significant bit first
//!     bit clear: 1 literal byte, copied out
//!     bit set:   2 bytes b0 b1
//!                length       = (b0 >> 4) + 3
//!                displacement = (((b0 & 0xF) << 8) | b1) + 1
//!                copy `length` bytes from `displacement` bytes back
//! ```
//!
//! Back-references may overlap the bytes they produce, which is how the
//! format encodes runs, so the copy is byte at a time rather than a block
//! move.
//!
//! Every failure is fail-closed. The hardware BIOS decodes garbage into
//! garbage; the importer refuses instead, because a silently wrong sprite is
//! far worse to debug than a refused import.

use crate::error::{ImportError, Lz77Fault};
use crate::reader::RomReader;

/// The compression type byte this decoder handles.
pub const LZ77_TYPE: u8 = 0x10;

/// How much output space to reserve up front.
///
/// The declared size is attacker-controlled (it comes from the user's ROM),
/// so the reservation is capped and the buffer grows on demand instead of
/// trusting a 16 MiB claim from a garbage stream.
const INITIAL_CAPACITY: usize = 64 * 1024;

/// Decompress an LZ77 type `0x10` stream.
///
/// `stream` may extend past the end of the compressed data; decoding stops
/// once the declared size is produced. `expected_len`, when given, is the
/// size the caller knows the data must have, and a disagreement is an error
/// rather than a silently short buffer.
///
/// # Errors
///
/// [`Lz77Fault::WrongType`] if the first byte is not [`LZ77_TYPE`];
/// [`Lz77Fault::UnexpectedEnd`] if a token needs bytes past the end of
/// `stream`; [`Lz77Fault::BackReferenceTooFar`] if a back-reference reaches
/// further back than the output produced so far;
/// [`Lz77Fault::SizeMismatch`] if `expected_len` disagrees with the result.
pub fn decompress(stream: &[u8], expected_len: Option<usize>) -> Result<Vec<u8>, Lz77Fault> {
    let header = stream.get(..4).ok_or(Lz77Fault::UnexpectedEnd)?;
    if header[0] != LZ77_TYPE {
        return Err(Lz77Fault::WrongType);
    }
    let declared_raw = u32::from_le_bytes([header[1], header[2], header[3], 0]);
    // A 24-bit size fits any target with a 32-bit or wider pointer; the
    // saturating fallback is unreachable there and fails the read elsewhere.
    let declared = usize::try_from(declared_raw).unwrap_or(usize::MAX);

    let mut out: Vec<u8> = Vec::with_capacity(declared.min(INITIAL_CAPACITY));
    let mut cursor = 4;

    while out.len() < declared {
        let flags = *stream.get(cursor).ok_or(Lz77Fault::UnexpectedEnd)?;
        cursor += 1;

        for bit in 0..8u32 {
            if out.len() >= declared {
                break;
            }
            if flags & (0x80 >> bit) == 0 {
                let byte = *stream.get(cursor).ok_or(Lz77Fault::UnexpectedEnd)?;
                cursor += 1;
                out.push(byte);
            } else {
                let token = stream
                    .get(cursor..cursor + 2)
                    .ok_or(Lz77Fault::UnexpectedEnd)?;
                cursor += 2;
                let len = usize::from(token[0] >> 4) + 3;
                let displacement =
                    usize::from((u16::from(token[0] & 0x0F) << 8) | u16::from(token[1])) + 1;
                if displacement > out.len() {
                    return Err(Lz77Fault::BackReferenceTooFar {
                        displacement,
                        produced: out.len(),
                    });
                }
                let start = out.len() - displacement;
                for step in 0..len {
                    if out.len() >= declared {
                        // The last token of a stream may claim more than the
                        // declared size; the BIOS stops at the size too.
                        break;
                    }
                    let byte = out[start + step];
                    out.push(byte);
                }
            }
        }
    }

    if let Some(expected) = expected_len {
        if out.len() != expected {
            return Err(Lz77Fault::SizeMismatch {
                expected,
                actual: out.len(),
            });
        }
    }
    Ok(out)
}

/// Decompress the LZ77 stream starting at ROM offset `at`.
///
/// # Errors
///
/// [`ImportError::Truncated`] if `at` is past the end of the ROM;
/// [`ImportError::Lz77`] carrying the [`Lz77Fault`] for any decoding
/// failure.
pub fn decompress_at(
    reader: &RomReader<'_>,
    at: usize,
    expected_len: Option<usize>,
) -> Result<Vec<u8>, ImportError> {
    let stream = reader.tail(at)?;
    decompress(stream, expected_len).map_err(|fault| ImportError::Lz77 { at, fault })
}

#[cfg(test)]
mod tests {
    use super::{decompress, decompress_at};
    use crate::error::{ImportError, Lz77Fault};
    use crate::reader::RomReader;

    /// Build a stream header for `size` decompressed bytes.
    fn header(size: u32) -> Vec<u8> {
        let bytes = size.to_le_bytes();
        vec![0x10, bytes[0], bytes[1], bytes[2]]
    }

    #[test]
    fn literals_only() {
        let mut stream = header(4);
        stream.extend_from_slice(&[0x00, b'A', b'B', b'C', b'D']);
        assert_eq!(decompress(&stream, None).unwrap(), b"ABCD");
        assert_eq!(decompress(&stream, Some(4)).unwrap(), b"ABCD");
    }

    #[test]
    fn empty_output() {
        assert_eq!(decompress(&header(0), Some(0)).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn back_reference() {
        // Three literals, then copy 3 bytes from 3 back.
        let mut stream = header(6);
        stream.extend_from_slice(&[0b0001_0000, b'A', b'B', b'C', 0x00, 0x02]);
        assert_eq!(decompress(&stream, Some(6)).unwrap(), b"ABCABC");
    }

    #[test]
    fn overlapping_back_reference_makes_a_run() {
        // One literal, then copy 3 bytes from 1 back: the copy reads bytes it
        // is still writing.
        let mut stream = header(4);
        stream.extend_from_slice(&[0b0100_0000, b'A', 0x00, 0x00]);
        assert_eq!(decompress(&stream, Some(4)).unwrap(), b"AAAA");
    }

    #[test]
    fn long_back_reference() {
        // Maximum token length, 18 bytes, from 2 back.
        let mut stream = header(20);
        stream.extend_from_slice(&[0b0010_0000, b'a', b'b', 0xF0, 0x01]);
        assert_eq!(
            decompress(&stream, Some(20)).unwrap(),
            b"abababababababababab"
        );
    }

    #[test]
    fn a_token_past_the_declared_size_is_clamped() {
        // The back-reference asks for 18 bytes with only 3 left to produce.
        let mut stream = header(4);
        stream.extend_from_slice(&[0b0100_0000, b'A', 0xF0, 0x00]);
        assert_eq!(decompress(&stream, Some(4)).unwrap(), b"AAAA");
    }

    #[test]
    fn wrong_type_byte_is_rejected() {
        let mut stream = header(4);
        stream[0] = 0x20;
        assert_eq!(decompress(&stream, None), Err(Lz77Fault::WrongType));
    }

    #[test]
    fn a_short_header_is_rejected() {
        assert_eq!(
            decompress(&[0x10, 0x00], None),
            Err(Lz77Fault::UnexpectedEnd)
        );
        assert_eq!(decompress(&[], None), Err(Lz77Fault::UnexpectedEnd));
    }

    #[test]
    fn a_stream_that_ends_mid_token_is_rejected() {
        // Declared 4 bytes, but the stream stops after the flag byte.
        let mut flags_only = header(4);
        flags_only.push(0x00);
        assert_eq!(decompress(&flags_only, None), Err(Lz77Fault::UnexpectedEnd));

        // Declared 4 bytes, but no flag byte at all.
        assert_eq!(decompress(&header(4), None), Err(Lz77Fault::UnexpectedEnd));

        // A back-reference token missing its second byte.
        let mut half_token = header(6);
        half_token.extend_from_slice(&[0b0100_0000, b'A', 0x00]);
        assert_eq!(decompress(&half_token, None), Err(Lz77Fault::UnexpectedEnd));
    }

    #[test]
    fn a_back_reference_past_the_output_start_is_rejected() {
        // The very first token is a back-reference, so nothing is behind it.
        let mut stream = header(4);
        stream.extend_from_slice(&[0b1000_0000, 0x00, 0x00]);
        assert_eq!(
            decompress(&stream, None),
            Err(Lz77Fault::BackReferenceTooFar {
                displacement: 1,
                produced: 0
            })
        );

        // Two literals out, then a reference 3 bytes back.
        let mut stream = header(8);
        stream.extend_from_slice(&[0b0010_0000, b'A', b'B', 0x00, 0x02]);
        assert_eq!(
            decompress(&stream, None),
            Err(Lz77Fault::BackReferenceTooFar {
                displacement: 3,
                produced: 2
            })
        );
    }

    #[test]
    fn an_expected_length_disagreement_is_rejected() {
        let mut stream = header(4);
        stream.extend_from_slice(&[0x00, b'A', b'B', b'C', b'D']);
        assert_eq!(
            decompress(&stream, Some(5)),
            Err(Lz77Fault::SizeMismatch {
                expected: 5,
                actual: 4
            })
        );
    }

    #[test]
    fn decompress_at_reports_the_rom_offset() {
        let mut image = vec![0u8; 32];
        let mut stream = header(4);
        stream.extend_from_slice(&[0x00, b'A', b'B', b'C', b'D']);
        image[8..8 + stream.len()].copy_from_slice(&stream);
        let reader = RomReader::new(&image);

        assert_eq!(decompress_at(&reader, 8, Some(4)).unwrap(), b"ABCD");
        assert!(matches!(
            decompress_at(&reader, 0, None),
            Err(ImportError::Lz77 {
                at: 0,
                fault: Lz77Fault::WrongType
            })
        ));
        assert!(matches!(
            decompress_at(&reader, 64, None),
            Err(ImportError::Truncated { at: 64, .. })
        ));
    }
}
