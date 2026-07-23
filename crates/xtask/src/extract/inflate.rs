//! A minimal, std-only DEFLATE (RFC 1951) + zlib (RFC 1950) decompressor.
//!
//! `extract` needs to read the pixel data out of the PNGs upstream ships as
//! graphics *sources* (see the `png` module), and PNG's `IDAT` payload is a
//! zlib stream. No compression crate is available here — `minimal-deps`
//! forbids adding one without owner sign-off, and pulling in `flate2` /
//! `miniz_oxide` just to read tile art is exactly the kind of dependency
//! that sign-off gate exists to weigh. RFC 1951 is small enough to implement
//! directly against the spec (this is an independent implementation of a
//! public IETF algorithm, not upstream `pokeemerald` code — `no-verbatim`
//! only concerns the latter).
//!
//! Scope: everything a PNG encoder is allowed to emit is supported (stored,
//! fixed-Huffman, and dynamic-Huffman blocks; the full length/distance
//! alphabets). `inflate_zlib` also verifies the Adler-32 trailer, giving a
//! cheap end-to-end correctness check on every decode.

use std::fmt;

/// An error produced while inflating a DEFLATE or zlib stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InflateError {
    /// The bit or byte stream ran out before a block/stream finished.
    UnexpectedEnd,
    /// A stored (uncompressed) block's length and its one's-complement
    /// check field disagreed.
    BadStoredBlockLength,
    /// A block header's 2-bit `BTYPE` field was `0b11` (reserved, unused by
    /// any encoder).
    ReservedBlockType,
    /// A canonical Huffman code table was malformed (over- or
    /// under-subscribed code lengths).
    BadHuffmanTable,
    /// A decoded symbol had no matching Huffman code (can only happen if
    /// [`BadHuffmanTable`](Self::BadHuffmanTable) was missed, kept as a
    /// defensive fallback).
    InvalidCode,
    /// A back-reference's distance pointed further back than any byte
    /// produced so far.
    DistanceTooFar,
    /// The zlib header's 2-byte `CMF`/`FLG` pair failed its own checks
    /// (compression method must be 8, `(CMF*256+FLG) % 31 == 0`, and
    /// `FDICT` — a preset dictionary — is not supported).
    BadZlibHeader,
    /// The zlib trailer's Adler-32 checksum did not match the decompressed
    /// data.
    AdlerMismatch,
    /// The decompressed output grew past [`MAX_OUTPUT`] — treated as a
    /// corrupt or hostile stream rather than allowed to exhaust memory,
    /// since no real asset approaches that size.
    OutputTooLarge,
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "inflate: unexpected end of input"),
            Self::BadStoredBlockLength => write!(f, "inflate: stored block length check failed"),
            Self::ReservedBlockType => write!(f, "inflate: reserved block type"),
            Self::BadHuffmanTable => write!(f, "inflate: malformed Huffman code table"),
            Self::InvalidCode => write!(f, "inflate: invalid Huffman code"),
            Self::DistanceTooFar => write!(f, "inflate: back-reference distance too far"),
            Self::BadZlibHeader => write!(f, "inflate: bad zlib header"),
            Self::AdlerMismatch => write!(f, "inflate: Adler-32 checksum mismatch"),
            Self::OutputTooLarge => write!(f, "inflate: decompressed output exceeded size limit"),
        }
    }
}

impl std::error::Error for InflateError {}

/// Absolute ceiling on decompressed output size. A corrupt or hostile stream
/// (e.g. one whose blocks never terminate, or a "zip bomb") could otherwise
/// grow `out` without bound and exhaust memory. 64 MiB is orders of magnitude
/// above anything `extract` decodes — the largest upstream PNG inflates to
/// well under a megabyte — so a valid stream never approaches this limit;
/// crossing it yields [`InflateError::OutputTooLarge`] instead of an OOM.
const MAX_OUTPUT: usize = 64 * 1024 * 1024;

