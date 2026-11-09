//! The evolution scene's own graphics: the checkout side of `sBgAnim_*`
//! (`src/evolution_scene.c:71-78`) and `sEvoSparkle_*`
//! (`src/evolution_graphics.c:41-42`). The four `unused_*.pal` files sit
//! among those declarations but nothing loads them, so the manifest names
//! each source instead of globbing the directory.

use std::path::{Path, PathBuf};

use pack_format::PackWriter;

use super::scene_pipeline::{
    self, ImageSource, PaletteFile, PaletteSource, SceneRules, TilemapSource,
};
use super::{png, ExtractError};

/// A 32x32 background of two-byte tilemap entries.
const BACKGROUND_TILEMAP_BYTES: usize = 32 * 32 * 2;

/// Both images are 4-bit, relative to the checkout's `graphics/`. `bg.png` is
/// 4-bit grayscale, which `gbagfx` inverts into tile indices; the sparkle is
/// indexed.
const IMAGES: [ImageSource; 2] = [
    ImageSource {
        png: "evolution_scene/bg.png",
        id: "evolution-scene/image/bg",
        width: 128,
        height: 112,
    },
    ImageSource {
        png: "misc/evo_sparkle.png",
        id: "evolution-scene/image/evo_sparkle",
        width: 8,
        height: 8,
    },
];

/// The intro file lists all 256 colours of an 8-bit palette although the scene
/// copies only its first bank; the animation file is the 32-colour lookup
/// table the running background draws from.
const PALETTES: [PaletteSource; 3] = [
    PaletteSource {
        file: PaletteFile::Jasc("evolution_scene/bg_anim_intro.pal"),
        id: "evolution-scene/palette/bg_anim_intro",
        colors: 256,
    },
    PaletteSource {
        file: PaletteFile::Jasc("evolution_scene/bg_anim.pal"),
        id: "evolution-scene/palette/bg_anim",
        colors: 32,
    },
    PaletteSource {
        file: PaletteFile::PngPlte("misc/evo_sparkle.png"),
        id: "evolution-scene/palette/evo_sparkle",
        colors: 16,
    },
];

const TILEMAPS: [TilemapSource; 2] = [
    TilemapSource {
        path: "graphics/evolution_scene/bg_inner.bin",
        id: "evolution-scene/raw/bg_inner",
    },
    TilemapSource {
        path: "graphics/evolution_scene/bg_outer.bin",
        id: "evolution-scene/raw/bg_outer",
    },
];

/// What separates this scene's pipeline from the other's.
const RULES: SceneRules = SceneRules {
    decode_image: png::decode,
    tilemap_bytes: BACKGROUND_TILEMAP_BYTES,
    mismatch,
};

/// Every pack id [`extract_evolution_scene`] emits, images first, then
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

/// Extracts the scene's fixed manifest: two images, three palettes, and two
/// tilemaps. Nothing is queued unless every source validated.
pub(super) fn extract_evolution_scene(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let graphics = upstream.join("graphics");
    let mut entries = Vec::with_capacity(IMAGES.len() + PALETTES.len() + TILEMAPS.len());
    for image in IMAGES {
        entries.push(scene_pipeline::image_entry(RULES, &graphics, image)?);
    }
    for palette in PALETTES {
        entries.push(scene_pipeline::palette_entry(RULES, &graphics, palette)?);
    }
    for tilemap in TILEMAPS {
        entries.push(scene_pipeline::tilemap_entry(RULES, upstream, tilemap)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn mismatch(path: PathBuf, detail: String) -> ExtractError {
    ExtractError::EvolutionSceneAssetMismatch { path, detail }
}

#[cfg(test)]
mod tests;
