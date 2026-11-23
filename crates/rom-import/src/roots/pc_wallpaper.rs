//! The non-default standard PC wallpapers' roots (`sWallpapers` 4 to 15).

use crate::reader::GbaPtr;
use crate::roots::{BlobRoot, ImageRoot, PaletteRoot};

/// One `sWallpapers` record: the tile stream, tilemap stream and palette pair
/// its row points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcWallpaperRoot {
    /// The `WALLPAPER_*` value, which is also the record's row in `sWallpapers`.
    pub wallpaper: u8,
    /// The frame's tiles then the background's, as one 8-pixel-wide strip.
    pub tiles: ImageRoot,
    /// The 20x18 tilemap, stored as an LZ77 stream.
    pub tilemap: BlobRoot,
    /// The frame palette: the first of the two the record's `palettes` names.
    pub frame_palette: PaletteRoot,
    /// The background palette: the 32 bytes after the frame palette.
    pub background_palette: PaletteRoot,
}

/// The wallpaper roots and the table that selects them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcWallpaperRoots {
    /// `sWallpapers`: 12-byte `{tiles, tilemap, palettes}` rows.
    pub table: GbaPtr,
    /// Every non-default standard wallpaper, in row order.
    pub wallpapers: &'static [PcWallpaperRoot],
}

impl PcWallpaperRoots {
    /// No wallpaper roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        table: GbaPtr::AT_BASE,
        wallpapers: &[],
    };
}