/// A least-significant-bit-first bit reader over a byte slice, as DEFLATE
/// requires (RFC 1951 §3.1.1).
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read `count` bits (`0..=16`) LSB-first, returned as a value with bit
    /// 0 of the output holding the first bit read.
    fn read_bits(&mut self, count: u32) -> Result<u32, InflateError> {
        let mut value = 0u32;
        for i in 0..count {
            let byte = *self
                .data
                .get(self.byte_pos)
                .ok_or(InflateError::UnexpectedEnd)?;
            let bit = (byte >> self.bit_pos) & 1;
            value |= u32::from(bit) << i;
            self.bit_pos += 1;
            if self.bit_pos == 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }
        Ok(value)
    }

    /// Discard any partial byte, aligning to the next byte boundary (used
    /// before a stored block).
    fn align_to_byte(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], InflateError> {
        debug_assert_eq!(self.bit_pos, 0);
        let end = self
            .byte_pos
            .checked_add(count)
            .ok_or(InflateError::UnexpectedEnd)?;
        let slice = self
            .data
            .get(self.byte_pos..end)
            .ok_or(InflateError::UnexpectedEnd)?;
        self.byte_pos = end;
        Ok(slice)
    }
}

/// A canonical Huffman decode table (RFC 1951 §3.2.2).
///
/// Built from an array of per-symbol code lengths. Decoding walks one bit
/// at a time, tracking `(code, first_code_of_this_length, count_of_this_length)`
/// exactly as the RFC's own decoding procedure describes — simple rather
/// than fast, which is fine at the data sizes `extract` deals with (single
/// PNGs, at most a few hundred KiB each).
struct HuffmanTable {
    /// `counts[len]` = how many symbols have code length `len` (`len` in
    /// `1..=15`; index 0 unused).
    counts: [u16; 16],
    /// Symbols sorted by `(code length, symbol value)` — the canonical
    /// assignment order.
    symbols: Vec<u16>,
}

