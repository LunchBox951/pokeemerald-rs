//! Fixed pack roots that must exist before location; ids come from [`crate::extract::scope`].

use super::error::GenRomProfileError;
use super::pack_source::PackSource;
use crate::extract::scope::{
    battle_interface_ids, evolution_scene_ids, healthbox_element_ids, healthbox_frame_ids,
    layout_ids, naming_screen_ids, opening_scene_ids, poke_ball_ids, sprite_palette_ids,
    sprite_sheet_ids, starter_chooser_ids, text_window_ids, tileset_animation_ids, title_ids,
    BATTLERS, FONTS, INTERFACE_IMAGE_IDS, INTERFACE_PALETTE_IDS, KEYPAD_ICONS, TILESETS,
};
use crate::extract::scope::{trainer_battler_ids, trainer_front_ids};

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

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every Latin glyph sheet and the keypad-icon sheet are present.
pub fn require_fonts(pack: &PackSource) -> Result<(), GenRomProfileError> {
    FONTS
        .into_iter()
        .chain([KEYPAD_ICONS])
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

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every battler image and palette is present.
pub fn require_battlers(pack: &PackSource) -> Result<(), GenRomProfileError> {
    BATTLERS
        .iter()
        .flat_map(|battler| battler.ids())
        .try_for_each(|id| pack.get(&id).map(|_| ()))
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] for the first `expected` id the pack
/// lacks, then with [`GenRomProfileError::StructMismatch`] for a pack entry under `prefix` that
/// `expected` omits: the domain locator reads only the inventory, so such an entry would
/// otherwise be silently left out of the profile. The diagnostic names the domain by `prefix`
/// without its trailing slash.
fn require_closed_inventory(
    pack: &PackSource,
    prefix: &str,
    expected: &[String],
) -> Result<(), GenRomProfileError> {
    expected
        .iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))?;
    match pack
        .ids_with_prefix(prefix)
        .into_iter()
        .find(|id| !expected.contains(id))
    {
        Some(surplus) => Err(GenRomProfileError::StructMismatch {
            id: surplus,
            reason: format!(
                "is not in the checkout's {} inventory",
                prefix.trim_end_matches('/')
            ),
        }),
        None => Ok(()),
    }
}

/// Closed `bag/` inventory; an empty expectation is refused as `MissingPackEntry("bag/*")` so a
/// whole missing family cannot pass as a complete one.
pub fn require_bag(pack: &PackSource, expected: &[String]) -> Result<(), GenRomProfileError> {
    if expected.is_empty() {
        return Err(GenRomProfileError::MissingPackEntry("bag/*".to_owned()));
    }
    require_closed_inventory(pack, "bag/", expected)
}

/// Closed `starter-chooser/` inventory.
pub fn require_starter_chooser(pack: &PackSource) -> Result<(), GenRomProfileError> {
    require_closed_inventory(pack, "starter-chooser/", &starter_chooser_ids())
}

/// Closed `opening-scene/` inventory.
pub fn require_opening_scene(pack: &PackSource) -> Result<(), GenRomProfileError> {
    require_closed_inventory(pack, "opening-scene/", &opening_scene_ids())
}

/// Closed `evolution-scene/` inventory.
pub fn require_evolution_scene(pack: &PackSource) -> Result<(), GenRomProfileError> {
    require_closed_inventory(pack, "evolution-scene/", &evolution_scene_ids())
}

/// Closed `poke-ball/` inventory.
pub fn require_poke_ball(pack: &PackSource) -> Result<(), GenRomProfileError> {
    require_closed_inventory(pack, "poke-ball/", &poke_ball_ids())
}

/// Closed `naming-screen/` inventory.
pub fn require_naming_screen(pack: &PackSource) -> Result<(), GenRomProfileError> {
    require_closed_inventory(pack, "naming-screen/", &naming_screen_ids())
}

/// Closed `battle-interface/` inventory, including every health-box record the checkout extracts there.
pub fn require_battle_interface(pack: &PackSource) -> Result<(), GenRomProfileError> {
    let expected: Vec<String> = battle_interface_ids()
        .into_iter()
        .chain(healthbox_element_ids())
        .chain(healthbox_frame_ids())
        .collect();
    require_closed_inventory(pack, "battle-interface/", &expected)
}

/// Closed `trainer-pic/` inventory.
pub fn require_trainer_front(pack: &PackSource) -> Result<(), GenRomProfileError> {
    require_closed_inventory(pack, "trainer-pic/", &trainer_front_ids())
}

/// Closed `trainer/` inventory.
pub fn require_trainer_battlers(pack: &PackSource) -> Result<(), GenRomProfileError> {
    require_closed_inventory(pack, "trainer/", &trainer_battler_ids())
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every interface palette is present.
pub fn require_interface_palettes(pack: &PackSource) -> Result<(), GenRomProfileError> {
    INTERFACE_PALETTE_IDS
        .into_iter()
        .try_for_each(|id| pack.get(id).map(|_| ()))
}

/// Refuses with [`GenRomProfileError::MissingPackEntry`] unless every interface image is present.
pub fn require_interface_images(pack: &PackSource) -> Result<(), GenRomProfileError> {
    INTERFACE_IMAGE_IDS
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
    require_interface_images(pack)?;
    require_interface_palettes(pack)?;
    require_bag(pack, bag_ids)?;
    require_starter_chooser(pack)?;
    require_opening_scene(pack)?;
    require_evolution_scene(pack)?;
    require_poke_ball(pack)?;
    require_battle_interface(pack)?;
    require_naming_screen(pack)?;
    require_trainer_front(pack)?;
    require_trainer_battlers(pack)
}
