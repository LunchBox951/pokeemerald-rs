//! Borrowed fields and typed bundles from a loaded [`AssetPack`](super::AssetPack).
//!
//! Pack loading validates payload ranges and declared shapes, then accessors
//! validate entry kinds before constructing these views. Their references point
//! into the pack's owned bytes and therefore cannot outlive it.

use std::mem::size_of;

const BYTES_PER_PALETTE_COLOR: usize = size_of::<u16>();

/// A palette-index image borrowing its pixel payload for `'a`.
///
/// Pack loading validates the declared shape and bit depth, and
/// [`AssetPack::image`](super::AssetPack::image) checks the entry kind before
/// returning this view.
#[derive(Debug, Clone, Copy)]
pub struct ImageRef<'a> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Source bit depth: 2, 4, or 8 bits per pixel.
    ///
    /// Pack payloads store one palette index byte per pixel at every depth.
    pub bit_depth: u8,
    /// Palette indices in row-major order, one byte per pixel.
    pub pixels: &'a [u8],
}

/// A palette borrowing little-endian GBA BGR555 colours for `'a`.
///
/// Pack loading validates the declared colour count against the payload length,
/// and [`AssetPack::palette`](super::AssetPack::palette) checks the entry kind
/// before returning this view.
#[derive(Debug, Clone, Copy)]
pub struct PaletteRef<'a> {
    /// Number of complete colours in the palette.
    pub color_count: u16,
    pub(super) raw: &'a [u8],
}

impl<'a> PaletteRef<'a> {
    /// Returns the BGR555 colour at `index`, or `None` when out of range.
    #[must_use]
    pub fn color(&self, index: usize) -> Option<u16> {
        let start = index.checked_mul(BYTES_PER_PALETTE_COLOR)?;
        let end = start.checked_add(BYTES_PER_PALETTE_COLOR)?;
        let bytes = self.raw.get(start..end)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Iterates over the palette's BGR555 colours in storage order.
    pub fn colors(&self) -> impl Iterator<Item = u16> + 'a {
        self.raw
            .chunks_exact(BYTES_PER_PALETTE_COLOR)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
}

/// Graphics and metatile data borrowing one tileset's payloads for `'a`.
///
/// [`AssetPack::tileset`](super::AssetPack::tileset) requires the tile image,
/// all 16 palettes, and both encoded metatile tables before returning this
/// handle.
#[derive(Debug, Clone, Copy)]
pub struct TilesetHandle<'a> {
    /// Palette-index tile bitmap.
    pub tiles: ImageRef<'a>,
    /// Sixteen palettes in slot order.
    pub palettes: [PaletteRef<'a>; 16],
    /// Encoded metatile-to-tile mappings.
    pub metatiles: &'a [u8],
    /// Encoded metatile attributes.
    pub metatile_attributes: &'a [u8],
}

impl<'a> TilesetHandle<'a> {
    /// Returns attributes indexed by this tileset's local metatile identity.
    ///
    /// The table borrows the pack bytes for `'a`, independently of the handle's
    /// temporary borrow.
    #[must_use]
    pub const fn metatile_attribute_table(
        &self,
    ) -> crate::metatile_attributes::MetatileAttributeTable<'a> {
        crate::metatile_attributes::MetatileAttributeTable::new(self.metatile_attributes)
    }
}

/// A window-frame tile sheet and 16-colour palette borrowing their payloads for
/// `'a`.
///
/// [`AssetPack::text_window_frame`](super::AssetPack::text_window_frame) and
/// [`AssetPack::message_box`](super::AssetPack::message_box) validate the
/// expected image dimensions, pixel count, palette size, and pixel indices
/// before returning this handle. Rendering code assigns the tiles to window
/// positions.
#[derive(Debug, Clone, Copy)]
pub struct WindowFrameHandle<'a> {
    /// Palette-index tile bitmap.
    pub tiles: ImageRef<'a>,
    /// Palette used by the tile bitmap.
    pub palette: PaletteRef<'a>,
}

#[cfg(test)]
mod tests {
    use super::{PaletteRef, BYTES_PER_PALETTE_COLOR};

    #[test]
    fn palette_color_returns_none_when_range_end_overflows() {
        let palette = PaletteRef {
            color_count: 1,
            raw: &[0x34, 0x12],
        };
        let index_with_overflowing_range_end = usize::MAX / BYTES_PER_PALETTE_COLOR;

        assert_eq!(palette.color(index_with_overflowing_range_end), None);
    }
}
