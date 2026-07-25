//! A minimal PNG decoder for exactly the subset of the format upstream's
//! own graphics sources use.
//!
//! `pokeemerald/graphics/**/*.png` and `pokeemerald/data/tilesets/**/*.png`
//! are the *sources* the decomp's build compiles into GBA `.4bpp`/`.gbapal`
//! data (via `gbagfx`); no compiled `.4bpp`/`.gbapal` output is checked into
//! the tree (confirmed by scanning the checkout: zero `.4bpp` files, zero
//! `.gbapal` files under `pokeemerald/graphics` or `pokeemerald/data`). So
//! `extract` decodes the PNGs directly.
//!
//! A survey of every PNG this pack draws from (`find … -iname '*.png'` under
//! `data/tilesets/{primary,secondary}/*`, `graphics/title_screen`, and
//! `graphics/object_events/pics/people`, IHDR-parsed) found exactly one
//! shape: **colour type 3 (palette/indexed), bit depth 4 or 8, compression
//! method 0, filter method 0 (per-scanline adaptive), non-interlaced**.
//! `graphics/fonts/latin_*.png` (S-4, issue #114) add a second, narrower bit
//! depth to that same shape: **bit depth 2** (`gbagfx`'s Latin-font
//! round-trip always emits a 4-colour, 2-bit-per-pixel indexed PNG — see
//! `pokeemerald/tools/gbagfx/font.c`'s `SetFontPalette`/`ReadLatinFont`).
//! Anything outside colour type 3 / bit depth 2, 4, or 8 / compression 0 /
//! filter method 0 / non-interlaced is a hard, typed [`PngError::Unsupported`]
//! rather than a best-effort guess.
//!
//! Ancillary chunks (`gAMA`, `sRGB`, `cHRM`, seen in the survey; no `tRNS`
//! was present anywhere in scope) are skipped unread — GBA tile art carries
//! no alpha channel, so nothing of value would come from them even if
//! present.
//!
//! Decoded output is a row-major, one-byte-per-pixel palette-index bitmap
//! ([`IndexedImage`]) — *not* GBA's packed tile format. The pack stores
//! this simpler, lossless shape rather than pre-packing into 8x8 nibble
//! tiles: no rendering pipeline exists yet to consume a hardware tile
//! layout, and [`decode`]'s own [`PLTE`](https://www.w3.org/TR/png/#11PLTE)
//! is deliberately *not* carried through (upstream tilesets' and fonts' real
//! in-game colours come from the sibling JASC `.pal` files, decoded by
//! [`crate::extract::jasc_pal`], not from the PNG's own preview palette).
//!
//! `graphics/text_window/*.png` (S-4, issue #114) are the one exception:
//! upstream's `INCGFX_U16(..., ".gbapal")` rule for these files reads the
//! palette *from the PNG's own `PLTE` chunk* (no sibling `.pal` file exists
//! for the per-frame border graphics — only the four `text_pal*.pal` extras
//! do). [`decode_palette`] reads exactly that chunk, separately from
//! [`decode`]'s pixel path, so the common tileset/sprite/font case (`PLTE`
//! ignored) stays exactly as it was.

use std::fmt;

use super::inflate::{self, InflateError};
use super::jasc_pal::Rgb888;

/// An error produced while decoding a PNG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PngError {
    /// The 8-byte PNG signature didn't match.
    BadSignature,
    /// A chunk's declared length ran past the end of the file, or a
    /// required chunk (`IHDR`, `IDAT`) was missing.
    Truncated,
    /// `IHDR` described a combination of colour type / bit depth /
    /// compression / filter / interlace method this decoder does not
    /// implement. Carries a short description of what was found.
    Unsupported(&'static str),
    /// The concatenated `IDAT` stream failed to inflate.
    Inflate(InflateError),
    /// A scanline used a filter-type byte outside `0..=4` (RFC 2083 §6.2 /
    /// the PNG spec's five defined filter types).
    BadFilterType(u8),
    /// The inflated pixel data was shorter than `IHDR`'s width/height/depth
    /// require.
    PixelDataTooShort,
    /// [`decode_palette`] was asked to read a `PLTE` chunk that is absent,
    /// empty, or whose length isn't a whole number of 3-byte RGB entries.
    MissingOrBadPalette,
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
            Self::MissingOrBadPalette => {
                write!(
                    f,
                    "PNG has no PLTE chunk, an empty PLTE chunk, or a PLTE length that isn't a multiple of 3"
                )
            }
        }
    }
}

