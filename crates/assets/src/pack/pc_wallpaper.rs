//! Typed PC box wallpapers 4 to 15 (upstream `sWallpapers` in
//! `src/data/wallpapers.h`, loaded by `LoadWallpaperGfx` and drawn by
//! `DrawWallpaper` in `src/pokemon_storage_system.c`).
//!
//! A save file keeps any wallpaper byte, so [`AssetPack::pc_wallpaper`] takes
//! the raw byte and resolves only the twelve identities of
//! [`PcWallpaperId::ALL`]; the four default styles, Walda's wallpaper and every
//! other byte fail with [`PackError::UnsupportedPcWallpaper`] before the pack is
//! read. The pack ids stay in this module. No producer emits these entries yet,
//! so a production pack fails with [`PackError::UnknownAsset`]; no art is ever
//! substituted. Every lookup re-validates shapes, since a hand-built or corrupt
//! pack can bypass extraction.
//!
//! # Tilemap words
//!
//! The tilemap holds the 20x18 two-byte little-endian entries upstream stores,
//! unchanged. Each word is a tile index (bits 0-9), a horizontal flip (bit 10),
//! a vertical flip (bit 11) and a palette bank (bits 12-15). `DrawWallpaper`
//! adds the tile offset and a palette offset of `3 + 2 * slot` to each word, and
//! `LoadWallpaperGfx` loads the frame palette at background bank `4 + 2 * slot`
//! and the background palette at `5 + 2 * slot`. A stored bank is therefore
//! one-based over the pair: 1 selects the frame palette and 2 the background
//! palette. Bank 0 selects a bank the wallpaper does not supply, so the only
//! accepted bank-0 word is the blank `0x0000`, and only while tile 0 is fully
//! transparent.

use super::{AssetPack, ImageRef, PackError, PaletteRef};

const TILE_SIDE: u32 = 8;
const TILE_PIXELS: usize = 64;
/// Tiles one wallpaper slot holds: `LoadWallpaperGfx` alternates slots 256 tiles apart.
const MAX_TILES: u32 = 256;
const BANK_COLORS: u16 = 16;

/// The 20x18 wallpaper window, in two-byte tilemap entries.
const TILEMAP_ENTRIES: usize = 20 * 18;
const TILEMAP_BYTES: usize = TILEMAP_ENTRIES * 2;

const TILE_INDEX_MASK: u16 = 0x03ff;
const H_FLIP_MASK: u16 = 0x0400;
const V_FLIP_MASK: u16 = 0x0800;
const BANK_SHIFT: u32 = 12;
/// Stored bank of the frame palette (the first of the pair).
const FRAME_BANK: u8 = 1;
/// Stored bank of the background palette (the second of the pair).
const BACKGROUND_BANK: u8 = 2;

/// A non-default standard wallpaper, in upstream `sWallpapers` order.
///
/// The discriminant is the wallpaper byte a save file stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PcWallpaperId {
    /// `WALLPAPER_CRAG`.
    Crag = 4,
    /// `WALLPAPER_VOLCANO`.
    Volcano = 5,
    /// `WALLPAPER_SNOW`.
    Snow = 6,
    /// `WALLPAPER_CAVE`.
    Cave = 7,
    /// `WALLPAPER_BEACH`.
    Beach = 8,
    /// `WALLPAPER_SEAFLOOR`.
    Seafloor = 9,
    /// `WALLPAPER_RIVER`.
    River = 10,
    /// `WALLPAPER_SKY`.
    Sky = 11,
    /// `WALLPAPER_POLKADOT`.
    Polkadot = 12,
    /// `WALLPAPER_POKECENTER`.
    Pokecenter = 13,
    /// `WALLPAPER_MACHINE`.
    Machine = 14,
    /// `WALLPAPER_PLAIN`.
    Plain = 15,
}

impl PcWallpaperId {
    /// Every supported wallpaper, in upstream order (bytes 4 to 15).
    pub const ALL: [Self; 12] = [
        Self::Crag,
        Self::Volcano,
        Self::Snow,
        Self::Cave,
        Self::Beach,
        Self::Seafloor,
        Self::River,
        Self::Sky,
        Self::Polkadot,
        Self::Pokecenter,
        Self::Machine,
        Self::Plain,
    ];

