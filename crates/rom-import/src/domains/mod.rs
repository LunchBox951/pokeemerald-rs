//! Domain readers: one asset domain's ROM bytes turned into pack entries.
//!
//! One module per domain, each exposing a `write` that walks its slice of
//! [`Roots`] and queues entries on a [`PackWriter`]. [`DOMAINS`] lists them
//! in the order [`crate::import`] runs them. Adding a domain is adding a
//! module and one line to that list; nothing else in the crate changes.
//!
//! Three shapes cover almost every root, because the ROM stores art,
//! colours, and opaque blobs the same way wherever they live:
//! [`write_image`], [`write_palette`], and [`write_blob`]. A domain that
//! needs something else (a `struct Tileset` to corroborate, a voicegroup to
//! walk) does that itself and still lands here to emit the bytes.
//!
//! Every read goes through [`RomReader`](crate::RomReader), so a wrong
//! address is a typed [`ImportError`] naming the offset rather than a panic
//! or a silently wrong asset. Nothing here scans; every address comes from
//! the profile table `(behavioral-fidelity)`.

pub(crate) mod interface;
pub(crate) mod title;

use pack_format::{EntryKind, PackWriter};

use crate::error::ImportError;
use crate::lz77::decompress_at;
use crate::rom::Rom;
use crate::roots::{BlobRoot, Encoding, ImageRoot, PaletteRoot, Roots};

/// One domain reader.
type Domain = fn(&Rom, &Roots, &mut PackWriter) -> Result<(), ImportError>;

/// Every domain reader, in the order [`crate::import`] runs them.
///
/// Order is cosmetic: [`PackWriter::finish`] sorts by id, so the pack's
/// bytes do not depend on it.
pub(crate) const DOMAINS: &[Domain] = &[title::write, interface::write];

/// Read one image root's tiles and queue its pack entry.
///
/// # Errors
///
/// [`ImportError::Truncated`] if the tile data is not inside the ROM;
/// [`ImportError::Lz77`] if a compressed root will not decode to exactly
/// the length its shape implies; [`ImportError::EntryShape`] if the tiles
/// do not fit the raster the root declares.
pub(crate) fn write_image(
    rom: &Rom,
    root: &ImageRoot,
    writer: &mut PackWriter,
) -> Result<(), ImportError> {
    let tiles = read_bytes(
        rom,
        root.addr.offset(),
        root.encoding,
        len_usize(root.tile_data_len()),
    )?;
    let mut entry = pack_format::image_entry_from_tiles(
        root.id.to_owned(),
        &tiles,
        root.rom_bit_depth,
        root.width,
        root.height,
        Some(root.metatile()),
    )
    .map_err(|source| ImportError::EntryShape {
        id: root.id,
        source,
    })?;
    // The pack records the *source PNG's* depth, not how the ROM packs the
    // tiles. `title/image/press_start` is the case that forces the split:
    // 4bpp in ROM, 8 in the pack. The payload is one byte per pixel either
    // way, so only the metadata differs.
    if let EntryKind::Image { bit_depth, .. } = &mut entry.kind {
        *bit_depth = root.pack_bit_depth;
    }
    writer.push(entry);
    Ok(())
}

/// Read one palette root's colours and queue its pack entry.
///
/// The ROM stores BGR555 little-endian, which is exactly what the pack's
/// payload holds, so the colours pass through unconverted.
///
/// # Errors
///
/// [`ImportError::Truncated`] if the colours are not inside the ROM;
/// [`ImportError::EntryShape`] if the count is unrepresentable, which a
/// `u16` count cannot be.
pub(crate) fn write_palette(
    rom: &Rom,
    root: &PaletteRoot,
    writer: &mut PackWriter,
) -> Result<(), ImportError> {
    let bytes = rom
        .reader()
        .slice(root.addr, usize::from(root.color_count) * 2)?;
    let colors: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let entry = pack_format::palette_entry(root.id.to_owned(), &colors).map_err(|source| {
        ImportError::EntryShape {
            id: root.id,
            source,
        }
    })?;
    writer.push(entry);
    Ok(())
}

/// Read one blob root's bytes and queue its pack entry.
///
/// # Errors
///
/// [`ImportError::Truncated`] if the bytes are not inside the ROM;
/// [`ImportError::Lz77`] if a compressed root will not decode to exactly
/// `len` bytes.
pub(crate) fn write_blob(
    rom: &Rom,
    root: &BlobRoot,
    writer: &mut PackWriter,
) -> Result<(), ImportError> {
    let payload = read_bytes(rom, root.addr.offset(), root.encoding, len_usize(root.len))?;
    writer.push(pack_format::raw_entry(root.id.to_owned(), payload));
    Ok(())
}

/// Read `len` bytes at ROM offset `at`, decompressing when `encoding` says
/// to.
///
/// A compressed root is decoded with `len` as the expected size rather than
/// trusting the stream's own header, so a stream that decodes to the wrong
/// length is a typed failure instead of a short or over-long asset.
fn read_bytes(
    rom: &Rom,
    at: usize,
    encoding: Encoding,
    len: usize,
) -> Result<Vec<u8>, ImportError> {
    let reader = rom.reader();
    match encoding {
        Encoding::Raw => Ok(reader.slice_at(at, len)?.to_vec()),
        Encoding::Lz77 => decompress_at(&reader, at, Some(len)),
    }
}

/// Widen a root's 32-bit length to a `usize`.
///
/// The saturating fallback is unreachable on any target with a 32-bit or
/// wider pointer; on a narrower one it turns into a length failure at the
/// next read rather than a wrong length.
fn len_usize(len: u32) -> usize {
    usize::try_from(len).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