impl std::error::Error for PngError {}

impl From<InflateError> for PngError {
    fn from(err: InflateError) -> Self {
        Self::Inflate(err)
    }
}

/// A decoded indexed-colour bitmap: one palette-index byte per pixel,
/// row-major, top row first (matching PNG's own scanline order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The source PNG's bit depth (2, 4, or 8) — informational only;
    /// `pixels` is always unpacked to one byte per index regardless.
    pub bit_depth: u8,
    /// `width * height` palette-index bytes, row-major.
    pub pixels: Vec<u8>,
}

const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// One raw chunk: its 4-byte type tag and its data (CRC not verified —
/// `extract` already checksums the whole pack for determinism; a corrupt
/// upstream checkout would fail loudly at pixel-unpack time instead).
struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

fn read_chunks(mut rest: &[u8]) -> Result<Vec<Chunk<'_>>, PngError> {
    let mut chunks = Vec::new();
    while !rest.is_empty() {
        if rest.len() < 8 {
            return Err(PngError::Truncated);
        }
        let len = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        let kind = [rest[4], rest[5], rest[6], rest[7]];
        let body_start: usize = 8;
        let body_end = body_start.checked_add(len).ok_or(PngError::Truncated)?;
        let crc_end = body_end.checked_add(4).ok_or(PngError::Truncated)?;
        if rest.len() < crc_end {
            return Err(PngError::Truncated);
        }
        let data = &rest[body_start..body_end];
        let is_end = &kind == b"IEND";
        chunks.push(Chunk { kind, data });
        rest = &rest[crc_end..];
        if is_end {
            break;
        }
    }
    Ok(chunks)
}

/// Decode a PNG file's bytes into an [`IndexedImage`].
///
/// # Errors
///
/// See [`PngError`]'s variants. In particular, [`PngError::Unsupported`]
/// covers every colour type other than indexed (3), every bit depth other
/// than 2, 4, or 8, any interlaced image, and any non-zero compression/filter
/// method — see the module docs for why that subset was chosen.
pub fn decode(data: &[u8]) -> Result<IndexedImage, PngError> {
    if data.len() < 8 || data[..8] != SIGNATURE {
        return Err(PngError::BadSignature);
    }
    let chunks = read_chunks(&data[8..])?;

    let ihdr = chunks
        .iter()
        .find(|c| &c.kind == b"IHDR")
        .ok_or(PngError::Truncated)?;
    if ihdr.data.len() != 13 {
        return Err(PngError::Truncated);
    }
    let width = u32::from_be_bytes([ihdr.data[0], ihdr.data[1], ihdr.data[2], ihdr.data[3]]);
    let height = u32::from_be_bytes([ihdr.data[4], ihdr.data[5], ihdr.data[6], ihdr.data[7]]);
    let bit_depth = ihdr.data[8];
    let color_type = ihdr.data[9];
    let compression = ihdr.data[10];
    let filter_method = ihdr.data[11];
    let interlace = ihdr.data[12];

    if color_type != 3 {
        return Err(PngError::Unsupported("colour type is not 3 (indexed)"));
    }
    if bit_depth != 2 && bit_depth != 4 && bit_depth != 8 {
        return Err(PngError::Unsupported("bit depth is not 2, 4, or 8"));
    }
    if compression != 0 {
        return Err(PngError::Unsupported("compression method is not 0"));
    }
    if filter_method != 0 {
        return Err(PngError::Unsupported("filter method is not 0"));
    }
    if interlace != 0 {
        return Err(PngError::Unsupported("image is interlaced"));
    }

    let mut idat = Vec::new();
    for chunk in &chunks {
        if &chunk.kind == b"IDAT" {
            idat.extend_from_slice(chunk.data);
        }
    }
    if idat.is_empty() {
        return Err(PngError::Truncated);
    }

    let raw = inflate::inflate_zlib(&idat)?;
    let pixels = defilter_and_unpack(&raw, width, height, bit_depth)?;

    Ok(IndexedImage {
        width,
        height,
        bit_depth,
        pixels,
    })
}

