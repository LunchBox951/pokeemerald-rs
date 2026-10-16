//! The fixed asset roots a pack must hold before anything is located.
//!
//! The ids come from [`crate::extract::scope`], the lists extraction itself
//! is fixed by, so the two cannot drift.

use super::error::GenRomProfileError;
use super::pack_source::PackSource;
use crate::extract::scope::{
    layout_ids, sprite_palette_ids, sprite_sheet_ids, text_window_ids, tileset_animation_ids,
    title_ids, BATTLERS, FONTS, TILESETS,
};

/// Require every fixed tileset root: all tile sheets, then every pinned
/// animation frame, independent of which frames the pack happens to hold.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_tilesets(pack: &PackSource) -> Result<(), GenRomProfileError> {
    TILESETS
        .into_iter()
        .try_for_each(|tileset| pack.get(&tileset.tiles_id()).map(|_| ()))?;
    tileset_animation_ids()
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Require every pinned people sprite sheet and fixed sprite palette, independent of which the pack holds.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_sprites(pack: &PackSource) -> Result<(), GenRomProfileError> {
    sprite_sheet_ids()
        .into_iter()
        .chain(sprite_palette_ids())
        .try_for_each(|id| pack.get(&id).map(|_| ()))
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

/// Require the battle-message token table root.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] when the pack lacks it.
pub fn require_battle_messages(pack: &PackSource) -> Result<(), GenRomProfileError> {
    pack.get(rom_import::BATTLE_MESSAGE_PACK_ID).map(|_| ())
}

/// Require every starter battler image and palette.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_battlers(pack: &PackSource) -> Result<(), GenRomProfileError> {
    BATTLERS
        .iter()
        .flat_map(|battler| battler.ids())
        .try_for_each(|id| pack.get(&id).map(|_| ()))
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
    require_text_window(pack)?;
    require_sprites(pack)?;
    require_battlers(pack)?;
    require_battle_messages(pack)
}
