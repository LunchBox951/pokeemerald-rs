//! Domain readers: one asset domain's ROM bytes turned into pack entries.
//!
//! One module per domain, each exposing a `write` that walks its slice of
//! [`Roots`] and queues entries on a [`PackWriter`]. [`DOMAINS`] lists them
//! in the order [`crate::import`] runs them. Adding a domain is adding a
//! module and one line to that list; nothing else in the crate changes.
//!
//! Three shapes cover almost every root, because the ROM stores art,
//! colours, and opaque blobs the same way wherever they live: [`image`],
//! [`palette`], and [`blob`]. Each hands back a [`PackEntry`] rather than
//! pushing one, so a domain that has to restamp what it read (a tile sheet
//! whose pack depth is not its ROM depth) does that before it queues, and a
//! domain that needs something else first (a `struct Tileset` to
//! corroborate, a voicegroup to walk) does that itself and still lands here
//! to shape the bytes.
//!
//! A reader never *finds* an address. It reads the ones its profile already
//! recorded and, where there is a ROM struct behind them, corroborates that
//! struct field by field with [`check_pointer`], so a wrong profile is a
//! typed [`ImportError`] rather than a plausible-looking asset. Every read
//! goes through [`RomReader`], which bounds-checks it and names the offset
//! `(behavioral-fidelity)`.

pub(crate) mod interface;
pub(crate) mod layouts;
pub(crate) mod tilesets;
pub(crate) mod title;

use pack_format::{raw_entry, EntryKind, PackEntry, PackWriter};

use crate::error::ImportError;
use crate::lz77::decompress_at;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{BlobRoot, Encoding, ImageRoot, PaletteRoot, Roots};

/// One domain reader.
type Domain = fn(&Rom, &Roots, &mut PackWriter) -> Result<(), ImportError>;

/// Every domain reader, in the order [`crate::import`] runs them.
///
/// Order is cosmetic: [`PackWriter::finish`] sorts by id, so the pack's
/// bytes do not depend on it.
pub(crate) const DOMAINS: &[Domain] = &[
    title::write,
    interface::write,
    tilesets::write,
    layouts::write,
];

/// Read one image root's tiles and unpack them into a pack entry.
///
/// # Errors
///
/// [`ImportError::Truncated`] if the tile data is not inside the ROM;
/// [`ImportError::Lz77`] if a compressed root will not decode to exactly
/// the length its shape implies; [`ImportError::EntryShape`] if the tiles
/// do not fit the raster the root declares.
pub(crate) fn image(reader: &RomReader<'_>, root: &ImageRoot) -> Result<PackEntry, ImportError> {
    let tiles = read_bytes(
        reader,
        root.addr,
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
    Ok(entry)
}

/// Read one palette root's colours into a pack entry.
///
/// The ROM stores BGR555 little-endian, which is exactly what the pack's
/// payload holds, so the colours pass through unconverted.
///
/// # Errors
///
/// [`ImportError::Truncated`] if the colours are not inside the ROM;
/// [`ImportError::EntryShape`] if the count is unrepresentable, which a
/// `u16` count cannot be.
pub(crate) fn palette(
    reader: &RomReader<'_>,
    root: &PaletteRoot,
) -> Result<PackEntry, ImportError> {
    let bytes = reader.slice(root.addr, usize::from(root.color_count) * 2)?;
    let colors: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    pack_format::palette_entry(root.id.to_owned(), &colors).map_err(|source| {
        ImportError::EntryShape {
            id: root.id,
            source,
        }
    })
}

/// Read one blob root's bytes into a raw pack entry.
///
/// # Errors
///
/// [`ImportError::Truncated`] if the bytes are not inside the ROM;
/// [`ImportError::Lz77`] if a compressed root will not decode to exactly
/// `len` bytes.
pub(crate) fn blob(reader: &RomReader<'_>, root: &BlobRoot) -> Result<PackEntry, ImportError> {
    let payload = read_bytes(reader, root.addr, root.encoding, len_usize(root.len))?;
    Ok(raw_entry(root.id.to_owned(), payload))
}

/// Check one pointer field of a struct against the address the profile
/// records for it.
///
/// `base` is the struct's ROM offset and `offset` the field's place inside
/// it. A saturated sum is past any ROM, so an offset that would wrap fails
/// the read rather than addressing the wrong field.
///
/// # Errors
///
/// [`ImportError::Truncated`] or [`ImportError::PointerOutOfRange`] if the
/// field does not read as a cartridge pointer;
/// [`ImportError::StructMismatch`] if it reads as a different one.
pub(crate) fn check_pointer(
    reader: &RomReader<'_>,
    base: usize,
    offset: usize,
    expected: GbaPtr,
    root: &'static str,
    field: &'static str,
) -> Result<(), ImportError> {
    if reader.ptr(base.saturating_add(offset))? != expected {
        return Err(ImportError::StructMismatch { root, field });
    }
    Ok(())
}

/// Read `len` bytes at `addr`, decompressing when `encoding` says to.
///
/// A compressed root is decoded with `len` as the expected size rather than
/// trusting the stream's own header, so a stream that decodes to the wrong
/// length is a typed failure instead of a short or over-long asset.
fn read_bytes(
    reader: &RomReader<'_>,
    addr: GbaPtr,
    encoding: Encoding,
    len: usize,
) -> Result<Vec<u8>, ImportError> {
    match encoding {
        Encoding::Raw => Ok(reader.slice(addr, len)?.to_vec()),
        Encoding::Lz77 => decompress_at(reader, addr.offset(), Some(len)),
    }
}

/// Widen a root's 32-bit length to a `usize`.
///
/// The saturating fallback is unreachable on any target with a 32-bit or
/// wider pointer; on a narrower one it turns into a length failure at the
/// next read rather than a wrong length.
pub(crate) fn len_usize(len: u32) -> usize {
    usize::try_from(len).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