    /// The wallpaper byte a save file stores for this style.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// The lowercase upstream directory name this style's pack ids use.
    #[must_use]
    pub const fn stem(self) -> &'static str {
        match self {
            Self::Crag => "crag",
            Self::Volcano => "volcano",
            Self::Snow => "snow",
            Self::Cave => "cave",
            Self::Beach => "beach",
            Self::Seafloor => "seafloor",
            Self::River => "river",
            Self::Sky => "sky",
            Self::Polkadot => "polkadot",
            Self::Pokecenter => "pokecenter",
            Self::Machine => "machine",
            Self::Plain => "plain",
        }
    }

    /// Pack id of the tile sheet (an image: 8 pixels wide, 8 per tile tall).
    #[must_use]
    pub fn tiles_id(self) -> String {
        format!("pc-wallpaper/{}/tiles", self.stem())
    }

    /// Pack id of the 20x18 tilemap (a raw blob).
    #[must_use]
    pub fn tilemap_id(self) -> String {
        format!("pc-wallpaper/{}/tilemap", self.stem())
    }

    /// Pack id of the frame palette (a 16-colour palette).
    #[must_use]
    pub fn frame_palette_id(self) -> String {
        format!("pc-wallpaper/{}/palette/frame", self.stem())
    }

    /// Pack id of the background palette (a 16-colour palette).
    #[must_use]
    pub fn background_palette_id(self) -> String {
        format!("pc-wallpaper/{}/palette/background", self.stem())
    }
}

impl TryFrom<u8> for PcWallpaperId {
    type Error = PackError;

    fn try_from(byte: u8) -> Result<Self, PackError> {
        Self::ALL
            .into_iter()
            .find(|id| id.index() == byte)
            .ok_or(PackError::UnsupportedPcWallpaper(byte))
    }
}

/// One decoded tilemap word of a wallpaper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcWallpaperTile {
    /// Index into the tile sheet.
    pub tile_index: u16,
    /// Whether the tile is mirrored horizontally.
    pub h_flip: bool,
    /// Whether the tile is mirrored vertically.
    pub v_flip: bool,
    /// The stored palette bank: 1 selects the frame palette and 2 the
    /// background palette; 0 only appears on the blank word.
    pub palette_bank: u8,
}

impl PcWallpaperTile {
    fn decode(word: u16) -> Self {
        Self {
            tile_index: word & TILE_INDEX_MASK,
            h_flip: word & H_FLIP_MASK != 0,
            v_flip: word & V_FLIP_MASK != 0,
            // The shift leaves four bits, so the narrowing is lossless.
            palette_bank: (word >> BANK_SHIFT) as u8,
        }
    }

    /// Whether this is the blank word (tile 0, no flips, bank 0).
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.tile_index == 0 && !self.h_flip && !self.v_flip && self.palette_bank == 0
    }
}

/// One wallpaper's graphics, borrowing the pack for `'a`.
///
/// [`AssetPack::pc_wallpaper`] returns it with every entry validated.
#[derive(Debug, Clone, Copy)]
pub struct PcWallpaperHandle<'a> {
    /// The wallpaper identity.
    pub id: PcWallpaperId,
    /// The tile sheet: frame tiles then background tiles as one strip, 8 pixels
    /// wide and 8 per tile tall (1 to 256 tiles). Tile `n` is pixel bytes
    /// `64 * n..64 * (n + 1)`; every index selects within one 16-colour bank.
    pub tiles: ImageRef<'a>,
    /// The 20x18 tilemap as upstream stores it: 360 little-endian words, 720
    /// bytes; [`entries`](Self::entries) decodes them.
    pub tilemap: &'a [u8],
    /// The palette pair in load order: `[frame, background]`.
    pub palettes: [PaletteRef<'a>; 2],
}

impl PcWallpaperHandle<'_> {
    /// The tilemap's 360 words in row-major order (20 columns, 18 rows).
    pub fn entries(&self) -> impl Iterator<Item = PcWallpaperTile> + '_ {
        words(self.tilemap).map(PcWallpaperTile::decode)
    }
}

fn words(tilemap: &[u8]) -> impl Iterator<Item = u16> + '_ {
    tilemap
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
}

