//! Indexed PNG decoding for extracted graphics.
//!
//! [`decode`] accepts non-interlaced, indexed images with 2-, 4-, or 8-bit
//! pixels and PNG's standard compression and filter methods. It verifies each
//! parsed chunk's CRC, ignores ancillary chunks, and returns unpacked
//! palette-index pixels in scanline order. [`decode_palette`] reads an embedded
//! `PLTE` chunk for assets whose in-game palette comes from the PNG itself.

use std::fmt;

use super::inflate::{self, InflateError};
use super::jasc_pal::Rgb888;

/// An error produced while decoding a PNG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PngError {
    /// The PNG signature is missing or invalid.
    BadSignature,
    /// A chunk is incomplete or a required `IHDR`, `IDAT`, or `IEND` chunk is
    /// missing.
    Truncated,
    /// The `IHDR` describes an unsupported image shape.
    Unsupported(&'static str),
    /// The `IDAT` stream could not be inflated.
    Inflate(InflateError),
    /// A scanline has an unknown filter type.
    BadFilterType(u8),
    /// The inflated data does not contain every declared scanline.
    PixelDataTooShort,
    /// A chunk's CRC does not match its type and data.
    ChunkCrcMismatch([u8; 4]),
    /// The `PLTE` chunk is missing, empty, or contains a partial RGB entry.
    MissingOrBadPalette,
    /// The `PLTE` chunk exceeds PNG's 256-entry limit.
    TooManyPaletteEntries(usize),
}

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSignature => write!(f, "not a PNG file (bad signature)"),
            Self::Truncated => write!(f, "PNG file truncated or missing a required chunk"),
            Self::Unsupported(what) => write!(f, "unsupported PNG shape: {what}"),
            Self::Inflate(err) => write!(f, "PNG IDAT stream: {err}"),
            Self::BadFilterType(byte) => write!(f, "invalid PNG scanline filter type {byte}"),
            Self::PixelDataTooShort => {
                write!(f, "PNG pixel data shorter than IHDR dimensions imply")
            }
            Self::ChunkCrcMismatch(kind) => write!(
                f,
                "PNG {} chunk CRC mismatch",
                String::from_utf8_lossy(kind)
            ),
            Self::MissingOrBadPalette => {
                write!(
                    f,
                    "PNG has no PLTE chunk, an empty PLTE chunk, or a PLTE length that isn't a multiple of 3"
                )
            }
            Self::TooManyPaletteEntries(count) => write!(
                f,
                "PNG PLTE chunk declares {count} entries: the format allows at most 256"
            ),
        }
    }
}

impl std::error::Error for PngError {}

impl From<InflateError> for PngError {
    fn from(err: InflateError) -> Self {
        Self::Inflate(err)
    }
}

/// A decoded indexed-colour bitmap in PNG scanline order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The source PNG's bit depth; each pixel is unpacked to one byte.
    pub bit_depth: u8,
    /// Palette indices in row-major order.
    pub pixels: Vec<u8>,
    /// Embedded `PLTE` entries in index order, or empty when `PLTE` is absent.
    pub palette: Vec<[u8; 3]>,
}

const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
const CHUNK_LENGTH_SIZE: usize = size_of::<u32>();
const CHUNK_KIND_SIZE: usize = 4;
const CHUNK_HEADER_SIZE: usize = CHUNK_LENGTH_SIZE + CHUNK_KIND_SIZE;
const CHUNK_CRC_SIZE: usize = size_of::<u32>();
const IHDR_SIZE: usize = 13;
const IHDR: [u8; CHUNK_KIND_SIZE] = *b"IHDR";
const PLTE: [u8; CHUNK_KIND_SIZE] = *b"PLTE";
const IDAT: [u8; CHUNK_KIND_SIZE] = *b"IDAT";
const IEND: [u8; CHUNK_KIND_SIZE] = *b"IEND";
const INDEXED_COLOR_TYPE: u8 = 3;
const DEFLATE_COMPRESSION_METHOD: u8 = 0;
const ADAPTIVE_FILTER_METHOD: u8 = 0;
const NO_INTERLACE: u8 = 0;
const TWO_BIT_DEPTH: u8 = 2;
const FOUR_BIT_DEPTH: u8 = 4;
const EIGHT_BIT_DEPTH: u8 = 8;
const RGB_CHANNEL_COUNT: usize = 3;
const MAX_PALETTE_ENTRIES: usize = 256;
const CRC32_REFLECTED_ISO_3309_POLYNOMIAL: u32 = 0xEDB8_8320;

