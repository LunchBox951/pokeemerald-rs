//! Borrowed, typed views over a loaded [`AssetPack`](super::AssetPack)'s
//! entries: [`ImageRef`], [`PaletteRef`], and the bundling [`TilesetHandle`].

/// A borrowed view over one [`EntryKind::Image`](super::EntryKind::Image)
/// entry's decoded pixels.
#[derive(Debug, Clone, Copy)]
pub struct ImageRef<'a> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The source PNG's bit depth (4 or 8) — informational.
    pub bit_depth: u8,
    /// `width * height` palette-index bytes, row-major.
    pub pixels: &'a [u8],
}

/// A borrowed view over one [`EntryKind::Palette`](super::EntryKind::Palette)
/// entry's colours.
#[derive(Debug, Clone, Copy)]
pub struct PaletteRef<'a> {
    /// Number of colours.
    pub color_count: u16,
    pub(super) raw: &'a [u8],
}

impl<'a> PaletteRef<'a> {
    /// The colour at `index`, as a packed GBA BGR555 value (bits 0-4 red,
    /// 5-9 green, 10-14 blue), or `None` if out of range.
    #[must_use]
    pub fn color(&self, index: usize) -> Option<u16> {
        let start = index.checked_mul(2)?;
        let bytes = self.raw.get(start..start + 2)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Every colour, in order, as packed GBA BGR555 values.
    pub fn colors(&self) -> impl Iterator<Item = u16> + 'a {
        self.raw
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
}

/// A tileset's bundled graphics: its tile bitmap, all 16 palette slots, and
/// its raw metatile tables (see `xtask::extract::mod`'s module docs for
/// exactly what's in each — `metatiles`/`metatile_attributes` are opaque,
/// upstream-format bytes this crate does not yet decode).
#[derive(Debug, Clone, Copy)]
pub struct TilesetHandle<'a> {
    /// The tileset's tile bitmap.
    pub tiles: ImageRef<'a>,
    /// The tileset's 16 palette slots, in upstream `palettes/00..15` order.
    pub palettes: [PaletteRef<'a>; 16],
    /// Raw `metatiles.bin` bytes (undecoded).
    pub metatiles: &'a [u8],
    /// Raw `metatile_attributes.bin` bytes (undecoded).
    pub metatile_attributes: &'a [u8],
}
