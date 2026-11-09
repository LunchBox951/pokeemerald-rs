//! Birch's new-game speech scene and the wall clock: the checkout side of
//! `sBirchSpeech*` (`src/main_menu.c:250-257`), `sNewGameBirch_*`
//! (`src/field_effect.c:245-247`), Lotad's front picture and normal palette
//! (`src/data/graphics/pokemon.h:1888-1889`), and the wall-clock sheets,
//! palettes, and tilemaps (`src/wallclock.c:74-75`, `src/graphics.c:1350-1354`).

use std::path::{Path, PathBuf};

use pack_format::PackWriter;

use super::scene_pipeline::{
    self, ImageSource, PaletteFile, PaletteSource, SceneRules, TilemapSource,
};
use super::{png, ExtractError};

/// A 4-bit sample selects from a 16-colour bank.
const BANK_COLORS: usize = 16;
/// A 32x20 screen of two-byte tilemap entries.
const SCREEN_TILEMAP_BYTES: usize = 32 * 20 * 2;

/// Every sheet is 4-bit, relative to the checkout's `graphics/`. `hand.png` is
/// 4-bit grayscale, which `gbagfx` inverts into palette indices.
const IMAGES: [ImageSource; 5] = [
    ImageSource {
        png: "birch_speech/shadow.png",
        id: "opening-scene/image/shadow",
        width: 128,
        height: 24,
    },
    ImageSource {
        png: "birch_speech/birch.png",
        id: "opening-scene/image/birch",
        width: 64,
        height: 64,
    },
    ImageSource {
        png: "pokemon/lotad/front.png",
        id: "opening-scene/image/lotad",
        width: 64,
        height: 64,
    },
    ImageSource {
        png: "wallclock/clock.png",
        id: "opening-scene/image/wall_clock",
        width: 128,
        height: 64,
    },
    ImageSource {
        png: "wallclock/hand.png",
        id: "opening-scene/image/clock_hand",
        width: 64,
        height: 144,
    },
];

const PALETTES: [PaletteSource; 8] = [
    PaletteSource {
        file: PaletteFile::Jasc("birch_speech/bg0.pal"),
        id: "opening-scene/palette/bg0",
        colors: BANK_COLORS,
    },
    PaletteSource {
        file: PaletteFile::Jasc("birch_speech/bg1.pal"),
        id: "opening-scene/palette/bg1",
        colors: BANK_COLORS,
    },
    PaletteSource {
        file: PaletteFile::Jasc("birch_speech/bg2.pal"),
        id: "opening-scene/palette/bg2",
        colors: BANK_COLORS,
    },
    PaletteSource {
        file: PaletteFile::PngPlte("birch_speech/birch.png"),
        id: "opening-scene/palette/birch",
        colors: BANK_COLORS,
    },
    PaletteSource {
        file: PaletteFile::Jasc("pokemon/lotad/normal.pal"),
        id: "opening-scene/palette/lotad",
        colors: BANK_COLORS,
    },
    PaletteSource {
        file: PaletteFile::Jasc("wallclock/male.pal"),
        id: "opening-scene/palette/clock_male",
        colors: BANK_COLORS,
    },
    PaletteSource {
        file: PaletteFile::Jasc("wallclock/female.pal"),
        id: "opening-scene/palette/clock_female",
        colors: BANK_COLORS,
    },
    PaletteSource {
        file: PaletteFile::Jasc("wallclock/text_prompt.pal"),
        id: "opening-scene/palette/clock_text_prompt",
        colors: 4,
    },
];

const TILEMAPS: [TilemapSource; 3] = [
    TilemapSource {
        path: "graphics/birch_speech/map.bin",
        id: "opening-scene/raw/birch_speech_map",
    },
    TilemapSource {
        path: "graphics/wallclock/clock_start.bin",
        id: "opening-scene/raw/clock_start",
    },
    TilemapSource {
        path: "graphics/wallclock/clock_view.bin",
        id: "opening-scene/raw/clock_view",
    },
];

/// What separates this scene's pipeline from the other's.
const RULES: SceneRules = SceneRules {
    decode_image: png::decode_truncating_palette,
    tilemap_bytes: SCREEN_TILEMAP_BYTES,
    mismatch,
};

/// Every pack id [`extract_opening_scene`] emits, images first, then
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

/// Extracts the scene's fixed manifest: five sheets, eight palettes, and
/// three tilemaps. Nothing is queued unless every source validated.
pub(super) fn extract_opening_scene(
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
    ExtractError::OpeningSceneAssetMismatch { path, detail }
}

#[cfg(test)]
mod tests;