struct Chunk<'a> {
    kind: [u8; CHUNK_KIND_SIZE],
    data: &'a [u8],
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..u8::BITS {
            let low_bit_mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (CRC32_REFLECTED_ISO_3309_POLYNOMIAL & low_bit_mask);
        }
    }
    !crc
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("caller supplies four bytes"))
}

fn read_chunks(mut rest: &[u8]) -> Result<Vec<Chunk<'_>>, PngError> {
    let mut chunks = Vec::new();
    let mut saw_iend = false;
    while !rest.is_empty() {
        let header = rest.get(..CHUNK_HEADER_SIZE).ok_or(PngError::Truncated)?;
        let data_len = read_u32(&header[..CHUNK_LENGTH_SIZE]) as usize;
        let kind = header[CHUNK_LENGTH_SIZE..]
            .try_into()
            .expect("chunk header contains a four-byte type");
        let data_start = CHUNK_HEADER_SIZE;
        let data_end = data_start
            .checked_add(data_len)
            .ok_or(PngError::Truncated)?;
        let crc_end = data_end
            .checked_add(CHUNK_CRC_SIZE)
            .ok_or(PngError::Truncated)?;
        if rest.len() < crc_end {
            return Err(PngError::Truncated);
        }
        let stored_crc = read_u32(&rest[data_end..crc_end]);
        let crc_input = &rest[CHUNK_LENGTH_SIZE..data_end];
        if crc32(crc_input) != stored_crc {
            return Err(PngError::ChunkCrcMismatch(kind));
        }
        let data = &rest[data_start..data_end];
        let is_end = kind == IEND;
        chunks.push(Chunk { kind, data });
        rest = &rest[crc_end..];
        if is_end {
            saw_iend = true;
            break;
        }
    }
    if !saw_iend {
        return Err(PngError::Truncated);
    }
    Ok(chunks)
}

struct Header {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    compression_method: u8,
    filter_method: u8,
    interlace_method: u8,
}

impl Header {
    fn parse(data: &[u8]) -> Result<Self, PngError> {
        let data: &[u8; IHDR_SIZE] = data.try_into().map_err(|_| PngError::Truncated)?;
        let &[width_0, width_1, width_2, width_3, height_0, height_1, height_2, height_3, bit_depth, color_type, compression_method, filter_method, interlace_method] =
            data;

        Ok(Self {
            width: u32::from_be_bytes([width_0, width_1, width_2, width_3]),
            height: u32::from_be_bytes([height_0, height_1, height_2, height_3]),
            bit_depth,
            color_type,
            compression_method,
            filter_method,
            interlace_method,
        })
    }

    fn validate(&self) -> Result<(), PngError> {
        if self.color_type != INDEXED_COLOR_TYPE {
            return Err(PngError::Unsupported("colour type is not 3 (indexed)"));
        }
        if !matches!(
            self.bit_depth,
            TWO_BIT_DEPTH | FOUR_BIT_DEPTH | EIGHT_BIT_DEPTH
        ) {
            return Err(PngError::Unsupported("bit depth is not 2, 4, or 8"));
        }
        if self.compression_method != DEFLATE_COMPRESSION_METHOD {
            return Err(PngError::Unsupported("compression method is not 0"));
        }
        if self.filter_method != ADAPTIVE_FILTER_METHOD {
            return Err(PngError::Unsupported("filter method is not 0"));
        }
        if self.interlace_method != NO_INTERLACE {
            return Err(PngError::Unsupported("image is interlaced"));
        }
        Ok(())
    }
}

