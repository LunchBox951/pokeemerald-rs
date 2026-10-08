//! The fixed asset roots a pack must hold before anything is located.
//!
//! The ids come from [`crate::extract::scope`], the lists extraction itself
//! is fixed by, so the two cannot drift.

use super::error::GenRomProfileError;
use super::pack_source::PackSource;
use crate::extract::scope::{layout_ids, text_window_ids, title_ids, FONTS, TILESETS};

/// Require every fixed tileset root.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_tilesets(pack: &PackSource) -> Result<(), GenRomProfileError> {
    TILESETS
        .into_iter()
        .try_for_each(|tileset| pack.get(&tileset.tiles_id()).map(|_| ()))
}

/// Require every fixed title image, tilemap, and palette root.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_title(pack: &PackSource) -> Result<(), GenRomProfileError> {
    title_ids()
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Require every fixed Latin glyph sheet root.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_fonts(pack: &PackSource) -> Result<(), GenRomProfileError> {
    FONTS
        .into_iter()
        .try_for_each(|font| pack.get(font.pack_id).map(|_| ()))
}

/// Require every fixed layout map and border root.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_layouts(pack: &PackSource) -> Result<(), GenRomProfileError> {
    layout_ids()
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Require every fixed text-window frame, message box, and palette root.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_text_window(pack: &PackSource) -> Result<(), GenRomProfileError> {
    text_window_ids()
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Require every fixed root, before any domain writes a report line.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn preflight(pack: &PackSource) -> Result<(), GenRomProfileError> {
    require_tilesets(pack)?;
    require_title(pack)?;
    require_fonts(pack)?;
    require_layouts(pack)?;
    require_text_window(pack)
}