impl HuffmanTable {
    /// Build a canonical Huffman table from per-symbol code lengths (`0`
    /// meaning "this symbol is unused").
    fn build(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut counts = [0u16; 16];
        for &len in lengths {
            if len > 15 {
                return Err(InflateError::BadHuffmanTable);
            }
            counts[usize::from(len)] += 1;
        }
        // A length-0 "count" isn't a real code length; RFC 1951 requires
        // every real tree have at least one symbol of nonzero length,
        // enforced implicitly by `decode` failing to find valid codes if
        // this table is degenerate.
        counts[0] = 0;

        // Reject an over-subscribed code: `left` tracks how much of the
        // code space remains unassigned, starting from 1 unit at length 0
        // and doubling (twice as much space becomes available) each time
        // the length increases by one, before that length's codes claim
        // their share. If more codes are claimed than the doubled space
        // allows, `left` goes negative -- an invalid table. An
        // *under*-subscribed table (`left > 0` after all 15 lengths) is
        // left unrejected: DEFLATE allows incomplete code tables (most
        // commonly a distance table with a single code), and every
        // encoder-emitted table this pipeline has decoded relies on that
        // leniency being present, not just tolerated.
        let mut left: i32 = 1;
        for &count in &counts[1..] {
            left = (left << 1) - i32::from(count);
            if left < 0 {
                return Err(InflateError::BadHuffmanTable);
            }
        }

        // Offsets into a length-sorted symbol table, one bucket per code
        // length.
        let mut offsets = [0u16; 16];
        for len in 1..16 {
            offsets[len] = offsets[len - 1] + counts[len - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &len) in lengths.iter().enumerate() {
            if len != 0 {
                #[allow(clippy::cast_possible_truncation)]
                let sym16 = sym as u16;
                symbols[usize::from(offsets[usize::from(len)])] = sym16;
                offsets[usize::from(len)] += 1;
            }
        }
        // Truncate away the unused tail left over from symbols with length 0
        // (they were never written into `symbols`, so its logical length is
        // just the sum of `counts`).
        let used: usize = counts.iter().map(|&c| usize::from(c)).sum();
        symbols.truncate(used);

        Ok(Self { counts, symbols })
    }

    /// Decode one symbol from `reader` using this table.
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, InflateError> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= i32::try_from(reader.read_bits(1)?).expect("1 bit fits i32");
            let count = i32::from(self.counts[len]);
            if code - first < count {
                #[allow(clippy::cast_sign_loss)]
                let sym_index = (index + (code - first)) as usize;
                return self
                    .symbols
                    .get(sym_index)
                    .copied()
                    .ok_or(InflateError::InvalidCode);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(InflateError::InvalidCode)
    }
}

/// Fixed Huffman literal/length code lengths (RFC 1951 §3.2.6): 288 symbols
/// (256 literals + end-of-block + 29 length codes, plus 2 unused).
fn fixed_lit_lengths() -> [u8; 288] {
    let mut lengths = [0u8; 288];
    for (i, len) in lengths.iter_mut().enumerate() {
        // 280..=287 shares literal 0..=143's 8-bit length (RFC 1951 §3.2.6);
        // written as its own arm (rather than folded into the `_` wildcard)
        // so the four literal/length ranges the RFC defines stay visible
        // one-to-one in the match.
        *len = match i {
            0..=143 | 280..=287 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => unreachable!("i is a valid index into a 288-element array"),
        };
    }
    lengths
}

/// Fixed Huffman distance code lengths: all 30 codes are 5 bits.
fn fixed_dist_lengths() -> [u8; 30] {
    [5; 30]
}

/// Length base values and extra-bit counts for length codes 257..=285
/// (RFC 1951 §3.2.5).
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Distance base values and extra-bit counts for distance codes 0..=29.
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// The order code-length codes themselves are transmitted in for a dynamic
/// block (RFC 1951 §3.2.7) — deliberately not ascending, a quirk of the
/// format.
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Read a dynamic block's two Huffman tables (literal/length and distance)
/// from `reader` (RFC 1951 §3.2.7).
fn read_dynamic_tables(
    reader: &mut BitReader<'_>,
) -> Result<(HuffmanTable, HuffmanTable), InflateError> {
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;

    let mut cl_lengths = [0u8; 19];
    for &pos in CODE_LENGTH_ORDER.iter().take(hclen) {
        #[allow(clippy::cast_possible_truncation)]
        let len = reader.read_bits(3)? as u8;
        cl_lengths[pos] = len;
    }
    let cl_table = HuffmanTable::build(&cl_lengths)?;

    let mut lengths = Vec::with_capacity(hlit + hdist);
    while lengths.len() < hlit + hdist {
        let sym = cl_table.decode(reader)?;
        match sym {
            0..=15 => {
                #[allow(clippy::cast_possible_truncation)]
                lengths.push(sym as u8);
            }
            16 => {
                let &prev = lengths.last().ok_or(InflateError::BadHuffmanTable)?;
                let repeat = reader.read_bits(2)? + 3;
                for _ in 0..repeat {
                    lengths.push(prev);
                }
            }
            17 => {
                let repeat = reader.read_bits(3)? + 3;
                lengths.resize(lengths.len() + repeat as usize, 0);
            }
            18 => {
                let repeat = reader.read_bits(7)? + 11;
                lengths.resize(lengths.len() + repeat as usize, 0);
            }
            _ => return Err(InflateError::BadHuffmanTable),
        }
    }
    if lengths.len() != hlit + hdist {
        return Err(InflateError::BadHuffmanTable);
    }

    let lit_table = HuffmanTable::build(&lengths[..hlit])?;
    let dist_table = HuffmanTable::build(&lengths[hlit..])?;
    Ok((lit_table, dist_table))
}

/// Inflate one block's worth of literal/length + distance symbols into
/// `out`, using the given tables, stopping at the end-of-block symbol
/// (256).
fn inflate_block(
    reader: &mut BitReader<'_>,
    lit_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    out: &mut Vec<u8>,
) -> Result<(), InflateError> {
    loop {
        let sym = lit_table.decode(reader)?;
        match sym {
            0..=255 => {
                #[allow(clippy::cast_possible_truncation)]
                out.push(sym as u8);
                if out.len() > MAX_OUTPUT {
                    return Err(InflateError::OutputTooLarge);
                }
            }
            256 => return Ok(()),
            257..=285 => {
                let idx = usize::from(sym - 257);
                let extra = reader.read_bits(u32::from(LENGTH_EXTRA[idx]))?;
                let length = usize::from(LENGTH_BASE[idx]) + extra as usize;

                let dist_sym = dist_table.decode(reader)?;
                let dist_idx = usize::from(dist_sym);
                if dist_idx >= DIST_BASE.len() {
                    return Err(InflateError::InvalidCode);
                }
                let dist_extra = reader.read_bits(u32::from(DIST_EXTRA[dist_idx]))?;
                let distance = usize::from(DIST_BASE[dist_idx]) + dist_extra as usize;

                if distance > out.len() {
                    return Err(InflateError::DistanceTooFar);
                }
                let start = out.len() - distance;
                for i in 0..length {
                    let byte = out[start + i];
                    out.push(byte);
                }
                if out.len() > MAX_OUTPUT {
                    return Err(InflateError::OutputTooLarge);
                }
            }
            _ => return Err(InflateError::InvalidCode),
        }
    }
}

/// Inflate a raw DEFLATE stream (RFC 1951), with no zlib framing.
///
/// # Errors
///
/// See [`InflateError`]'s variants for the malformed-stream cases detected.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let mut reader = BitReader::new(data);
    let mut out = Vec::new();