/// Decode a PNG file's bytes into an [`IndexedImage`].
///
/// # Errors
///
/// Returns [`PngError`] when framing, the supported image shape, compressed
/// data, scanline filters, or an optional palette is invalid.
pub fn decode(data: &[u8]) -> Result<IndexedImage, PngError> {
    let rest = data
        .strip_prefix(&SIGNATURE)
        .ok_or(PngError::BadSignature)?;
    let chunks = read_chunks(rest)?;

    let ihdr = chunks
        .iter()
        .find(|chunk| chunk.kind == IHDR)
        .ok_or(PngError::Truncated)?;
    let header = Header::parse(ihdr.data)?;
    header.validate()?;

    let mut idat = Vec::new();
    for chunk in &chunks {
        if chunk.kind == IDAT {
            idat.extend_from_slice(chunk.data);
        }
    }
    if idat.is_empty() {
        return Err(PngError::Truncated);
    }

    let raw = inflate::inflate_zlib(&idat)?;
    let pixels = defilter_and_unpack(&raw, header.width, header.height, header.bit_depth)?;
    let palette = match chunks.iter().find(|chunk| chunk.kind == PLTE) {
        Some(chunk) => parse_plte(chunk.data)?,
        None => Vec::new(),
    };

    Ok(IndexedImage {
        width: header.width,
        height: header.height,
        bit_depth: header.bit_depth,
        pixels,
        palette,
    })
}

fn parse_plte(data: &[u8]) -> Result<Vec<[u8; 3]>, PngError> {
    if data.is_empty() || !data.len().is_multiple_of(RGB_CHANNEL_COUNT) {
        return Err(PngError::MissingOrBadPalette);
    }
    let count = data.len() / RGB_CHANNEL_COUNT;
    if count > MAX_PALETTE_ENTRIES {
        return Err(PngError::TooManyPaletteEntries(count));
    }
    Ok(data
        .chunks_exact(RGB_CHANNEL_COUNT)
        .map(|channels| [channels[0], channels[1], channels[2]])
        .collect())
}

const FILTER_PREFIX_SIZE: usize = 1;
const INDEXED_FILTER_PIXEL_STRIDE: usize = 1;
const FILTER_NONE: u8 = 0;
const FILTER_SUB: u8 = 1;
const FILTER_UP: u8 = 2;
const FILTER_AVERAGE: u8 = 3;
const FILTER_PAETH: u8 = 4;
const NIBBLES_PER_BYTE: usize = 2;
const HIGH_NIBBLE_SHIFT: u32 = 4;
const NIBBLE_MASK: u8 = 0x0F;
const TWO_BIT_INDICES_PER_BYTE: usize = 4;
const TWO_BIT_INDEX_MASK: u8 = 0b11;

fn defilter_and_unpack(
    raw: &[u8],
    width: u32,
    height: u32,
    bit_depth: u8,
) -> Result<Vec<u8>, PngError> {
    let width = width as usize;
    let height = height as usize;
    let packed_row_bytes = (width * usize::from(bit_depth)).div_ceil(8);
    let scanline_size = FILTER_PREFIX_SIZE + packed_row_bytes;

    if raw.len() < scanline_size * height {
        return Err(PngError::PixelDataTooShort);
    }

    let mut previous_row = vec![0u8; packed_row_bytes];
    let mut pixels = Vec::with_capacity(width * height);

    for row in 0..height {
        let row_start = row * scanline_size;
        let filter_type = raw[row_start];
        let filtered_row = &raw[row_start + FILTER_PREFIX_SIZE..row_start + scanline_size];

        let mut current_row = vec![0u8; packed_row_bytes];
        for column in 0..packed_row_bytes {
            let filtered_byte = i32::from(filtered_row[column]);
            // PNG filters sub-byte indexed pixels by packed byte, so every
            // supported depth uses the same one-byte left-neighbor stride.
            let left = if column >= INDEXED_FILTER_PIXEL_STRIDE {
                i32::from(current_row[column - INDEXED_FILTER_PIXEL_STRIDE])
            } else {
                0
            };
            let above = i32::from(previous_row[column]);
            let upper_left = if column >= INDEXED_FILTER_PIXEL_STRIDE {
                i32::from(previous_row[column - INDEXED_FILTER_PIXEL_STRIDE])
            } else {
                0
            };
            let reconstructed = match filter_type {
                FILTER_NONE => filtered_byte,
                FILTER_SUB => filtered_byte + left,
                FILTER_UP => filtered_byte + above,
                FILTER_AVERAGE => filtered_byte + i32::midpoint(left, above),
                FILTER_PAETH => filtered_byte + paeth_predictor(left, above, upper_left),
                other => return Err(PngError::BadFilterType(other)),
            };
            let byte = u8::try_from(reconstructed & i32::from(u8::MAX))
                .expect("reconstructed byte is masked to eight bits");
            current_row[column] = byte;
        }

        unpack_row(&current_row, width, bit_depth, &mut pixels);
        previous_row = current_row;
    }

    Ok(pixels)
}

