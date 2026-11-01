//! Decodes the title pack's raw palette and tilemap blobs.

use super::{TitleSceneError, BG_DIM_TILES, LOGO_PALETTE_COLORS, RAYQUAZA_CLOUDS_PALETTE_COLORS};
use assets::PaletteRef;
use rendering::{AffineTilemap, Bgr555, Palette, RenderError, ScreenEntry, Tilemap};

pub(super) fn title_palette_from_refs(
    logo: PaletteRef<'_>,
    rayquaza_clouds: PaletteRef<'_>,
) -> Palette {
    let mut colors = [Bgr555::default(); Palette::LEN];

    let logo_colors = usize::from(logo.color_count).min(LOGO_PALETTE_COLORS);
    for (slot, raw) in colors.iter_mut().zip(logo.colors()).take(logo_colors) {
        *slot = Bgr555::from_raw(raw);
    }

    let rayquaza_clouds_colors =
        usize::from(rayquaza_clouds.color_count).min(RAYQUAZA_CLOUDS_PALETTE_COLORS);
    for (slot, raw) in colors[LOGO_PALETTE_COLORS..]
        .iter_mut()
        .zip(rayquaza_clouds.colors())
        .take(rayquaza_clouds_colors)
    {
        *slot = Bgr555::from_raw(raw);
    }

    Palette::new(colors)
}

/// Entry count to report for a wrong-length raw tilemap; rounds toward the valid count from
/// whichever side `len` falls on, so it never equals `expected_bytes`'s entry count.
pub(super) fn reported_entry_count(len: usize, expected_bytes: usize) -> usize {
    if len < expected_bytes {
        len / 2
    } else {
        len.div_ceil(2)
    }
}

pub(super) fn regular_tilemap_from_raw(raw: &[u8]) -> Result<Tilemap, TitleSceneError> {
    let expected_entries = BG_DIM_TILES * BG_DIM_TILES;
    let expected_bytes = expected_entries * 2;
    if raw.len() != expected_bytes {
        return Err(TitleSceneError::from(RenderError::TilemapSizeMismatch {
            expected: expected_entries,
            actual: reported_entry_count(raw.len(), expected_bytes),
        }));
    }
    let entries: Vec<ScreenEntry> = raw
        .chunks_exact(2)
        .map(|b| ScreenEntry::from_raw(u16::from_le_bytes([b[0], b[1]])))
        .collect();
    Tilemap::new(BG_DIM_TILES, BG_DIM_TILES, entries).map_err(TitleSceneError::from)
}

pub(super) fn affine_tilemap_from_raw(raw: &[u8]) -> Result<AffineTilemap, TitleSceneError> {
    AffineTilemap::new(BG_DIM_TILES, BG_DIM_TILES, raw.to_vec()).map_err(TitleSceneError::from)
}
