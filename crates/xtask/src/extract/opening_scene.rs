//! Birch's new-game speech scene and the wall clock: the checkout side of
//! `sBirchSpeech*` (`src/main_menu.c:250-257`), `sNewGameBirch_*`
//! (`src/field_effect.c:245-247`), Lotad's front picture and normal palette
//! (`src/data/graphics/pokemon.h:1888-1889`), and the wall-clock sheets,
//! palettes, and tilemaps (`src/wallclock.c:74-75`, `src/graphics.c:1350-1354`).

use std::path::Path;

use pack_format::{PackEntry, PackWriter};

use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, read_file, read_text, ExtractError,
};

/// The PNG bit depth every scene sheet carries and the typed accessors require.
const BIT_DEPTH: u8 = 4;
/// A 4-bit sample selects from a 16-colour bank.
const BANK_COLORS: usize = 16;
/// A 32x20 screen of two-byte tilemap entries.
const SCREEN_TILEMAP_BYTES: usize = 32 * 20 * 2;

/// A 4-bit sheet, relative to the checkout's `graphics/`.
///
/// `hand.png` is 4-bit grayscale, which `gbagfx` inverts into palette indices.
#[derive(Clone, Copy, Debug)]
struct ImageSource {
    png: &'static str,
    id: &'static str,
    width: u32,
    height: u32,
}

/// Where a 16-colour (or shorter) bank comes from.
#[derive(Clone, Copy, Debug)]
enum PaletteFile {
    /// A JASC `.pal` file.
    Jasc(&'static str),
    /// The `PLTE` chunk of the PNG `gbagfx` takes the in-game palette from.
    PngPlte(&'static str),
}

#[derive(Clone, Copy, Debug)]
struct PaletteSource {
    file: PaletteFile,
    id: &'static str,
    colors: usize,
}

#[derive(Clone, Copy, Debug)]
struct TilemapSource {
    path: &'static str,
    id: &'static str,
}

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
        entries.push(image_entry(&graphics, image)?);
    }
    for palette in PALETTES {
        entries.push(palette_entry(&graphics, palette)?);
    }
    for tilemap in TILEMAPS {
        entries.push(tilemap_entry(upstream, tilemap)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn image_entry(graphics: &Path, source: ImageSource) -> Result<PackEntry, ExtractError> {
    let path = graphics.join(source.png);
    let image = png::decode(&read_file(&path)?).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.width != source.width || image.height != source.height {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {}x{}",
                image.width, image.height, source.width, source.height
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

fn palette_entry(graphics: &Path, source: PaletteSource) -> Result<PackEntry, ExtractError> {
    let (path, colors) = match source.file {
        PaletteFile::Jasc(name) => {
            let path = graphics.join(name);
            let colors = jasc_pal::parse(&read_text(&path)?)
                .map_err(|e| ExtractError::Pal(path.clone(), e))?;
            (path, colors)
        }
        PaletteFile::PngPlte(name) => {
            let path = graphics.join(name);
            let colors = png::decode_palette(&read_file(&path)?)
                .map_err(|e| ExtractError::Png(path.clone(), e))?;
            (path, colors)
        }
    };
    if colors.len() != source.colors {
        return Err(mismatch(
            &path,
            format!("has {} colours: expected {}", colors.len(), source.colors),
        ));
    }
    build_palette_entry(&path, &colors, source.id.to_owned())
}

fn tilemap_entry(upstream: &Path, source: TilemapSource) -> Result<PackEntry, ExtractError> {
    let path = upstream.join(source.path);
    let payload = read_file(&path)?;
    if payload.len() != SCREEN_TILEMAP_BYTES {
        return Err(mismatch(
            &path,
            format!(
                "is {} bytes: expected {SCREEN_TILEMAP_BYTES}",
                payload.len()
            ),
        ));
    }
    Ok(pack_format::raw_entry(source.id.to_owned(), payload))
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::OpeningSceneAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