/// Undo PNG's per-scanline filtering (RFC 2083 §6) and unpack sub-byte
/// pixel indices (bit depths 2 and 4) into one byte per pixel.
fn defilter_and_unpack(
    raw: &[u8],
    width: u32,
    height: u32,
    bit_depth: u8,
) -> Result<Vec<u8>, PngError> {
    let width = width as usize;
    let height = height as usize;
    // Bytes-per-pixel for filtering purposes: PNG defines the filter's
    // "corresponding byte" step as ceil(bit_depth * channels / 8). Indexed
    // colour has exactly one channel, so at both bit depths this decoder
    // supports (2, 4, and 8) that step is always 1 whole byte.
    let bpp_for_filter: usize = 1;
    let packed_row_bytes = (width * usize::from(bit_depth)).div_ceil(8);
    let stride = packed_row_bytes + 1; // +1 for the filter-type byte prefix

    if raw.len() < stride * height {
        return Err(PngError::PixelDataTooShort);
    }

    let mut prev_row = vec![0u8; packed_row_bytes];
    let mut pixels = Vec::with_capacity(width * height);

    for row in 0..height {
        let row_start = row * stride;
        let filter_type = raw[row_start];
        let packed = &raw[row_start + 1..row_start + 1 + packed_row_bytes];

        let mut cur_row = vec![0u8; packed_row_bytes];
        for i in 0..packed_row_bytes {
            let x = i32::from(packed[i]);
            let a = if i >= bpp_for_filter {
                i32::from(cur_row[i - bpp_for_filter])
            } else {
                0
            };
            let b = i32::from(prev_row[i]);
            let c = if i >= bpp_for_filter {
                i32::from(prev_row[i - bpp_for_filter])
            } else {
                0
            };
            let value = match filter_type {
                0 => x,
                1 => x + a,
                2 => x + b,
                3 => x + i32::midpoint(a, b),
                4 => x + paeth_predictor(a, b, c),
                other => return Err(PngError::BadFilterType(other)),
            };
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let byte = (value & 0xFF) as u8;
            cur_row[i] = byte;
        }

        unpack_row(&cur_row, width, bit_depth, &mut pixels);
        prev_row = cur_row;
    }

    Ok(pixels)
}

