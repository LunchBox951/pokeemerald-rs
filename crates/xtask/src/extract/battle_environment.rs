//! The checkpoint battle environments: the checkout side of the tall-grass and
//! plain `sBattleEnvironmentTable` rows (`src/battle_bg.c`) and the
//! declarations in `src/data/graphics/battle_environment.h`.
//!
//! Plain aliases the building environment's tiles, tilemap, entry tiles and
//! entry tilemap and differs only in its palette, so the manifest carries the
//! building art once and both palettes. It names each source instead of
//! globbing `graphics/battle_environment/`, which holds many environments
//! outside this closure.

use std::path::Path;

use pack_format::{PackEntry, PackWriter};

use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, read_file, read_text, ExtractError,
};

/// The PNG bit depth every tile sheet carries and the typed accessor requires.
const BIT_DEPTH: u8 = 4;
/// Both tile sheets are 128x128 pixels.
const SHEET_SIZE: (u32, u32) = (128, 128);
/// Three 16-colour banks.
const PALETTE_COLORS: usize = 48;
/// BG3 at screen size 1: 64x32 two-byte entries.
const MAIN_TILEMAP_BYTES: usize = 64 * 32 * 2;
/// One 32x32 screenblock of two-byte entries.
const ENTRY_TILEMAP_BYTES: usize = 32 * 32 * 2;

/// A 4-bit tile sheet, relative to the checkout's `graphics/battle_environment/`.
#[derive(Clone, Copy, Debug)]
struct ImageSource {
    png: &'static str,
    id: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct PaletteSource {
    pal: &'static str,
    id: &'static str,
}

/// A raw little-endian tilemap, the uncompressed source of the ROM's LZ77 blob.
#[derive(Clone, Copy, Debug)]
struct TilemapSource {
    bin: &'static str,
    id: &'static str,
    bytes: usize,
}

const IMAGES: [ImageSource; 4] = [
    ImageSource {
        png: "tall_grass/tiles.png",
        id: "battle-environment/image/tall_grass/tiles",
    },
    ImageSource {
        png: "tall_grass/anim_tiles.png",
        id: "battle-environment/image/tall_grass/anim_tiles",
    },
    ImageSource {
        png: "building/tiles.png",
        id: "battle-environment/image/building/tiles",
    },
    ImageSource {
        png: "building/anim_tiles.png",
        id: "battle-environment/image/building/anim_tiles",
    },
];

/// The explicit `.pal` files, not the sheets' own `PLTE` chunks: the
/// environment palette is 48 colours while the sheets are 4-bit.
const PALETTES: [PaletteSource; 2] = [
    PaletteSource {
        pal: "tall_grass/palette.pal",
        id: "battle-environment/palette/tall_grass",
    },
    PaletteSource {
        pal: "plain/palette.pal",
        id: "battle-environment/palette/plain",
    },
];

const TILEMAPS: [TilemapSource; 4] = [
    TilemapSource {
        bin: "tall_grass/map.bin",
        id: "battle-environment/raw/tall_grass/map",
        bytes: MAIN_TILEMAP_BYTES,
    },
    TilemapSource {
        bin: "tall_grass/anim_map.bin",
        id: "battle-environment/raw/tall_grass/anim_map",
        bytes: ENTRY_TILEMAP_BYTES,
    },
    TilemapSource {
        bin: "building/map.bin",
        id: "battle-environment/raw/building/map",
        bytes: MAIN_TILEMAP_BYTES,
    },
    TilemapSource {
        bin: "building/anim_map.bin",
        id: "battle-environment/raw/building/anim_map",
        bytes: ENTRY_TILEMAP_BYTES,
    },
];

/// Every pack id [`extract_battle_environment`] emits, images first, then
/// palettes, then tilemaps. The profile generator reads this as its fixed
/// inventory so extraction and generation cannot drift apart.
pub(crate) fn pack_ids() -> Vec<String> {
    IMAGES
        .iter()
        .map(|image| image.id)
        .chain(PALETTES.iter().map(|palette| palette.id))
        .chain(TILEMAPS.iter().map(|tilemap| tilemap.id))
        .map(str::to_owned)
        .collect()
}

/// Extracts the fixed manifest: four tile sheets, two palettes, and four
/// tilemaps. Nothing is queued unless every source validated.
pub(super) fn extract_battle_environment(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let root = upstream.join("graphics/battle_environment");
    let mut entries = Vec::with_capacity(IMAGES.len() + PALETTES.len() + TILEMAPS.len());
    for image in IMAGES {
        entries.push(image_entry(&root, image)?);
    }
    for palette in PALETTES {
        entries.push(palette_entry(&root, palette)?);
    }
    for tilemap in TILEMAPS {
        entries.push(tilemap_entry(&root, tilemap)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn image_entry(root: &Path, source: ImageSource) -> Result<PackEntry, ExtractError> {
    let path = root.join(source.png);
    let image = png::decode(&read_file(&path)?).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if (image.width, image.height) != SHEET_SIZE {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {}x{}",
                image.width, image.height, SHEET_SIZE.0, SHEET_SIZE.1
            ),
        ));
    }
    if image.bit_depth != BIT_DEPTH {
        return Err(mismatch(
            &path,
            format!("is {}-bit: expected {BIT_DEPTH}-bit", image.bit_depth),
        ));
    }
    build_image_entry(&path, source.id.to_owned(), image)
}

fn palette_entry(root: &Path, source: PaletteSource) -> Result<PackEntry, ExtractError> {
    let path = root.join(source.pal);
    let colors =
        jasc_pal::parse(&read_text(&path)?).map_err(|e| ExtractError::Pal(path.clone(), e))?;
    if colors.len() != PALETTE_COLORS {
        return Err(mismatch(
            &path,
            format!("has {} colours: expected {PALETTE_COLORS}", colors.len()),
        ));
    }
    build_palette_entry(&path, &colors, source.id.to_owned())
}

fn tilemap_entry(root: &Path, source: TilemapSource) -> Result<PackEntry, ExtractError> {
    let path = root.join(source.bin);
    let payload = read_file(&path)?;
    if payload.len() != source.bytes {
        return Err(mismatch(
            &path,
            format!("is {} bytes: expected {}", payload.len(), source.bytes),
        ));
    }
    Ok(pack_format::raw_entry(source.id.to_owned(), payload))
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::BattleEnvironmentAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