fn paeth_predictor(left: i32, above: i32, upper_left: i32) -> i32 {
    let estimate = left + above - upper_left;
    let left_distance = (estimate - left).abs();
    let above_distance = (estimate - above).abs();
    let upper_left_distance = (estimate - upper_left).abs();
    if left_distance <= above_distance && left_distance <= upper_left_distance {
        left
    } else if above_distance <= upper_left_distance {
        above
    } else {
        upper_left
    }
}

fn unpack_row(packed: &[u8], width: usize, bit_depth: u8, out: &mut Vec<u8>) {
    match bit_depth {
        EIGHT_BIT_DEPTH => out.extend_from_slice(&packed[..width]),
        FOUR_BIT_DEPTH => {
            for column in 0..width {
                let byte = packed[column / NIBBLES_PER_BYTE];
                let index = if column.is_multiple_of(NIBBLES_PER_BYTE) {
                    byte >> HIGH_NIBBLE_SHIFT
                } else {
                    byte & NIBBLE_MASK
                };
                out.push(index);
            }
        }
        TWO_BIT_DEPTH => {
            for column in 0..width {
                let byte = packed[column / TWO_BIT_INDICES_PER_BYTE];
                let index_in_byte = column % TWO_BIT_INDICES_PER_BYTE;
                let shift = u8::BITS
                    - u32::from(TWO_BIT_DEPTH)
                        * u32::try_from(index_in_byte + 1).expect("index fits in u32");
                let index = (byte >> shift) & TWO_BIT_INDEX_MASK;
                out.push(index);
            }
        }
        _ => unreachable!("bit depth already validated to be 2, 4, or 8"),
    }
}

