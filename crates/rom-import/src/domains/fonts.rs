//! The Latin glyph sheets: `font/*/glyphs`.
//!
//! A glyph sheet is the one image the ROM does not store as a tile sheet.
//! `gbagfx` writes it in its own `.latfont` layout
//! (`pokeemerald/tools/gbagfx/font.c`, `ConvertToLatinFont`), and the ROM
//! holds that verbatim: a 16x32 grid of 16x16 glyphs, walked glyph by
//! glyph, each glyph as four 8x8 tiles in reading order, each tile row as
//! its two 2bpp bytes with the right-hand four pixels *first*. This module
//! is the inverse of that walk, reimplemented rather than copied
//! `(no-verbatim)`. `cargo xtask gen-rom-profile` packs the checkout raster
//! the same way to find the sheet, so the two directions are checked
//! against each other by the equivalence harness.
//!
//! The layout is fixed: 256x512 pixels, 2bpp, 32 KiB. A [`FontRoot`] that
//! claims any other shape is a profile the decoder cannot honour, and is
//! refused as [`ImportError::FontShape`] before a byte is read.

use pack_format::{image_entry, PackEntry, PackWriter};

use super::len_usize;
use crate::error::ImportError;
use crate::reader::RomReader;
use crate::rom::Rom;
use crate::roots::{FontRoot, Roots};

/// A glyph cell's side, in pixels.
const GLYPH_PX: usize = 16;
/// Glyph columns across a sheet.
const COLUMNS: usize = 16;
/// Glyph rows down a sheet.
const ROWS: usize = 32;
/// The sheet's width in pixels.
const SHEET_WIDTH: usize = COLUMNS * GLYPH_PX;
/// The sheet's height in pixels.
const SHEET_HEIGHT: usize = ROWS * GLYPH_PX;
/// Bits per pixel. Four colours, `gbagfx`'s `SetFontPalette`.
const BIT_DEPTH: u8 = 2;
/// Pixels per 2bpp byte.
const PIXELS_PER_BYTE: usize = 4;
/// One 8x8 tile row is two 2bpp bytes.
const TILE_ROW_BYTES: usize = 2;
/// Bytes per stored sheet: 512 glyphs of 64 bytes.
const SHEET_BYTES: usize = ROWS * COLUMNS * 4 * 8 * TILE_ROW_BYTES;

/// Write every Latin glyph sheet.
///
/// # Errors
///
/// [`ImportError::FontShape`] if a root is not the one shape `.latfont`
/// has; [`ImportError::Truncated`] if its bytes are not inside the ROM.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    for root in roots.fonts {
        writer.push(font(&reader, root)?);
    }
    Ok(())
}

/// Read one glyph sheet and unpack its `.latfont` bytes into a raster.
pub(crate) fn font(reader: &RomReader<'_>, root: &FontRoot) -> Result<PackEntry, ImportError> {
    if len_usize(root.width) != SHEET_WIDTH
        || len_usize(root.height) != SHEET_HEIGHT
        || root.bit_depth != BIT_DEPTH
        || len_usize(root.len) != SHEET_BYTES
    {
        return Err(ImportError::FontShape {
            id: root.id,
            width: root.width,
            height: root.height,
            bit_depth: root.bit_depth,
            len: root.len,
        });
    }
    let packed = reader.slice(root.addr, SHEET_BYTES)?;
    let pixels = unpack(packed);
    image_entry(
        root.id.to_owned(),
        root.width,
        root.height,
        root.bit_depth,
        pixels,
    )
    .map_err(|source| ImportError::EntryShape {
        id: root.id,
        source,
    })
}

/// Unpack a `.latfont` sheet into a row-major, one-byte-per-pixel raster.
///
/// `packed` must be exactly [`SHEET_BYTES`] long; [`font`] checks that.
fn unpack(packed: &[u8]) -> Vec<u8> {
    let mut pixels = vec![0u8; SHEET_WIDTH * SHEET_HEIGHT];
    let mut bytes = packed.chunks_exact(TILE_ROW_BYTES);
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            for glyph_tile in 0..4 {
                let x0 = column * GLYPH_PX + (glyph_tile & 1) * 8;
                let y0 = row * GLYPH_PX + (glyph_tile >> 1) * 8;
                for line in 0..8 {
                    let pair = bytes.next().expect("the length was checked");
                    // The tool wrote the right half first, then the left.
                    let halves = [(pair[1], x0), (pair[0], x0 + PIXELS_PER_BYTE)];
                    for (byte, x) in halves {
                        let base = (y0 + line) * SHEET_WIDTH + x;
                        for (n, pixel) in
                            pixels[base..base + PIXELS_PER_BYTE].iter_mut().enumerate()
                        {
                            // Most significant pair first.
                            *pixel = (byte >> (6 - 2 * n)) & 0x03;
                        }
                    }
                }
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests;
