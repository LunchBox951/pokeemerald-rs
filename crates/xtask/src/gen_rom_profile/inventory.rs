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

/// Every fixed payload root of a tileset: tiles, metatiles, metatile
/// attributes, and each palette bank, as `tilesets::locate_one` asks for them.
pub fn tileset_payload_ids(name: &str) -> Vec<String> {
    [
        format!("tileset/{name}/tiles"),
        format!("tileset/{name}/metatiles"),
        format!("tileset/{name}/metatile-attributes"),
    ]
    .into_iter()
    .chain(
        (0..super::tilesets::PALETTE_BANKS).map(|bank| format!("tileset/{name}/palette/{bank:02}")),
    )
    .collect()
}

/// Require every fixed tileset root: each tileset's tile sheet, metatiles,
/// attributes, and palette banks, then every pinned animation frame,
/// independent of which frames the pack happens to hold.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root.
pub fn require_tilesets(pack: &PackSource) -> Result<(), GenRomProfileError> {
    TILESETS
        .into_iter()
        .flat_map(|tileset| tileset_payload_ids(tileset.name))
        .try_for_each(|id| pack.get(&id).map(|_| ()))?;
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

/// Require the battle-animation catalog root.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] when the pack lacks it.
pub fn require_battle_anims(pack: &PackSource) -> Result<(), GenRomProfileError> {
    pack.get(rom_import::BATTLE_ANIM_PACK_ID).map(|_| ())
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

/// Require exactly the bag entries the checkout's inventory expects.
///
/// The bag locator derives its roots from the expectation, not the pack, so
/// a pack entry outside it would otherwise be dropped from the profile.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent entry,
/// or `bag/*` when the expectation is empty, so a whole missing family
/// cannot pass as a complete one. [`GenRomProfileError::StructMismatch`]
/// naming the first pack `bag/` entry the expectation does not hold.
pub fn require_bag(pack: &PackSource, expected: &[String]) -> Result<(), GenRomProfileError> {
    if expected.is_empty() {
        return Err(GenRomProfileError::MissingPackEntry("bag/*".to_owned()));
    }
    expected
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))?;
    if let Some(surplus) = pack
        .ids_with_prefix("bag/")
        .into_iter()
        .find(|id| !expected.contains(id))
    {
        return Err(GenRomProfileError::StructMismatch {
            id: surplus,
            reason: "is not in the checkout's bag inventory".to_owned(),
        });
    }
    Ok(())
}

/// Require every fixed root, before any domain writes a report line.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first absent root,
/// or [`GenRomProfileError::StructMismatch`] for a surplus `bag/` entry.
pub fn preflight(pack: &PackSource, bag_ids: &[String]) -> Result<(), GenRomProfileError> {
    require_tilesets(pack)?;
    require_title(pack)?;
    require_fonts(pack)?;
    require_layouts(pack)?;
    require_text_window(pack)?;
    require_sprites(pack)?;
    require_battle_anims(pack)?;
    require_battlers(pack)?;
    require_battle_messages(pack)?;
    require_bag(pack, bag_ids)
}