/// The PNG Paeth predictor (RFC 2083 §6.6), used by filter type 4.
fn paeth_predictor(a: i32, b: i32, c: i32) -> i32 {
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Unpack one defiltered, byte-packed scanline into one palette-index byte
/// per pixel, appended to `out`.
fn unpack_row(packed: &[u8], width: usize, bit_depth: u8, out: &mut Vec<u8>) {
    match bit_depth {
        8 => out.extend_from_slice(&packed[..width]),
        4 => {
            for x in 0..width {
                let byte = packed[x / 2];
                let index = if x % 2 == 0 { byte >> 4 } else { byte & 0x0F };
                out.push(index);
            }
        }
        2 => {
            for x in 0..width {
                let byte = packed[x / 4];
                let shift = 6 - 2 * (x % 4);
                let index = (byte >> shift) & 0x03;
                out.push(index);
            }
        }
        _ => unreachable!("bit depth already validated to be 2, 4, or 8"),
    }
}

/// Read a PNG's `PLTE` chunk as decoded 8-bit-per-channel colours, in
/// on-disk order (index 0 first).
///
/// This is deliberately separate from [`decode`] (which never reads `PLTE`
/// — see the module docs): only `graphics/text_window/*.png`'s border-frame
/// graphics need their embedded palette, since (unlike tilesets/sprites)
/// they have no sibling `.pal` file of their own.
///
/// # Errors
///
/// [`PngError::BadSignature`] / [`PngError::Truncated`] for a malformed
/// file (same as [`decode`]); [`PngError::MissingOrBadPalette`] if no `PLTE`
/// chunk is present, is empty, or its length is not a multiple of 3 bytes.
pub fn decode_palette(data: &[u8]) -> Result<Vec<Rgb888>, PngError> {
    if data.len() < 8 || data[..8] != SIGNATURE {
        return Err(PngError::BadSignature);
    }
    let chunks = read_chunks(&data[8..])?;

    let plte = chunks
        .iter()
        .find(|c| &c.kind == b"PLTE")
        .ok_or(PngError::MissingOrBadPalette)?;
    if plte.data.is_empty() || plte.data.len() % 3 != 0 {
        return Err(PngError::MissingOrBadPalette);
    }

    Ok(plte
        .data
        .chunks_exact(3)
        .map(|c| Rgb888 {
            r: c[0],
            g: c[1],
            b: c[2],
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{decode, decode_palette, paeth_predictor, PngError};
    use crate::extract::jasc_pal::Rgb888;

    /// A local Adler-32 (test-only): mirrors `inflate`'s private
    /// implementation just enough to hand-build a well-formed zlib stream
    /// for these fixtures, without exposing that helper outside its module.
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

    /// Build one raw PNG chunk (length + type + data + a dummy CRC, unread
    /// by [`decode`]) -- shared by every hand-built test fixture below.
    fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        out.extend_from_slice(&kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0, 0, 0, 0]); // CRC unchecked by this decoder
        out
    }

    /// Assemble a well-formed indexed PNG around an already-built stream of
    /// raw scanlines (each `[filter_type, ...packed bytes]`, the exact bytes
    /// [`decode`] inflates and defilters). The `IDAT` is a single stored
    /// DEFLATE block, zlib-wrapped by hand (mirrors `inflate::tests`, just in
    /// the compress direction) -- no dependency on any real upstream file.
    fn indexed_png_from_raw(bit_depth: u8, width: u32, height: u32, raw: &[u8]) -> Vec<u8> {
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.push(bit_depth);
        ihdr.push(3); // colour type: indexed
        ihdr.push(0); // compression
        ihdr.push(0); // filter method
        ihdr.push(0); // interlace

        let mut zlib_body = vec![0x78, 0x01]; // CMF/FLG: method 8, no dict, valid checksum
        let len = u16::try_from(raw.len()).unwrap();
        zlib_body.push(0b0000_0001); // BFINAL=1, BTYPE=00 stored
        zlib_body.extend_from_slice(&len.to_le_bytes());
        zlib_body.extend_from_slice(&(!len).to_le_bytes());
        zlib_body.extend_from_slice(raw);
        zlib_body.extend_from_slice(&adler32(raw).to_be_bytes());

        let mut png = Vec::new();
        png.extend_from_slice(&super::SIGNATURE);
        png.extend_from_slice(&chunk(*b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(*b"IDAT", &zlib_body));
        png.extend_from_slice(&chunk(*b"IEND", &[]));
        png
    }

    /// Build a minimal, well-formed indexed PNG (filter type 0 on every row)
    /// entirely by hand, to test the decoder without depending on any real
    /// upstream file.
    fn tiny_indexed_png(bit_depth: u8, width: u32, height: u32, packed_rows: &[u8]) -> Vec<u8> {
        let packed_row_bytes = packed_rows.len() / usize::try_from(height).unwrap();
        let mut raw = Vec::new();
        for row in 0..height as usize {
            raw.push(0u8); // filter type: None
            raw.extend_from_slice(
                &packed_rows[row * packed_row_bytes..(row + 1) * packed_row_bytes],
            );
        }
        indexed_png_from_raw(bit_depth, width, height, &raw)
    }

    /// Build an indexed PNG from per-row `(filter_type, packed filtered
    /// bytes)` pairs, taken verbatim as the raw scanlines -- so a test can
    /// exercise filter types other than 0 (Sub/Up/Average/Paeth) and assert
    /// the decoder reconstructs the originals.
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
        // 2x2 image, 8bpp: row0 = [1, 2], row1 = [3, 0].
        let png = tiny_indexed_png(8, 2, 2, &[1, 2, 3, 0]);
        let image = decode(&png).unwrap();
        assert_eq!(image.width, 2);
        assert_eq!(image.height, 2);
        assert_eq!(image.bit_depth, 8);
        assert_eq!(image.pixels, vec![1, 2, 3, 0]);
    }

    #[test]
    fn decodes_4bit_indexed_row() {
        // 2x1 image, 4bpp: one packed byte 0xAB -> pixels [0xA, 0xB].
        let png = tiny_indexed_png(4, 2, 1, &[0xAB]);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![0x0A, 0x0B]);
    }

    #[test]
    fn decodes_2bit_indexed_row() {
        // 4x1 image, 2bpp: one packed byte 0b01_10_11_00 -> pixels [1,2,3,0]
        // (matching `graphics/fonts/latin_*.png`'s bit depth -- see the
        // module docs).
        let png = tiny_indexed_png(2, 4, 1, &[0b0110_1100]);
        let image = decode(&png).unwrap();
        assert_eq!(image.bit_depth, 2);
        assert_eq!(image.pixels, vec![1, 2, 3, 0]);
    }

    #[test]
    fn decodes_2bit_indexed_two_rows() {
        // 8x2 image, 2bpp: exercises a full byte-per-row boundary (8 px = 2
        // packed bytes) rather than the single-byte case above.
        let png = tiny_indexed_png(2, 8, 2, &[0b00_01_10_11, 0b11_10_01_00, 0xFF, 0x00]);
        let image = decode(&png).unwrap();
        assert_eq!(
            image.pixels,
            vec![0, 1, 2, 3, 3, 2, 1, 0, 3, 3, 3, 3, 0, 0, 0, 0]
        );
    }

    #[test]
    fn rejects_bit_depth_1() {
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[1, 3, 0, 0, 0]); // bit depth 1, colour type 3
        let mut png = Vec::new();
        png.extend_from_slice(&super::SIGNATURE);
        png.extend_from_slice(&chunk(*b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(*b"IEND", &[]));
        let err = decode(&png).unwrap_err();
        assert_eq!(err, PngError::Unsupported("bit depth is not 2, 4, or 8"));
    }

    #[test]
    fn rejects_bad_signature() {
        let err = decode(&[0u8; 16]).unwrap_err();
        assert_eq!(err, PngError::BadSignature);
    }

    #[test]
    fn rejects_truecolor() {
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // colour type 2 = truecolor
        let mut png = Vec::new();
        png.extend_from_slice(&super::SIGNATURE);
        png.extend_from_slice(&chunk(*b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(*b"IEND", &[]));
        let err = decode(&png).unwrap_err();
        assert_eq!(err, PngError::Unsupported("colour type is not 3 (indexed)"));
    }

    // The per-row filter reconstructions (RFC 2083 §6). Every real upstream
    // PNG and the fixtures above use filter type 0 (None); these exercise the
    // other four types on hand-built 8-bit rows (bytes-per-pixel = 1), with
    // every expected pixel hand-computed from the reconstruction formula.

    #[test]
    fn filter_type_1_sub_reconstructs() {
        // Sub: recon(x) = filt(x) + recon(x-1), first byte's left = 0.
        // Row [10, 5, 3, 250] -> 10, 15, 18, (250+18)&0xFF = 12.
        let png = filtered_indexed_png(8, 4, 1, &[(1, &[10, 5, 3, 250])]);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![10, 15, 18, 12]);
    }

    #[test]
    fn filter_type_2_up_reconstructs() {
        // Up: recon(x) = filt(x) + recon_above(x); first row's "above" = 0.
        // Row0 (None) -> [1, 2, 3, 4]; Row1 (Up) [10,20,30,40] -> add above.
        let png = filtered_indexed_png(8, 4, 2, &[(0, &[1, 2, 3, 4]), (2, &[10, 20, 30, 40])]);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![1, 2, 3, 4, 11, 22, 33, 44]);
    }

    #[test]
    fn filter_type_3_average_reconstructs() {
        // Average: recon(x) = filt(x) + floor((left + above) / 2).
        // Row0 (None) -> [4, 8, 10, 20]; Row1 (Average) [2, 3, 4, 5]:
        //   i0: left 0, above 4  -> avg 2  -> 2+2  = 4
        //   i1: left 4, above 8  -> avg 6  -> 3+6  = 9
        //   i2: left 9, above 10 -> avg 9  -> 4+9  = 13
        //   i3: left 13, above 20 -> avg 16 -> 5+16 = 21
        let png = filtered_indexed_png(8, 4, 2, &[(0, &[4, 8, 10, 20]), (3, &[2, 3, 4, 5])]);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![4, 8, 10, 20, 4, 9, 13, 21]);
    }

    #[test]
    fn filter_type_4_paeth_reconstructs() {
        // Paeth: recon(x) = filt(x) + Paeth(left, above, above-left).
        // Row0 (None) -> [8, 3, 10, 200]; Row1 (Paeth) [5, 0, 250, 1]:
        //   i0: a0 b8 c0   -> Paeth = 8  -> 5+8       = 13
        //   i1: a13 b3 c8  -> Paeth = 8  -> 0+8       = 8   (above-left wins)
        //   i2: a8 b10 c3  -> Paeth = 10 -> (250+10)&0xFF = 4
        //   i3: a4 b200 c10 -> Paeth = 200 -> 1+200   = 201
        let png = filtered_indexed_png(8, 4, 2, &[(0, &[8, 3, 10, 200]), (4, &[5, 0, 250, 1])]);
        let image = decode(&png).unwrap();
        assert_eq!(image.pixels, vec![8, 3, 10, 200, 13, 8, 4, 201]);
    }

    #[test]
    fn paeth_predictor_selects_each_neighbor() {
        // One case for each branch of the predictor (RFC 2083 §6.6).
        assert_eq!(paeth_predictor(5, 20, 18), 5, "left (a) wins");
        assert_eq!(paeth_predictor(20, 5, 18), 5, "above (b) wins");
        assert_eq!(paeth_predictor(13, 3, 8), 8, "above-left (c) wins");
    }

    /// Build a minimal indexed PNG (bit depth 4, one pixel) with a `PLTE`
    /// chunk of `colors` spliced in right after `IHDR` (PNG's required
    /// chunk order) -- for [`decode_palette`] tests, which never look past
    /// `PLTE`. Reuses [`tiny_indexed_png`] for the rest of the file (a
    /// well-formed `IHDR`/`IDAT`/`IEND` with no `PLTE` of its own) rather
    /// than hand-building another zlib stream.
    fn indexed_png_with_palette(colors: &[(u8, u8, u8)]) -> Vec<u8> {
        let base = tiny_indexed_png(4, 1, 1, &[0x00]);
        // `SIGNATURE` (8 bytes) + one whole `IHDR` chunk
        // (4 length + 4 type + 13 data + 4 CRC = 25 bytes) = 33.
        let ihdr_end = 8 + 25;

        let mut plte = Vec::new();
        for &(r, g, b) in colors {
            plte.extend_from_slice(&[r, g, b]);
        }

        let mut png = base[..ihdr_end].to_vec();
        png.extend_from_slice(&chunk(*b"PLTE", &plte));
        png.extend_from_slice(&base[ihdr_end..]);
        png
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
    fn decode_palette_rejects_missing_plte() {
        // A well-formed indexed PNG with no PLTE chunk at all.
        let png = tiny_indexed_png(8, 2, 1, &[1, 2]);
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
    fn decode_palette_rejects_a_plte_length_not_a_multiple_of_three() {
        // A `PLTE` chunk with a dangling partial RGB triple (2 bytes) -- a
        // corrupt-file case no real upstream PNG produces, but the parser
        // must fail closed rather than panic on an uneven `chunks_exact(3)`.
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[4, 3, 0, 0, 0]); // bit depth 4, colour type 3
        let mut png = Vec::new();
        png.extend_from_slice(&super::SIGNATURE);
        png.extend_from_slice(&chunk(*b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(*b"PLTE", &[1, 2]));
        png.extend_from_slice(&chunk(*b"IEND", &[]));
        let err = decode_palette(&png).unwrap_err();
        assert_eq!(err, PngError::MissingOrBadPalette);
    }
}