    loop {
        let is_final = reader.read_bits(1)? != 0;
        let block_type = reader.read_bits(2)?;
        match block_type {
            0 => {
                reader.align_to_byte();
                let len_bytes = reader.read_bytes(4)?;
                let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]);
                let nlen = u16::from_le_bytes([len_bytes[2], len_bytes[3]]);
                if len != !nlen {
                    return Err(InflateError::BadStoredBlockLength);
                }
                let literal = reader.read_bytes(usize::from(len))?;
                out.extend_from_slice(literal);
                if out.len() > MAX_OUTPUT {
                    return Err(InflateError::OutputTooLarge);
                }
            }
            1 => {
                let lit_table = HuffmanTable::build(&fixed_lit_lengths())?;
                let dist_table = HuffmanTable::build(&fixed_dist_lengths())?;
                inflate_block(&mut reader, &lit_table, &dist_table, &mut out)?;
            }
            2 => {
                let (lit_table, dist_table) = read_dynamic_tables(&mut reader)?;
                inflate_block(&mut reader, &lit_table, &dist_table, &mut out)?;
            }
            _ => return Err(InflateError::ReservedBlockType),
        }
        if is_final {
            break;
        }
    }
    Ok(out)
}

/// Compute the Adler-32 checksum (RFC 1950 §8.2 / RFC 1950 Annex) of `data`.
fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + u32::from(byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

/// Inflate a zlib-wrapped DEFLATE stream (RFC 1950) — the format PNG's
/// `IDAT` chunks concatenate to. Validates the 2-byte header and the
/// trailing Adler-32 checksum.
///
/// # Errors
///
/// [`InflateError::BadZlibHeader`] if the 2-byte header fails its checks
/// (wrong compression method, bad check bits, or a preset dictionary,
/// which PNG never uses); [`InflateError::AdlerMismatch`] if the trailing
/// checksum does not match; any [`inflate`] error for a malformed DEFLATE
/// body.
pub fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let &[cmf, flg, ref body @ ..] = data else {
        return Err(InflateError::BadZlibHeader);
    };
    let method = cmf & 0x0F;
    let fdict = (flg & 0b0010_0000) != 0;
    if method != 8 || fdict || (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 {
        return Err(InflateError::BadZlibHeader);
    }
    if body.len() < 4 {
        return Err(InflateError::UnexpectedEnd);
    }
    let (deflate_body, trailer) = body.split_at(body.len() - 4);
    let expected_adler = u32::from_be_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);

    let out = inflate(deflate_body)?;
    if adler32(&out) != expected_adler {
        return Err(InflateError::AdlerMismatch);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{adler32, inflate, inflate_zlib, InflateError};

    /// A stored (uncompressed) DEFLATE block is the simplest case: build one
    /// by hand and check the round trip.
    #[test]
    fn stored_block_round_trips() {
        let payload = b"hello, pokeemerald-rs";
        let len = u16::try_from(payload.len()).unwrap();
        let mut data = vec![0b0000_0001]; // BFINAL=1, BTYPE=00 (stored)
        data.extend_from_slice(&len.to_le_bytes());
        data.extend_from_slice(&(!len).to_le_bytes());
        data.extend_from_slice(payload);
        assert_eq!(inflate(&data).unwrap(), payload);
    }

    #[test]
    fn adler32_matches_known_vector() {
        // "Wikipedia" -> 0x11E60398, a commonly cited Adler-32 test vector.
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    /// Round-trip a real zlib stream produced by an external, known-correct
    /// encoder (Python's `zlib.compress`, invoked once to freeze these bytes
    /// as a literal -- no runtime dependency). The source text was chosen
    /// (`random.seed(1)` over a small word list) so `zlib` picks a dynamic
    /// Huffman block (`BTYPE=2`, confirmed by inspecting the first stream
    /// byte) -- this is the path `stored_block_round_trips` and the fixed
    /// table in `fixed_lit_lengths`/`fixed_dist_lengths` don't exercise.
    #[test]
    fn zlib_stream_round_trips_dynamic_huffman() {
        const COMPRESSED: &[u8] = &[
            0x78, 0x9c, 0x55, 0x8d, 0x5b, 0x16, 0x80, 0x20, 0x08, 0x44, 0xb7, 0xc2, 0xd6, 0x2c,
            0xa9, 0x2c, 0x0b, 0x53, 0xec, 0xb5, 0xfa, 0xd2, 0xb0, 0xc7, 0x07, 0x1c, 0xce, 0x0c,
            0x77, 0xa6, 0xf2, 0xb4, 0x4e, 0xc0, 0xc6, 0x62, 0x40, 0x86, 0x39, 0x9a, 0x7a, 0x80,
            0x3e, 0x8e, 0x2e, 0xc8, 0xad, 0xa9, 0x7d, 0xc6, 0x29, 0x8b, 0xcc, 0x08, 0x56, 0x1d,
            0x3b, 0x34, 0xb4, 0x7d, 0x5e, 0xb8, 0x13, 0x39, 0xaf, 0x12, 0x97, 0xd4, 0xe0, 0xbc,
            0xb9, 0x98, 0xf4, 0x74, 0xe7, 0x8a, 0x90, 0xf8, 0x7f, 0x2d, 0x2d, 0xe8, 0x33, 0x52,
            0xa6, 0xf4, 0x39, 0x1a, 0x10, 0x47, 0xf4, 0xca, 0xea, 0xb7, 0xa8, 0x98, 0x29, 0x27,
            0x0b, 0x92, 0x9b, 0xc1, 0x0f, 0x70, 0xf9, 0x27, 0xf2, 0xc4, 0x55, 0x06,
        ];
        const EXPECTED: &[u8] = b"brown tileset quick jumps quick dog dog dog palette lazy fox \
quick dog the lazy lazy tileset the sprite dog jumps sprite fox tileset quick over the the the \
palette pokeemerald the lazy palette fox lazy sprite the pokeemerald fox";
        assert_eq!(inflate_zlib(COMPRESSED).unwrap(), EXPECTED);
    }

    #[test]
    fn bad_zlib_header_is_rejected() {
        let err = inflate_zlib(&[0x00, 0x00]).unwrap_err();
        assert_eq!(err, InflateError::BadZlibHeader);
    }

    #[test]
    fn truncated_stream_is_rejected() {
        let err = inflate(&[]).unwrap_err();
        assert_eq!(err, InflateError::UnexpectedEnd);
    }
}
