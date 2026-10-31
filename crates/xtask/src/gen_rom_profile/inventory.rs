//! Fixed pack roots that must exist before location; ids come from [`crate::extract::scope`].

use super::error::GenRomProfileError;
use super::pack_source::PackSource;
use crate::extract::scope::{
    battle_interface_ids, healthbox_element_ids, healthbox_frame_ids, layout_ids,
    opening_scene_ids, poke_ball_ids, sprite_palette_ids, sprite_sheet_ids, starter_chooser_ids,
    text_window_ids, tileset_animation_ids, title_ids, BATTLERS, FONTS, INTERFACE_PALETTE_IDS,
    TILESETS,
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

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every tileset payload root and pinned animation frame is present.
pub fn require_tilesets(pack: &PackSource) -> Result<(), GenRomProfileError> {
    TILESETS
        .into_iter()
        .flat_map(|tileset| tileset_payload_ids(tileset.name))
        .try_for_each(|id| pack.get(&id).map(|_| ()))?;
    tileset_animation_ids()
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every pinned sprite sheet and sprite palette is present.
pub fn require_sprites(pack: &PackSource) -> Result<(), GenRomProfileError> {
    sprite_sheet_ids()
        .into_iter()
        .chain(sprite_palette_ids())
        .try_for_each(|id| pack.get(&id).map(|_| ()))
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every title image, tilemap, and palette is present.
pub fn require_title(pack: &PackSource) -> Result<(), GenRomProfileError> {
    title_ids()
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every Latin glyph sheet is present.
pub fn require_fonts(pack: &PackSource) -> Result<(), GenRomProfileError> {
    FONTS
        .into_iter()
        .try_for_each(|font| pack.get(font.pack_id).map(|_| ()))
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every layout map and border is present.
pub fn require_layouts(pack: &PackSource) -> Result<(), GenRomProfileError> {
    layout_ids()
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every text-window frame, message box, and palette is present.
pub fn require_text_window(pack: &PackSource) -> Result<(), GenRomProfileError> {
    text_window_ids()
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless the battle-message token table is present.
pub fn require_battle_messages(pack: &PackSource) -> Result<(), GenRomProfileError> {
    pack.get(rom_import::BATTLE_MESSAGE_PACK_ID).map(|_| ())
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless the field-message token table is present.
pub fn require_field_messages(pack: &PackSource) -> Result<(), GenRomProfileError> {
    pack.get(rom_import::FIELD_MESSAGE_PACK_ID).map(|_| ())
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless the battle-animation catalog is present.
pub fn require_battle_anims(pack: &PackSource) -> Result<(), GenRomProfileError> {
    pack.get(rom_import::BATTLE_ANIM_PACK_ID).map(|_| ())
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every starter battler image and palette is present.
pub fn require_battlers(pack: &PackSource) -> Result<(), GenRomProfileError> {
    BATTLERS
        .iter()
        .flat_map(|battler| battler.ids())
        .try_for_each(|id| pack.get(&id).map(|_| ()))
}

/// Refuses with `MissingPackEntry` for an absent entry, or for an empty expectation (`bag/*`) so a
/// whole missing family cannot pass as a complete one, and with
/// [`GenRomProfileError::StructMismatch`] for a pack `bag/` entry the expectation lacks, which
/// the bag locator, reading only the expectation, would otherwise silently leave out of the profile.
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

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every starter-chooser image,
/// palette, and tilemap is present, and with [`GenRomProfileError::StructMismatch`] for a pack
/// `starter-chooser/` entry outside the fixed inventory, which the locator, reading only the
/// inventory, would otherwise silently leave out of the profile.
pub fn require_starter_chooser(pack: &PackSource) -> Result<(), GenRomProfileError> {
    let expected = starter_chooser_ids();
    expected
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))?;
    if let Some(surplus) = pack
        .ids_with_prefix("starter-chooser/")
        .into_iter()
        .find(|id| !expected.contains(id))
    {
        return Err(GenRomProfileError::StructMismatch {
            id: surplus,
            reason: "is not in the checkout's starter-chooser inventory".to_owned(),
        });
    }
    Ok(())
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every opening-scene image,
/// palette, and tilemap is present, and with [`GenRomProfileError::StructMismatch`] for a pack
/// `opening-scene/` entry outside the fixed inventory, which the locator, reading only the
/// inventory, would otherwise silently leave out of the profile.
pub fn require_opening_scene(pack: &PackSource) -> Result<(), GenRomProfileError> {
    let expected = opening_scene_ids();
    expected
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))?;
    if let Some(surplus) = pack
        .ids_with_prefix("opening-scene/")
        .into_iter()
        .find(|id| !expected.contains(id))
    {
        return Err(GenRomProfileError::StructMismatch {
            id: surplus,
            reason: "is not in the checkout's opening-scene inventory".to_owned(),
        });
    }
    Ok(())
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless the Poké Ball's sheet, palette,
/// and opened-ball sheet are all present, and with [`GenRomProfileError::StructMismatch`] for a
/// pack `poke-ball/` entry outside the fixed inventory, which the locator, reading only the
/// inventory, would otherwise silently leave out of the profile.
pub fn require_poke_ball(pack: &PackSource) -> Result<(), GenRomProfileError> {
    let expected = poke_ball_ids();
    expected
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))?;
    if let Some(surplus) = pack
        .ids_with_prefix("poke-ball/")
        .into_iter()
        .find(|id| !expected.contains(id))
    {
        return Err(GenRomProfileError::StructMismatch {
            id: surplus,
            reason: "is not in the checkout's poke-ball inventory".to_owned(),
        });
    }
    Ok(())
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless the battle textbox image, its
/// three palettes, its tilemap, the health-box elements, the battle bar, the two singles frames,
/// the enemy shadow, and the two ball palettes are present, and with
/// [`GenRomProfileError::StructMismatch`] for a pack `battle-interface/` entry outside the fixed
/// inventory, which the locator, reading only the inventory, would otherwise silently leave out
/// of the profile. The same holds for every health-box record the checkout extracts there.
pub fn require_battle_interface(pack: &PackSource) -> Result<(), GenRomProfileError> {
    let expected: Vec<String> = battle_interface_ids()
        .into_iter()
        .chain(healthbox_element_ids())
        .chain(healthbox_frame_ids())
        .collect();
    expected
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))?;
    if let Some(surplus) = pack
        .ids_with_prefix("battle-interface/")
        .into_iter()
        .find(|id| !expected.contains(id))
    {
        return Err(GenRomProfileError::StructMismatch {
            id: surplus,
            reason: "is not in the checkout's battle-interface inventory".to_owned(),
        });
    }
    Ok(())
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every interface palette is present.
pub fn require_interface_palettes(pack: &PackSource) -> Result<(), GenRomProfileError> {
    INTERFACE_PALETTE_IDS
        .into_iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Refuses every missing fixed root, and any surplus `bag/` entry, before a domain writes a report line.
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
    require_field_messages(pack)?;
    require_interface_palettes(pack)?;
    require_bag(pack, bag_ids)?;
    require_starter_chooser(pack)?;
    require_opening_scene(pack)?;
    require_poke_ball(pack)?;
    require_battle_interface(pack)
}
