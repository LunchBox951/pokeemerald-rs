//! DEFLATE and zlib decoding for PNG asset extraction.
//!
//! Supports stored, fixed-Huffman, and dynamic-Huffman blocks and validates
//! zlib headers and Adler-32 trailers.

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
    /// Huffman code lengths exceeded the format limit, over-subscribed the
    /// code space, or contained an invalid repeat.
    BadHuffmanTable,
    /// A bit sequence or distance symbol did not map to a valid code.
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
    /// The decompressed output exceeded the 64 MiB safety limit.
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

const MAX_DECOMPRESSED_SIZE: usize = 64 * 1024 * 1024;

/// Reads DEFLATE fields in their least-significant-bit-first stream order.
///
/// RFC 1951 section 3.1.1 defines this order separately from Huffman code order.
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_offset: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_offset: 0,
        }
    }

    fn read_bits(&mut self, count: u32) -> Result<u32, InflateError> {
        let mut value = 0u32;
        for output_bit in 0..count {
            let byte = *self
                .data
                .get(self.byte_pos)
                .ok_or(InflateError::UnexpectedEnd)?;
            let bit = (byte >> self.bit_offset) & 1;
            value |= u32::from(bit) << output_bit;
            self.bit_offset += 1;
            if self.bit_offset == 8 {
                self.bit_offset = 0;
                self.byte_pos += 1;
            }
        }
        Ok(value)
    }

    fn align_to_byte(&mut self) {
        if self.bit_offset != 0 {
            self.bit_offset = 0;
            self.byte_pos += 1;
        }
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], InflateError> {
        debug_assert_eq!(self.bit_offset, 0);
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

/// Canonical Huffman decoder from RFC 1951 section 3.2.2.
struct HuffmanTable {
    symbol_counts_by_length: [u16; 16],
    symbols_by_length: Vec<u16>,
}

impl HuffmanTable {
    fn build(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut symbol_counts_by_length = [0u16; 16];
        for &length in lengths {
            if length > 15 {
                return Err(InflateError::BadHuffmanTable);
            }
            symbol_counts_by_length[usize::from(length)] += 1;
        }
        symbol_counts_by_length[0] = 0;

        let mut remaining_code_space: i32 = 1;
        for &symbol_count in &symbol_counts_by_length[1..] {
            remaining_code_space = (remaining_code_space << 1) - i32::from(symbol_count);
            if remaining_code_space < 0 {
                return Err(InflateError::BadHuffmanTable);
            }
        }
        // RFC 1951 section 3.2.7 permits a one-symbol distance tree, so
        // `remaining_code_space` may be positive.

        let mut next_symbol_by_length = [0u16; 16];
        for length in 1..16 {
            next_symbol_by_length[length] =
                next_symbol_by_length[length - 1] + symbol_counts_by_length[length - 1];
        }
        let symbol_count = symbol_counts_by_length
            .iter()
            .map(|&count| usize::from(count))
            .sum();
        let mut symbols_by_length = vec![0u16; symbol_count];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                let symbol = u16::try_from(symbol).expect("DEFLATE alphabets fit in u16");
                let symbol_index = &mut next_symbol_by_length[usize::from(length)];
                symbols_by_length[usize::from(*symbol_index)] = symbol;
                *symbol_index += 1;
            }
        }

        Ok(Self {
            symbol_counts_by_length,
            symbols_by_length,
        })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, InflateError> {
        let mut code: i32 = 0;
        let mut first_code_of_length: i32 = 0;
        let mut first_symbol_of_length: i32 = 0;
        for length in 1..16 {
            code |= i32::try_from(reader.read_bits(1)?).expect("1 bit fits i32");
            let symbol_count = i32::from(self.symbol_counts_by_length[length]);
            if code - first_code_of_length < symbol_count {
                let symbol_index =
                    usize::try_from(first_symbol_of_length + (code - first_code_of_length))
                        .expect("canonical Huffman symbol indexes are non-negative");
                return self
                    .symbols_by_length
                    .get(symbol_index)
                    .copied()
                    .ok_or(InflateError::InvalidCode);
            }
            first_symbol_of_length += symbol_count;
            first_code_of_length = (first_code_of_length + symbol_count) << 1;
            code <<= 1;
        }
        Err(InflateError::InvalidCode)
    }
}