/// Reads a PNG's embedded palette in index order.
///
/// # Errors
///
/// Returns [`PngError`] when framing, a parsed chunk's CRC, or the `PLTE`
/// shape is invalid.
pub fn decode_palette(data: &[u8]) -> Result<Vec<Rgb888>, PngError> {
    let rest = data
        .strip_prefix(&SIGNATURE)
        .ok_or(PngError::BadSignature)?;
    let chunks = read_chunks(rest)?;

    let plte = chunks
        .iter()
        .find(|chunk| chunk.kind == PLTE)
        .ok_or(PngError::MissingOrBadPalette)?;

    Ok(parse_plte(plte.data)?
        .into_iter()
        .map(|[r, g, b]| Rgb888 { r, g, b })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{decode, decode_palette, paeth_predictor, PngError};
    use crate::extract::jasc_pal::Rgb888;

    const ADLER_MODULUS: u32 = 65_521;
    const ZLIB_STORED_HEADER: [u8; 2] = [0x78, 0x01];
    const FINAL_STORED_BLOCK_HEADER: u8 = 0b0000_0001;

    fn adler32(data: &[u8]) -> u32 {
        let mut a: u32 = 1;
        let mut b: u32 = 0;
        for &byte in data {
            a = (a + u32::from(byte)) % ADLER_MODULUS;
            b = (b + a) % ADLER_MODULUS;
        }
        (b << 16) | a
    }

    #[test]
    fn crc32_matches_standard_check_value() {
        const ISO_3309_CHECK_VALUE: u32 = 0xCBF4_3926;

        assert_eq!(super::crc32(b"123456789"), ISO_3309_CHECK_VALUE);
    }

    fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        out.extend_from_slice(&kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&super::crc32(&out[super::CHUNK_LENGTH_SIZE..]).to_be_bytes());
        out
    }

    fn ihdr(bit_depth: u8, color_type: u8, width: u32, height: u32) -> Vec<u8> {
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.push(bit_depth);
        ihdr.push(color_type);
        ihdr.push(super::DEFLATE_COMPRESSION_METHOD);
        ihdr.push(super::ADAPTIVE_FILTER_METHOD);
        ihdr.push(super::NO_INTERLACE);
        ihdr
    }

    fn stored_zlib(raw: &[u8]) -> Vec<u8> {
        let mut zlib_body = ZLIB_STORED_HEADER.to_vec();
        let len = u16::try_from(raw.len()).unwrap();
        zlib_body.push(FINAL_STORED_BLOCK_HEADER);
        zlib_body.extend_from_slice(&len.to_le_bytes());
        zlib_body.extend_from_slice(&(!len).to_le_bytes());
        zlib_body.extend_from_slice(raw);
        zlib_body.extend_from_slice(&adler32(raw).to_be_bytes());
        zlib_body
    }

    fn indexed_png(
        bit_depth: u8,
        width: u32,
        height: u32,
        raw: &[u8],
        palette: Option<&[u8]>,
    ) -> Vec<u8> {
        let mut png = Vec::new();
        png.extend_from_slice(&super::SIGNATURE);
        png.extend_from_slice(&chunk(
            super::IHDR,
            &ihdr(bit_depth, super::INDEXED_COLOR_TYPE, width, height),
        ));
        if let Some(palette) = palette {
            png.extend_from_slice(&chunk(super::PLTE, palette));
        }
        png.extend_from_slice(&chunk(super::IDAT, &stored_zlib(raw)));
        png.extend_from_slice(&chunk(super::IEND, &[]));
        png
    }

    fn indexed_png_from_raw(bit_depth: u8, width: u32, height: u32, raw: &[u8]) -> Vec<u8> {
        indexed_png(bit_depth, width, height, raw, None)
    }

    fn indexed_png_from_raw_with_plte(
        bit_depth: u8,
        width: u32,
        height: u32,
        raw: &[u8],
        plte: &[u8],
    ) -> Vec<u8> {
        indexed_png(bit_depth, width, height, raw, Some(plte))
    }

    fn tiny_indexed_png(bit_depth: u8, width: u32, height: u32, packed_rows: &[u8]) -> Vec<u8> {
        let packed_row_bytes = packed_rows.len() / usize::try_from(height).unwrap();
        let mut raw = Vec::new();
        for row in 0..height as usize {
            raw.push(super::FILTER_NONE);
            raw.extend_from_slice(
                &packed_rows[row * packed_row_bytes..(row + 1) * packed_row_bytes],
            );
        }
        indexed_png_from_raw(bit_depth, width, height, &raw)
    }

    fn filtered_indexed_png(
        bit_depth: u8,
        width: u32,
        height: u32,
        rows: &[(u8, &[u8])],
    ) -> Vec<u8> {
        let mut raw = Vec::new();
        for (filter_type, packed) in rows {
            raw.push(*filter_type);
            raw.extend_from_slice(packed);
        }
        indexed_png_from_raw(bit_depth, width, height, &raw)
    }

    #[test]
    fn decodes_8bit_indexed_2x2() {
        let png = tiny_indexed_png(super::EIGHT_BIT_DEPTH, 2, 2, &[1, 2, 3, 0]);
        let image = decode(&png).unwrap();
        assert_eq!(image.width, 2);
        assert_eq!(image.height, 2);
        assert_eq!(image.bit_depth, super::EIGHT_BIT_DEPTH);
        assert_eq!(image.pixels, vec![1, 2, 3, 0]);
    }

    #[test]
    fn decodes_4bit_indexed_row() {
        let png = tiny_indexed_png(super::FOUR_BIT_DEPTH, 2, 1, &[0xAB]);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![0x0A, 0x0B]);
    }

    #[test]
    fn decodes_2bit_indexed_row() {
        let png = tiny_indexed_png(super::TWO_BIT_DEPTH, 4, 1, &[0b01_10_11_00]);
        let image = decode(&png).unwrap();
        assert_eq!(image.bit_depth, super::TWO_BIT_DEPTH);
        assert_eq!(image.pixels, vec![1, 2, 3, 0]);
    }

    #[test]
    fn decodes_2bit_indexed_two_rows() {
        let packed_rows = [0b00_01_10_11, 0b11_10_01_00, 0xFF, 0x00];
        let png = tiny_indexed_png(super::TWO_BIT_DEPTH, 8, 2, &packed_rows);
        let image = decode(&png).unwrap();
        assert_eq!(
            image.pixels,
            vec![0, 1, 2, 3, 3, 2, 1, 0, 3, 3, 3, 3, 0, 0, 0, 0]
        );
    }

    #[test]
    fn rejects_bit_depth_1() {
        let mut png = Vec::new();
        png.extend_from_slice(&super::SIGNATURE);
        png.extend_from_slice(&chunk(
            super::IHDR,
            &ihdr(1, super::INDEXED_COLOR_TYPE, 1, 1),
        ));
        png.extend_from_slice(&chunk(super::IEND, &[]));
        let err = decode(&png).unwrap_err();
        assert_eq!(err, PngError::Unsupported("bit depth is not 2, 4, or 8"));
    }

    #[test]
    fn rejects_bad_signature() {
        let err = decode(&[0u8; 16]).unwrap_err();
        assert_eq!(err, PngError::BadSignature);
    }

    #[test]
    fn rejects_missing_iend() {
        let mut png = tiny_indexed_png(super::EIGHT_BIT_DEPTH, 1, 1, &[0]);
        let iend_size = chunk(super::IEND, &[]).len();
        png.truncate(png.len() - iend_size);

        let err = decode(&png).unwrap_err();
        assert_eq!(err, PngError::Truncated);
    }

    #[test]
    fn rejects_truecolor() {
        const TRUECOLOR_TYPE: u8 = 2;

        let mut png = Vec::new();
        png.extend_from_slice(&super::SIGNATURE);
        png.extend_from_slice(&chunk(
            super::IHDR,
            &ihdr(super::EIGHT_BIT_DEPTH, TRUECOLOR_TYPE, 1, 1),
        ));
        png.extend_from_slice(&chunk(super::IEND, &[]));
        let err = decode(&png).unwrap_err();
        assert_eq!(err, PngError::Unsupported("colour type is not 3 (indexed)"));
    }

    #[test]
    fn sub_filter_reconstructs_from_left_bytes() {
        let rows = [(super::FILTER_SUB, &[10, 5, 3, 250][..])];
        let png = filtered_indexed_png(super::EIGHT_BIT_DEPTH, 4, 1, &rows);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![10, 15, 18, 12]);
    }

    #[test]
    fn up_filter_reconstructs_from_previous_row() {
        let rows = [
            (super::FILTER_NONE, &[1, 2, 3, 4][..]),
            (super::FILTER_UP, &[10, 20, 30, 40][..]),
        ];
        let png = filtered_indexed_png(super::EIGHT_BIT_DEPTH, 4, 2, &rows);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![1, 2, 3, 4, 11, 22, 33, 44]);
    }

    #[test]
    fn average_filter_reconstructs_from_left_and_previous_row() {
        let rows = [
            (super::FILTER_NONE, &[4, 8, 10, 20][..]),
            (super::FILTER_AVERAGE, &[2, 3, 4, 5][..]),
        ];
        let png = filtered_indexed_png(super::EIGHT_BIT_DEPTH, 4, 2, &rows);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![4, 8, 10, 20, 4, 9, 13, 21]);
    }

    #[test]
    fn paeth_filter_reconstructs_from_nearest_neighbor() {
        let rows = [
            (super::FILTER_NONE, &[8, 3, 10, 200][..]),
            (super::FILTER_PAETH, &[5, 0, 250, 1][..]),
        ];
        let png = filtered_indexed_png(super::EIGHT_BIT_DEPTH, 4, 2, &rows);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![8, 3, 10, 200, 13, 8, 4, 201]);
    }

    #[test]
    fn paeth_predictor_selects_each_neighbor() {
        assert_eq!(paeth_predictor(5, 20, 18), 5, "left wins");
        assert_eq!(paeth_predictor(20, 5, 18), 5, "above wins");
        assert_eq!(paeth_predictor(13, 3, 8), 8, "upper-left wins");
    }

    fn indexed_png_with_palette(colors: &[(u8, u8, u8)]) -> Vec<u8> {
        let mut plte = Vec::new();
        for &(r, g, b) in colors {
            plte.extend_from_slice(&[r, g, b]);
        }
        indexed_png_from_raw_with_plte(super::FOUR_BIT_DEPTH, 1, 1, &[super::FILTER_NONE, 0], &plte)
    }

    #[test]
    fn decode_reads_an_embedded_plte_chunk() {
        const RGB_PALETTE: [u8; 9] = [u8::MAX, 0, 0, 0, u8::MAX, 0, 0, 0, u8::MAX];

        let raw = [super::FILTER_NONE, 1, 2];
        let png = indexed_png_from_raw_with_plte(super::EIGHT_BIT_DEPTH, 2, 1, &raw, &RGB_PALETTE);
        let image = decode(&png).unwrap();
        assert_eq!(
            image.palette,
            vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]],
            "PLTE triples decode in index order"
        );
    }

    #[test]
    fn decode_palette_reads_plte_entries_in_order() {
        let png = indexed_png_with_palette(&[(115, 205, 164), (255, 255, 255), (0, 0, 0)]);
        let colors = decode_palette(&png).unwrap();
        assert_eq!(
            colors,
            vec![
                Rgb888 {
                    r: 115,
                    g: 205,
                    b: 164
                },
                Rgb888 {
                    r: 255,
                    g: 255,
                    b: 255
                },
                Rgb888 { r: 0, g: 0, b: 0 },
            ]
        );
    }

    #[test]
    fn decode_without_a_plte_chunk_leaves_the_palette_empty() {
        let png = tiny_indexed_png(super::EIGHT_BIT_DEPTH, 2, 2, &[1, 2, 3, 0]);
        let image = decode(&png).unwrap();
        assert!(image.palette.is_empty());
    }

    #[test]
    fn parse_plte_rejects_a_trailing_partial_entry() {
        let plte = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        assert_eq!(
            super::parse_plte(&plte).unwrap_err(),
            PngError::MissingOrBadPalette
        );
        assert_eq!(
            super::parse_plte(&[]).unwrap_err(),
            PngError::MissingOrBadPalette
        );
        assert_eq!(
            super::parse_plte(&[1, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap(),
            vec![[1, 2, 3], [4, 5, 6], [7, 8, 9]]
        );
    }

    #[test]
    fn parse_plte_accepts_exactly_256_entries_but_rejects_257() {
        let at_cap = vec![0u8; super::MAX_PALETTE_ENTRIES * super::RGB_CHANNEL_COUNT];
        assert_eq!(
            super::parse_plte(&at_cap).unwrap().len(),
            super::MAX_PALETTE_ENTRIES
        );

        let over_cap_entries = super::MAX_PALETTE_ENTRIES + 1;
        let over_cap = vec![0u8; over_cap_entries * super::RGB_CHANNEL_COUNT];
        assert_eq!(
            super::parse_plte(&over_cap).unwrap_err(),
            PngError::TooManyPaletteEntries(over_cap_entries)
        );
    }

    #[test]
    fn decode_palette_rejects_missing_plte() {
        let png = tiny_indexed_png(super::EIGHT_BIT_DEPTH, 2, 1, &[1, 2]);
        let err = decode_palette(&png).unwrap_err();
        assert_eq!(err, PngError::MissingOrBadPalette);
    }

    #[test]
    fn decode_palette_rejects_empty_plte() {
        let png = indexed_png_with_palette(&[]);
        let err = decode_palette(&png).unwrap_err();
        assert_eq!(err, PngError::MissingOrBadPalette);
    }

    #[test]
    fn decode_palette_rejects_corrupt_plte_crc() {
        let mut png = indexed_png_with_palette(&[(115, 205, 164)]);
        let plte_kind_offset = png
            .windows(super::PLTE.len())
            .position(|window| window == super::PLTE)
            .unwrap();
        let plte_data_offset = plte_kind_offset + super::PLTE.len();
        png[plte_data_offset] ^= u8::MAX;
        let err = decode_palette(&png).unwrap_err();
        assert_eq!(err, PngError::ChunkCrcMismatch(*b"PLTE"));
    }

    #[test]
    fn decode_palette_rejects_a_plte_length_not_a_multiple_of_three() {
        let png = indexed_png_from_raw_with_plte(
            super::FOUR_BIT_DEPTH,
            1,
            1,
            &[super::FILTER_NONE, 0],
            &[1, 2],
        );
        let err = decode_palette(&png).unwrap_err();
        assert_eq!(err, PngError::MissingOrBadPalette);
    }
}