fn malformed(id: String, detail: String) -> PackError {
    PackError::MalformedPcWallpaperAsset { id, detail }
}

impl AssetPack {
    /// One non-default standard PC wallpaper, validated.
    ///
    /// `wallpaper` is the byte a save file stores for a box; it is checked
    /// against [`PcWallpaperId::ALL`] before the pack is read.
    ///
    /// # Errors
    ///
    /// [`PackError::UnsupportedPcWallpaper`] for a byte outside 4 to 15;
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedPcWallpaperAsset`] when the tile
    /// sheet is not an 8-pixel-wide 4-bit strip of 1 to 256 tiles, a palette
    /// does not hold 16 colours, the tilemap is not 720 bytes, a tilemap word
    /// names a tile beyond the sheet or a bank outside the pair, or the blank
    /// word appears over a non-transparent tile 0.
    pub fn pc_wallpaper(&self, wallpaper: u8) -> Result<PcWallpaperHandle<'_>, PackError> {
        let id = PcWallpaperId::try_from(wallpaper)?;
        let tiles_id = id.tiles_id();
        let tiles = self.image(&tiles_id)?;
        let tile_count = tile_count(&tiles, &tiles_id)?;
        let tiles = self.bank_image(&tiles_id, (TILE_SIDE, tiles.height), malformed)?;
        let frame_id = id.frame_palette_id();
        let background_id = id.background_palette_id();
        let palettes = [
            self.sized_palette(&frame_id, BANK_COLORS, malformed)?,
            self.sized_palette(&background_id, BANK_COLORS, malformed)?,
        ];
        let tilemap_id = id.tilemap_id();
        let tilemap = self.raw(&tilemap_id)?;
        check_tilemap(tilemap, &tilemap_id, &tiles, tile_count)?;
        Ok(PcWallpaperHandle {
            id,
            tiles,
            tilemap,
            palettes,
        })
    }
}

/// Tiles in the sheet, after bounding its dimensions so no later arithmetic
/// trusts an unchecked size.
fn tile_count(tiles: &ImageRef<'_>, id: &str) -> Result<u32, PackError> {
    let max_height = MAX_TILES * TILE_SIDE;
    if tiles.width != TILE_SIDE
        || tiles.height < TILE_SIDE
        || tiles.height > max_height
        || !tiles.height.is_multiple_of(TILE_SIDE)
    {
        return Err(malformed(
            id.to_owned(),
            format!(
                "is {}x{}: expected {TILE_SIDE} pixels wide and a multiple of {TILE_SIDE} up to {max_height} tall",
                tiles.width, tiles.height
            ),
        ));
    }
    Ok(tiles.height / TILE_SIDE)
}

fn check_tilemap(
    tilemap: &[u8],
    id: &str,
    tiles: &ImageRef<'_>,
    tile_count: u32,
) -> Result<(), PackError> {
    if tilemap.len() != TILEMAP_BYTES {
        return Err(malformed(
            id.to_owned(),
            format!("is {} bytes: expected {TILEMAP_BYTES}", tilemap.len()),
        ));
    }
    let mut blank_seen = false;
    for (cell, word) in words(tilemap).enumerate() {
        let tile = PcWallpaperTile::decode(word);
        if u32::from(tile.tile_index) >= tile_count {
            return Err(malformed(
                id.to_owned(),
                format!(
                    "cell {cell} names tile {}: the sheet holds {tile_count}",
                    tile.tile_index
                ),
            ));
        }
        if tile.is_blank() {
            blank_seen = true;
        } else if tile.palette_bank != FRAME_BANK && tile.palette_bank != BACKGROUND_BANK {
            return Err(malformed(
                id.to_owned(),
                format!(
                    "cell {cell} selects palette bank {}: the pair supplies banks {FRAME_BANK} (frame) and {BACKGROUND_BANK} (background)",
                    tile.palette_bank
                ),
            ));
        }
    }
    let tile_zero_clear = tiles
        .pixels
        .get(..TILE_PIXELS)
        .is_some_and(|tile| tile.iter().all(|&pixel| pixel == 0));
    if blank_seen && !tile_zero_clear {
        return Err(malformed(
            id.to_owned(),
            "has blank cells over a tile 0 that is not transparent".to_owned(),
        ));
    }
    Ok(())
}
