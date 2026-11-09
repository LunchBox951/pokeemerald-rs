//! The extraction pipeline `opening_scene` and `evolution_scene` share: palette
//! source dispatch and colour-count validation, tilemap size validation, and
//! image dimension and bit-depth validation, each ending in one pack entry.
//!
//! Each scene owns its asset tables and the three things that genuinely differ,
//! which [`SceneRules`] names so a shared edit cannot erase them: the PNG
//! decoder, the tilemap size, and the error variant a mismatch is reported as.

use std::path::{Path, PathBuf};

use pack_format::PackEntry;

use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, read_file, read_text, ExtractError,
};

/// The PNG bit depth every scene sheet carries and the typed accessors require.
const BIT_DEPTH: u8 = 4;

/// What one scene does differently from the other.
#[derive(Clone, Copy)]
pub(super) struct SceneRules {
    /// The PNG decoder for [`ImageSource`] files. The opening scene truncates
    /// an oversized palette ([`png::decode_truncating_palette`]); the
    /// evolution scene rejects one ([`png::decode`]).
    pub(super) decode_image: fn(&[u8]) -> Result<png::IndexedImage, png::PngError>,
    /// The exact byte length of every [`TilemapSource`] file.
    pub(super) tilemap_bytes: usize,
    /// Wraps a path and detail in the scene's own asset-mismatch error.
    pub(super) mismatch: fn(PathBuf, String) -> ExtractError,
}

/// A 4-bit sheet, relative to the checkout's `graphics/`.
#[derive(Clone, Copy, Debug)]
pub(super) struct ImageSource {
    pub(super) png: &'static str,
    pub(super) id: &'static str,
    pub(super) width: u32,
    pub(super) height: u32,
}

/// Where a palette's colours come from.
#[derive(Clone, Copy, Debug)]
pub(super) enum PaletteFile {
    /// A JASC `.pal` file.
    Jasc(&'static str),
    /// The `PLTE` chunk of the PNG `gbagfx` takes the in-game palette from.
    PngPlte(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PaletteSource {
    pub(super) file: PaletteFile,
    pub(super) id: &'static str,
    pub(super) colors: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TilemapSource {
    pub(super) path: &'static str,
    pub(super) id: &'static str,
}

pub(super) fn image_entry(
    rules: SceneRules,
    graphics: &Path,
    source: ImageSource,
) -> Result<PackEntry, ExtractError> {
    let path = graphics.join(source.png);
    let image =
        (rules.decode_image)(&read_file(&path)?).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.width != source.width || image.height != source.height {
        return Err((rules.mismatch)(
            path,
            format!(
                "is {}x{}: expected {}x{}",
                image.width, image.height, source.width, source.height
            ),
        ));
    }
    if image.bit_depth != BIT_DEPTH {
        return Err((rules.mismatch)(
            path,
            format!("is {}-bit: expected {BIT_DEPTH}-bit", image.bit_depth),
        ));
    }
    build_image_entry(&path, source.id.to_owned(), image)
}

pub(super) fn palette_entry(
    rules: SceneRules,
    graphics: &Path,
    source: PaletteSource,
) -> Result<PackEntry, ExtractError> {
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
        return Err((rules.mismatch)(
            path,
            format!("has {} colours: expected {}", colors.len(), source.colors),
        ));
    }
    build_palette_entry(&path, &colors, source.id.to_owned())
}

pub(super) fn tilemap_entry(
    rules: SceneRules,
    upstream: &Path,
    source: TilemapSource,
) -> Result<PackEntry, ExtractError> {
    let path = upstream.join(source.path);
    let payload = read_file(&path)?;
    if payload.len() != rules.tilemap_bytes {
        let expected = rules.tilemap_bytes;
        return Err((rules.mismatch)(
            path,
            format!("is {} bytes: expected {expected}", payload.len()),
        ));
    }
    Ok(pack_format::raw_entry(source.id.to_owned(), payload))
}