fn fixed_lit_lengths() -> [u8; 288] {
    let mut lengths = [0u8; 288];
    for (symbol, length) in lengths.iter_mut().enumerate() {
        *length = match symbol {
            0..=143 | 280..=287 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => unreachable!("symbol is a valid index into a 288-element array"),
        };
    }
    lengths
}

fn fixed_dist_lengths() -> [u8; 30] {
    [5; 30]
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

// RFC 1951 section 3.2.7 requires this nonascending transmission order.
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn read_dynamic_tables(
    reader: &mut BitReader<'_>,
) -> Result<(HuffmanTable, HuffmanTable), InflateError> {
    let literal_length_code_count =
        usize::try_from(reader.read_bits(5)?).expect("5 bits fit usize") + 257;
    let distance_code_count = usize::try_from(reader.read_bits(5)?).expect("5 bits fit usize") + 1;
    let code_length_code_count =
        usize::try_from(reader.read_bits(4)?).expect("4 bits fit usize") + 4;

    let mut code_length_code_lengths = [0u8; 19];
    for &symbol in CODE_LENGTH_ORDER.iter().take(code_length_code_count) {
        let length = u8::try_from(reader.read_bits(3)?).expect("3 bits fit u8");
        code_length_code_lengths[symbol] = length;
    }
    let code_length_table = HuffmanTable::build(&code_length_code_lengths)?;

    let total_code_count = literal_length_code_count + distance_code_count;
    let mut lengths = Vec::with_capacity(total_code_count);
    while lengths.len() < total_code_count {
        let symbol = code_length_table.decode(reader)?;
        match symbol {
            0..=15 => lengths.push(u8::try_from(symbol).expect("code length fits u8")),
            16 => {
                let &previous_length = lengths.last().ok_or(InflateError::BadHuffmanTable)?;
                let repeat_count =
                    usize::try_from(reader.read_bits(2)? + 3).expect("repeat count fits usize");
                append_code_lengths(
                    &mut lengths,
                    previous_length,
                    repeat_count,
                    total_code_count,
                )?;
            }
            17 => {
                let repeat_count =
                    usize::try_from(reader.read_bits(3)? + 3).expect("repeat count fits usize");
                append_code_lengths(&mut lengths, 0, repeat_count, total_code_count)?;
            }
            18 => {
                let repeat_count =
                    usize::try_from(reader.read_bits(7)? + 11).expect("repeat count fits usize");
                append_code_lengths(&mut lengths, 0, repeat_count, total_code_count)?;
            }
            _ => return Err(InflateError::BadHuffmanTable),
        }
    }

    let literal_length_table = HuffmanTable::build(&lengths[..literal_length_code_count])?;
    let distance_table = HuffmanTable::build(&lengths[literal_length_code_count..])?;
    Ok((literal_length_table, distance_table))
}

fn append_code_lengths(
    lengths: &mut Vec<u8>,
    length: u8,
    repeat_count: usize,
    total_code_count: usize,
) -> Result<(), InflateError> {
    let new_count = lengths
        .len()
        .checked_add(repeat_count)
        .filter(|&count| count <= total_code_count)
        .ok_or(InflateError::BadHuffmanTable)?;
    lengths.resize(new_count, length);
    Ok(())
}

fn inflate_block(
    reader: &mut BitReader<'_>,
    literal_length_table: &HuffmanTable,
    distance_table: &HuffmanTable,
    output: &mut Vec<u8>,
) -> Result<(), InflateError> {
    loop {
        let symbol = literal_length_table.decode(reader)?;
        match symbol {
            0..=255 => {
                output.push(u8::try_from(symbol).expect("literal symbol fits u8"));
                ensure_output_size(output)?;
            }
            256 => return Ok(()),
            257..=285 => {
                let length_index = usize::from(symbol - 257);
                let length_extra = reader.read_bits(u32::from(LENGTH_EXTRA[length_index]))?;
                let length = usize::from(LENGTH_BASE[length_index])
                    + usize::try_from(length_extra).expect("length extra bits fit usize");

                let distance_index = usize::from(distance_table.decode(reader)?);
                if distance_index >= DIST_BASE.len() {
                    return Err(InflateError::InvalidCode);
                }
                let distance_extra = reader.read_bits(u32::from(DIST_EXTRA[distance_index]))?;
                let distance = usize::from(DIST_BASE[distance_index])
                    + usize::try_from(distance_extra).expect("distance extra bits fit usize");

                if distance > output.len() {
                    return Err(InflateError::DistanceTooFar);
                }
                let source_start = output.len() - distance;
                for offset in 0..length {
                    let byte = output[source_start + offset];
                    output.push(byte);
                }
                ensure_output_size(output)?;
            }
            _ => return Err(InflateError::InvalidCode),
        }
    }
}

fn ensure_output_size(output: &[u8]) -> Result<(), InflateError> {
    if output.len() > MAX_DECOMPRESSED_SIZE {
        return Err(InflateError::OutputTooLarge);
    }
    Ok(())
}

const STORED_BLOCK: u32 = 0;
const FIXED_HUFFMAN_BLOCK: u32 = 1;
const DYNAMIC_HUFFMAN_BLOCK: u32 = 2;

/// Inflate a raw DEFLATE stream (RFC 1951), with no zlib framing.
///
/// # Errors
///
/// See [`InflateError`]'s variants for the malformed-stream cases detected.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let mut reader = BitReader::new(data);
    let mut output = Vec::new();

    loop {
        let is_final = reader.read_bits(1)? != 0;
        let block_type = reader.read_bits(2)?;
        match block_type {
            STORED_BLOCK => {
                reader.align_to_byte();
                let length_fields = reader.read_bytes(4)?;
                let length = u16::from_le_bytes([length_fields[0], length_fields[1]]);
                let complemented_length = u16::from_le_bytes([length_fields[2], length_fields[3]]);
                if length != !complemented_length {
                    return Err(InflateError::BadStoredBlockLength);
                }
                let literal_bytes = reader.read_bytes(usize::from(length))?;
                output.extend_from_slice(literal_bytes);
                ensure_output_size(&output)?;
            }
            FIXED_HUFFMAN_BLOCK => {
                let literal_length_table = HuffmanTable::build(&fixed_lit_lengths())?;
                let distance_table = HuffmanTable::build(&fixed_dist_lengths())?;
                inflate_block(
                    &mut reader,
                    &literal_length_table,
                    &distance_table,
                    &mut output,
                )?;
            }
            DYNAMIC_HUFFMAN_BLOCK => {
                let (literal_length_table, distance_table) = read_dynamic_tables(&mut reader)?;
                inflate_block(
                    &mut reader,
                    &literal_length_table,
                    &distance_table,
                    &mut output,
                )?;
            }
            _ => return Err(InflateError::ReservedBlockType),
        }
        if is_final {
            break;
        }
    }
    Ok(output)
}

