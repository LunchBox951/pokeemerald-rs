//! The non-default standard PC wallpapers: `pc-wallpaper/*`.
//!
//! Each `sWallpapers` row (`src/data/wallpapers.h`) is `{tiles, tilemap,
//! palettes}`; the tiles and tilemap are LZ77 streams and `palettes` names a
//! raw frame palette with the background palette in the 32 bytes after it. The
//! row must name the addresses the profile records, so a profile that picked a
//! stray copy of the art is [`ImportError::StructMismatch`].
//!
//! Every tilemap word is then checked as `AssetPack::pc_wallpaper` checks it,
//! so an import never publishes an entry that lookup refuses.

use assets::PcWallpaperTile;
use pack_format::{PackEntry, PackWriter};

use super::{check_pointer, image, palette};
use crate::error::ImportError;
use crate::reader::RomReader;
use crate::rom::Rom;
use crate::roots::{Encoding, PcWallpaperRoot, PcWallpaperRoots, Roots};

/// `struct Wallpaper`: three pointers.
const RECORD_BYTES: usize = 12;
/// `WALLPAPER_COUNT - 1`: the rows of `sWallpapers`.
const TABLE_ROWS: usize = 16;
const TILEMAP_FIELD: usize = 4;
const PALETTES_FIELD: usize = 8;
/// Both palettes: two banks of 16 colours.
const BANK_BYTES: u32 = 32;
const BANK_COLORS: u16 = 16;
const TILE_SIDE: u32 = 8;
const TILE_PIXELS: usize = 64;
/// `LoadWallpaperGfx` alternates slots 256 tiles apart.
const MAX_TILES: u32 = 256;
const TILEMAP_BYTES: u32 = 20 * 18 * 2;
/// Stored banks of the frame and background palettes.
const PALETTE_BANKS: [u8; 2] = [1, 2];

/// Write every non-default wallpaper's sheet, tilemap and palettes.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no wallpaper roots writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a row or a decoded wallpaper disagrees
/// with the profile; [`ImportError::IndexOutOfRange`] for a record past the
/// end of the table; any [`ImportError`] an image, palette or blob read
/// raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let entries = read_entries(&rom.reader(), &roots.pc_wallpaper)?;
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn read_entries(
    reader: &RomReader<'_>,
    family: &PcWallpaperRoots,
) -> Result<Vec<PackEntry>, ImportError> {
    let mut entries = Vec::with_capacity(family.wallpapers.len() * 4);
    for paper in family.wallpapers {
        check_roots(paper)?;
        corroborate(reader, family, paper)?;
        let tiles = image(reader, &paper.tiles)?;
        let tilemap = super::blob(reader, &paper.tilemap)?;
        check_tilemap(paper.tilemap.id, &tilemap.payload, &tiles)?;
        entries.push(tiles);
        entries.push(tilemap);
        entries.push(palette(reader, &paper.frame_palette)?);
        entries.push(palette(reader, &paper.background_palette)?);
    }
    Ok(entries)
}

/// Refuse a record whose declared shape `AssetPack::pc_wallpaper` would not
/// accept, before any of its bytes are decoded.
fn check_roots(paper: &PcWallpaperRoot) -> Result<(), ImportError> {
    let tiles = &paper.tiles;
    let sheet_ok = tiles.encoding == Encoding::Lz77
        && tiles.width == TILE_SIDE
        && tiles.rom_bit_depth == 4
        && tiles.pack_bit_depth == 4
        && tiles.height >= TILE_SIDE
        && tiles.height.is_multiple_of(TILE_SIDE)
        && tiles.height <= MAX_TILES * TILE_SIDE
        && (1..=tiles.height / TILE_SIDE).contains(&tiles.tile_count);
    if !sheet_ok {
        return Err(mismatch(tiles.id, "sheet shape"));
    }
    if paper.tilemap.encoding != Encoding::Lz77 || paper.tilemap.len != TILEMAP_BYTES {
        return Err(mismatch(paper.tilemap.id, "tilemap shape"));
    }
    for bank in [&paper.frame_palette, &paper.background_palette] {
        if bank.encoding != Encoding::Raw || bank.color_count != BANK_COLORS {
            return Err(mismatch(bank.id, "palette shape"));
        }
    }
    Ok(())
}

/// Check the record's row in `sWallpapers` against the recorded art.
fn corroborate(
    reader: &RomReader<'_>,
    family: &PcWallpaperRoots,
    paper: &PcWallpaperRoot,
) -> Result<(), ImportError> {
    let index = usize::from(paper.wallpaper);
    reader.table_entry("sWallpapers", family.table, index, TABLE_ROWS, RECORD_BYTES)?;
    let row = family.table.offset() + index * RECORD_BYTES;
    let root = paper.tiles.id;
    check_pointer(reader, row, 0, paper.tiles.addr, root, "Wallpaper.tiles")?;
    check_pointer(
        reader,
        row,
        TILEMAP_FIELD,
        paper.tilemap.addr,
        root,
        "Wallpaper.tilemap",
    )?;
    check_pointer(
        reader,
        row,
        PALETTES_FIELD,
        paper.frame_palette.addr,
        root,
        "Wallpaper.palettes",
    )?;
    if paper.frame_palette.addr.raw().checked_add(BANK_BYTES)
        != Some(paper.background_palette.addr.raw())
    {
        return Err(mismatch(
            paper.background_palette.id,
            "Wallpaper.palettes[1]",
        ));
    }
    Ok(())
}

/// Every word must be a tile of the sheet, on a bank the pair supplies; the
/// blank word is allowed only over a transparent tile 0.
fn check_tilemap(id: &'static str, tilemap: &[u8], tiles: &PackEntry) -> Result<(), ImportError> {
    let tile_count = tiles.payload.len() / TILE_PIXELS;
    let mut blank_seen = false;
    for pair in tilemap.chunks_exact(2) {
        let tile = PcWallpaperTile::decode(u16::from_le_bytes([pair[0], pair[1]]));
        if usize::from(tile.tile_index) >= tile_count {
            return Err(mismatch(id, "tilemap tile index"));
        }
        if tile.is_blank() {
            blank_seen = true;
        } else if !PALETTE_BANKS.contains(&tile.palette_bank) {
            return Err(mismatch(id, "tilemap palette bank"));
        }
    }
    let tile_zero_clear = tiles
        .payload
        .get(..TILE_PIXELS)
        .is_some_and(|tile| tile.iter().all(|&pixel| pixel == 0));
    if blank_seen && !tile_zero_clear {
        return Err(mismatch(id, "tilemap blank over opaque tile 0"));
    }
    Ok(())
}

const fn mismatch(root: &'static str, field: &'static str) -> ImportError {
    ImportError::StructMismatch { root, field }
}

#[cfg(test)]
mod tests;
