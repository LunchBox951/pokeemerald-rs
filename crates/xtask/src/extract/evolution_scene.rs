//! The evolution scene's own graphics: the checkout side of `sBgAnim_*`
//! (`src/evolution_scene.c:71-78`) and `sEvoSparkle_*`
//! (`src/evolution_graphics.c:41-42`). The four `unused_*.pal` files sit
//! among those declarations but nothing loads them, so the manifest names
//! each source instead of globbing the directory.

use std::path::Path;

use pack_format::{PackEntry, PackWriter};

use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, read_file, read_text, ExtractError,
};

/// The PNG bit depth both images carry and the typed accessor requires.
const BIT_DEPTH: u8 = 4;
/// A 32x32 background of two-byte tilemap entries.
const BACKGROUND_TILEMAP_BYTES: usize = 32 * 32 * 2;

/// A 4-bit image, relative to the checkout's `graphics/`.
///
/// `bg.png` is 4-bit grayscale, which `gbagfx` inverts into tile indices; the
/// sparkle is indexed.
#[derive(Clone, Copy, Debug)]
struct ImageSource {
    png: &'static str,
    id: &'static str,
    width: u32,
    height: u32,
}

/// Where a palette's colours come from.
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
    if payload.len() != BACKGROUND_TILEMAP_BYTES {
        return Err(mismatch(
            &path,
            format!(
                "is {} bytes: expected {BACKGROUND_TILEMAP_BYTES}",
                payload.len()
            ),
        ));
    }
    Ok(pack_format::raw_entry(source.id.to_owned(), payload))
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::EvolutionSceneAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