fn adler32(data: &[u8]) -> u32 {
    const ADLER_MODULUS: u32 = 65_521;

    let mut sum = 1u32;
    let mut weighted_sum = 0u32;
    for &byte in data {
        sum = (sum + u32::from(byte)) % ADLER_MODULUS;
        weighted_sum = (weighted_sum + sum) % ADLER_MODULUS;
    }
    (weighted_sum << 16) | sum
}

const ZLIB_COMPRESSION_METHOD_MASK: u8 = 0x0f;
const ZLIB_DEFLATE_METHOD: u8 = 8;
const ZLIB_PRESET_DICTIONARY_FLAG: u8 = 0b0010_0000;
const ZLIB_HEADER_CHECK_DIVISOR: u16 = 31;
const ZLIB_TRAILER_SIZE: usize = 4;

/// Inflate the zlib stream formed by a PNG's concatenated `IDAT` chunks.
///
/// # Errors
///
/// Returns [`InflateError::BadZlibHeader`] for a non-DEFLATE method, invalid
/// header check bits, or a preset dictionary. Returns
/// [`InflateError::AdlerMismatch`] when the decompressed data does not match
/// the trailing checksum. Malformed DEFLATE data returns the corresponding
/// [`inflate`] error.
pub fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let &[compression_method_and_flags, flags, ref body @ ..] = data else {
        return Err(InflateError::BadZlibHeader);
    };
    let compression_method = compression_method_and_flags & ZLIB_COMPRESSION_METHOD_MASK;
    let uses_preset_dictionary = (flags & ZLIB_PRESET_DICTIONARY_FLAG) != 0;
    let header = u16::from_be_bytes([compression_method_and_flags, flags]);
    if compression_method != ZLIB_DEFLATE_METHOD
        || uses_preset_dictionary
        || header % ZLIB_HEADER_CHECK_DIVISOR != 0
    {
        return Err(InflateError::BadZlibHeader);
    }
    if body.len() < ZLIB_TRAILER_SIZE {
        return Err(InflateError::UnexpectedEnd);
    }
    let (deflate_body, trailer) = body.split_at(body.len() - ZLIB_TRAILER_SIZE);
    let expected_adler = u32::from_be_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);

    let output = inflate(deflate_body)?;
    if adler32(&output) != expected_adler {
        return Err(InflateError::AdlerMismatch);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{adler32, inflate, inflate_zlib, InflateError};

    #[test]
    fn stored_block_decodes_literal_bytes() {
        const FINAL_STORED_BLOCK_HEADER: u8 = 0b0000_0001;

        let payload = b"hello, pokeemerald-rs";
        let length = u16::try_from(payload.len()).unwrap();
        let mut stored_stream = vec![FINAL_STORED_BLOCK_HEADER];
        stored_stream.extend_from_slice(&length.to_le_bytes());
        stored_stream.extend_from_slice(&(!length).to_le_bytes());
        stored_stream.extend_from_slice(payload);

        assert_eq!(inflate(&stored_stream).unwrap(), payload);
    }

    #[test]
    fn adler32_matches_known_vector() {
        const WIKIPEDIA_ADLER32: u32 = 0x11E6_0398;

        assert_eq!(adler32(b"Wikipedia"), WIKIPEDIA_ADLER32);
    }

    #[test]
    fn dynamic_huffman_zlib_stream_decodes() {
        const DYNAMIC_HUFFMAN_ZLIB_STREAM: &[u8] = &[
            0x78, 0x9c, 0x55, 0x8d, 0x5b, 0x16, 0x80, 0x20, 0x08, 0x44, 0xb7, 0xc2, 0xd6, 0x2c,
            0xa9, 0x2c, 0x0b, 0x53, 0xec, 0xb5, 0xfa, 0xd2, 0xb0, 0xc7, 0x07, 0x1c, 0xce, 0x0c,
            0x77, 0xa6, 0xf2, 0xb4, 0x4e, 0xc0, 0xc6, 0x62, 0x40, 0x86, 0x39, 0x9a, 0x7a, 0x80,
            0x3e, 0x8e, 0x2e, 0xc8, 0xad, 0xa9, 0x7d, 0xc6, 0x29, 0x8b, 0xcc, 0x08, 0x56, 0x1d,
            0x3b, 0x34, 0xb4, 0x7d, 0x5e, 0xb8, 0x13, 0x39, 0xaf, 0x12, 0x97, 0xd4, 0xe0, 0xbc,
            0xb9, 0x98, 0xf4, 0x74, 0xe7, 0x8a, 0x90, 0xf8, 0x7f, 0x2d, 0x2d, 0xe8, 0x33, 0x52,
            0xa6, 0xf4, 0x39, 0x1a, 0x10, 0x47, 0xf4, 0xca, 0xea, 0xb7, 0xa8, 0x98, 0x29, 0x27,
            0x0b, 0x92, 0x9b, 0xc1, 0x0f, 0x70, 0xf9, 0x27, 0xf2, 0xc4, 0x55, 0x06,
        ];
        const DECOMPRESSED: &[u8] =
            b"brown tileset quick jumps quick dog dog dog palette lazy fox \
quick dog the lazy lazy tileset the sprite dog jumps sprite fox tileset quick over the the the \
palette pokeemerald the lazy palette fox lazy sprite the pokeemerald fox";

        assert_eq!(
            inflate_zlib(DYNAMIC_HUFFMAN_ZLIB_STREAM).unwrap(),
            DECOMPRESSED
        );
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
