//! Extracts the door animations (upstream `graphics/door_anims/` and
//! `sDoorAnimGraphicsTable`, `src/field_door.c`).
//!
//! The manifest is fixed here, so a file it does not name cannot change the
//! pack and a missing one fails extraction. It states the table's meaning
//! rather than its source: each door metatile with its sound, size class,
//! palette banks, and the sheet it animates with. Each referenced `.png` is
//! the sheet the ROM build turns into `.4bpp` tiles, and three of them are
//! shared (the Battle Tent doors), so there are fewer sheets than doors.
//!
//! An 8-bit PNG keeps the low nibble of each index, as `gbagfx` does for its
//! 4bpp sheet: the pack publishes every sheet at 4 bits, with the whole
//! raster in row-major order. `unused_top.png` and `unused_bottom.png` are
//! outside the manifest: no table row names them (`src/field_door.c:46-50`),
//! so no metatile reaches them. The `sDoorAnimPalettes_Unused` array is
//! unreferenced for the same reason.
//!
//! The table's bytes are written by [`assets::DoorAnimationTable`]. Ids are
//! `door-animation/image/<stem>` and `door-animation/table`.

use std::path::{Path, PathBuf};

use assets::{DoorAnimationRecord, DoorAnimationTable, DoorSize, DoorSound};
use pack_format::{PackEntry, PackWriter};

use super::{build_image_entry, png, push_raw_bytes, read_file, ExtractError};

const DIRECTORY: &str = "graphics/door_anims";
pub(crate) const TABLE_ID: &str = "door-animation/table";
/// Keeps the low nibble `gbagfx` keeps when it narrows an 8-bit index.
const INDEX_MASK: u8 = 0x0F;
const BIT_DEPTH: u8 = 4;

/// A sheet's source PNG: its height and bit depth, since its width follows
/// the size class of the doors that use it.
struct Sheet {
    stem: &'static str,
    height: u32,
    source_depth: u8,
}

const fn sheet(stem: &'static str, height: u32, source_depth: u8) -> Sheet {
    Sheet {
        stem,
        height,
        source_depth,
    }
}

/// One animating door metatile.
struct Door {
    metatile: u16,
    sound: DoorSound,
    size: DoorSize,
    sheet: &'static str,
    palette_slots: [u8; 8],
}

const fn door(
    metatile: u16,
    sound: DoorSound,
    size: DoorSize,
    sheet: &'static str,
    palette_slots: [u8; 8],
) -> Door {
    Door {
        metatile,
        sound,
        size,
        sheet,
        palette_slots,
    }
}

/// Every referenced sheet, in the order the table first names it.
const SHEETS: [Sheet; 51] = [
    sheet("general", 96, 4),
    sheet("poke_center", 96, 4),
    sheet("gym", 96, 4),
    sheet("poke_mart", 96, 8),
    sheet("littleroot", 96, 8),
    sheet("birchs_lab", 96, 4),
    sheet("rustboro_tan", 96, 4),
    sheet("rustboro_gray", 96, 4),
    sheet("fallarbor_light_roof", 96, 4),
    sheet("oldale", 96, 8),
    sheet("mauville", 96, 4),
    sheet("verdanturf", 96, 8),
    sheet("slateport", 96, 8),
    sheet("dewford", 96, 8),
    sheet("contest", 96, 4),
    sheet("lilycove", 96, 4),
    sheet("lilycove_wooden", 96, 4),
    sheet("mossdeep", 96, 8),
    sheet("sootopolis_peaked_roof", 96, 4),
    sheet("sootopolis", 96, 4),
    sheet("pokemon_league", 96, 4),
    sheet("pacifidlog", 96, 4),
    sheet("petalburg_gym", 96, 4),
    sheet("cycling_road", 96, 4),
    sheet("lilycove_dept_store", 96, 4),
    sheet("safari_zone", 96, 4),
    sheet("mossdeep_space_center", 96, 4),
    sheet("cable_club", 96, 4),
    sheet("abandoned_ship", 96, 4),
    sheet("fallarbor_dark_roof", 96, 8),
    sheet("abandoned_ship_room", 96, 4),
    sheet("lilycove_dept_store_elevator", 96, 8),
    sheet("battle_tower_old", 96, 4),
    sheet("battle_tower_elevator", 96, 4),
    sheet("unused_battle_frontier", 96, 4),
    sheet("battle_dome", 96, 4),
    sheet("battle_factory", 96, 4),
    sheet("battle_tower", 96, 4),
    sheet("battle_arena", 96, 4),
    sheet("battle_arena_lobby", 96, 4),
    sheet("battle_dome_lobby", 96, 4),
    sheet("battle_palace_lobby", 96, 4),
    sheet("battle_tent", 128, 4),
    sheet("battle_dome_corridor", 96, 4),
    sheet("battle_tower_multi_corridor", 96, 4),
    sheet("battle_frontier", 96, 4),
    sheet("battle_frontier_sliding", 96, 4),
    sheet("battle_dome_pre_battle_room", 96, 8),
    sheet("battle_tent_interior", 128, 4),
    sheet("trainer_hill_lobby_elevator", 96, 4),
    sheet("trainer_hill_roof_elevator", 96, 8),
];

/// The table in upstream's order, each row with the metatile label it
/// translates (`include/constants/metatile_labels.h`).
const DOORS: [Door; 53] = [
    door(
        0x021,
        DoorSound::Normal,
        DoorSize::OneTile,
        "general",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_General_Door
    door(
        0x061,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "poke_center",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_General_Door_PokeCenter
    door(
        0x1CD,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "gym",
        [5, 5, 5, 5, 5, 5, 5, 5],
    ), // METATILE_General_Door_Gym
    door(
        0x041,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "poke_mart",
        [0, 0, 1, 1, 1, 1, 1, 1],
    ), // METATILE_General_Door_PokeMart
    door(
        0x248,
        DoorSound::Normal,
        DoorSize::OneTile,
        "littleroot",
        [10, 10, 6, 6, 6, 6, 6, 6],
    ), // METATILE_Petalburg_Door_Littleroot
    door(
        0x249,
        DoorSound::Normal,
        DoorSize::OneTile,
        "birchs_lab",
        [8, 8, 8, 8, 8, 8, 8, 8],
    ), // METATILE_Petalburg_Door_BirchsLab
    door(
        0x22F,
        DoorSound::Normal,
        DoorSize::OneTile,
        "rustboro_tan",
        [11, 11, 11, 11, 11, 11, 11, 11],
    ), // METATILE_Rustboro_Door_Tan
    door(
        0x21F,
        DoorSound::Normal,
        DoorSize::OneTile,
        "rustboro_gray",
        [10, 10, 10, 10, 10, 10, 10, 10],
    ), // METATILE_Rustboro_Door_Gray
    door(
        0x2A5,
        DoorSound::Normal,
        DoorSize::OneTile,
        "fallarbor_light_roof",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_Fallarbor_Door_LightRoof
    door(
        0x287,
        DoorSound::Normal,
        DoorSize::OneTile,
        "oldale",
        [10, 10, 9, 9, 9, 9, 9, 9],
    ), // METATILE_Petalburg_Door_Oldale
    door(
        0x2AC,
        DoorSound::Normal,
        DoorSize::OneTile,
        "mauville",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_Mauville_Door
    door(
        0x3A1,
        DoorSound::Normal,
        DoorSize::OneTile,
        "verdanturf",
        [6, 6, 5, 5, 5, 5, 5, 5],
    ), // METATILE_Mauville_Door_Verdanturf
    door(
        0x2DC,
        DoorSound::Normal,
        DoorSize::OneTile,
        "slateport",
        [6, 6, 1, 1, 1, 1, 1, 1],
    ), // METATILE_Slateport_Door
    door(
        0x225,
        DoorSound::Normal,
        DoorSize::OneTile,
        "dewford",
        [0, 0, 5, 5, 5, 5, 5, 5],
    ), // METATILE_Dewford_Door
    door(
        0x1DB,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "contest",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_General_Door_Contest
    door(
        0x246,
        DoorSound::Normal,
        DoorSize::OneTile,
        "lilycove",
        [8, 8, 8, 8, 8, 8, 8, 8],
    ), // METATILE_Lilycove_Door
    door(
        0x28E,
        DoorSound::Normal,
        DoorSize::OneTile,
        "lilycove_wooden",
        [5, 5, 5, 5, 5, 5, 5, 5],
    ), // METATILE_Lilycove_Door_Wooden
    door(
        0x2A1,
        DoorSound::Normal,
        DoorSize::OneTile,
        "mossdeep",
        [9, 9, 1, 1, 1, 1, 1, 1],
    ), // METATILE_Mossdeep_Door
    door(
        0x21C,
        DoorSound::Normal,
        DoorSize::OneTile,
        "sootopolis_peaked_roof",
        [6, 6, 6, 6, 6, 6, 6, 6],
    ), // METATILE_Sootopolis_Door_PeakedRoof
    door(
        0x21E,
        DoorSound::Normal,
        DoorSize::OneTile,
        "sootopolis",
        [6, 6, 6, 6, 6, 6, 6, 6],
    ), // METATILE_Sootopolis_Door
    door(
        0x21D,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "pokemon_league",
        [8, 8, 8, 8, 8, 8, 8, 8],
    ), // METATILE_EverGrande_Door_PokemonLeague
    door(
        0x21A,
        DoorSound::Normal,
        DoorSize::OneTile,
        "pacifidlog",
        [9, 9, 9, 9, 9, 9, 9, 9],
    ), // METATILE_Pacifidlog_Door
    door(
        0x224,
        DoorSound::Normal,
        DoorSize::OneTile,
        "petalburg_gym",
        [6, 6, 6, 6, 6, 6, 6, 6],
    ), // METATILE_PetalburgGym_Door
    door(
        0x289,
        DoorSound::Normal,
        DoorSize::OneTile,
        "cycling_road",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_Mauville_Door_CyclingRoad
    door(
        0x30C,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "lilycove_dept_store",
        [5, 5, 5, 5, 5, 5, 5, 5],
    ), // METATILE_Lilycove_Door_DeptStore
    door(
        0x32D,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "safari_zone",
        [9, 9, 9, 9, 9, 9, 9, 9],
    ), // METATILE_Lilycove_Door_SafariZone
    door(
        0x2ED,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "mossdeep_space_center",
        [8, 8, 8, 8, 8, 8, 8, 8],
    ), // METATILE_Mossdeep_Door_SpaceCenter
    door(
        0x264,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "cable_club",
        [6, 6, 6, 6, 6, 6, 6, 6],
    ), // METATILE_PokemonCenter_Door_CableClub
    door(
        0x22B,
        DoorSound::Normal,
        DoorSize::OneTile,
        "abandoned_ship",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_InsideShip_IntactDoor_Bottom_Unlocked
    door(
        0x2F7,
        DoorSound::Normal,
        DoorSize::OneTile,
        "fallarbor_dark_roof",
        [11, 11, 7, 7, 7, 7, 7, 7],
    ), // METATILE_Fallarbor_Door_DarkRoof
    door(
        0x297,
        DoorSound::Normal,
        DoorSize::OneTile,
        "abandoned_ship_room",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_InsideShip_IntactDoor_Bottom_Interior
    door(
        0x285,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "lilycove_dept_store_elevator",
        [6, 6, 7, 7, 7, 7, 7, 7],
    ), // METATILE_Shop_Door_Elevator
    door(
        0x25D,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_tower_old",
        [9, 9, 9, 9, 9, 9, 9, 9],
    ), // METATILE_Dewford_Door_BattleTower
    door(
        0x20E,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_tower_elevator",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_BattleFrontier_Door_Elevator
    door(
        0x3B0,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "unused_battle_frontier",
        [9, 9, 9, 9, 9, 9, 9, 9],
    ), // unlabelled upstream
    door(
        0x28A,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_dome",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_BattleFrontierOutsideWest_Door_BattleDome
    door(
        0x263,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_factory",
        [9, 9, 9, 9, 9, 9, 9, 9],
    ), // METATILE_BattleFrontierOutsideWest_Door_BattleFactory
    door(
        0x329,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_tower",
        [0, 0, 0, 0, 0, 0, 0, 0],
    ), // METATILE_BattleFrontierOutsideEast_Door_BattleTower
    door(
        0x291,
        DoorSound::Normal,
        DoorSize::OneTile,
        "battle_arena",
        [5, 5, 5, 5, 5, 5, 5, 5],
    ), // METATILE_BattleFrontierOutsideEast_Door_BattleArena
    door(
        0x21B,
        DoorSound::Arena,
        DoorSize::OneTile,
        "battle_arena_lobby",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_BattleArena_Door
    door(
        0x209,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_dome_lobby",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_BattleDome_Door_Lobby
    door(
        0x219,
        DoorSound::Normal,
        DoorSize::OneTile,
        "battle_palace_lobby",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_BattlePalace_Door
    door(
        0x393,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_tent",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_Slateport_Door_BattleTent
    door(
        0x3D4,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_tent",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_Mauville_Door_BattleTent
    door(
        0x36C,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_tent",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_Fallarbor_Door_BattleTent
    door(
        0x25E,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_dome_corridor",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_BattleDome_Door_Corridor
    door(
        0x2AD,
        DoorSound::Sliding,
        DoorSize::Large,
        "battle_tower_multi_corridor",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_BattleFrontier_Door_MultiCorridor
    door(
        0x3FC,
        DoorSound::Normal,
        DoorSize::OneTile,
        "battle_frontier",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_BattleFrontierOutsideWest_Door
    door(
        0x396,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_frontier_sliding",
        [1, 1, 1, 1, 1, 1, 1, 1],
    ), // METATILE_BattleFrontierOutsideWest_Door_Sliding
    door(
        0x20A,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_dome_pre_battle_room",
        [9, 9, 7, 7, 7, 7, 7, 7],
    ), // METATILE_BattleDome_Door_PreBattleRoom
    door(
        0x26B,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "battle_tent_interior",
        [9, 9, 9, 9, 9, 9, 9, 9],
    ), // METATILE_BattleTent_Door
    door(
        0x32C,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "trainer_hill_lobby_elevator",
        [7, 7, 7, 7, 7, 7, 7, 7],
    ), // METATILE_TrainerHill_Door_Elevator_Lobby
    door(
        0x383,
        DoorSound::Sliding,
        DoorSize::OneTile,
        "trainer_hill_roof_elevator",
        [9, 9, 7, 7, 7, 7, 7, 7],
    ), // METATILE_TrainerHill_Door_Elevator_Roof
];

/// Every pack id [`extract_door_animation`] emits. The profile generator reads
/// this as its fixed inventory so extraction and generation cannot drift apart.
pub(crate) fn pack_ids() -> Vec<String> {
    SHEETS
        .iter()
        .map(|sheet| image_id(sheet.stem))
        .chain([TABLE_ID.to_owned()])
        .collect()
}

fn image_id(stem: &str) -> String {
    format!("door-animation/image/{stem}")
}

/// The records the table entry carries, in upstream's order.
pub(crate) fn records() -> Vec<DoorAnimationRecord> {
    DOORS
        .iter()
        .map(|door| DoorAnimationRecord {
            metatile: door.metatile,
            sound: door.sound,
            size: door.size,
            palette_slots: door.palette_slots,
            sheet: door.sheet.to_owned(),
        })
        .collect()
}

/// Extracts every door sheet and the door table.
///
/// Every entry is validated before any is queued, so a failure leaves the
/// writer untouched.
pub(super) fn extract_door_animation(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let dir = upstream.join(DIRECTORY);
    let mut entries = Vec::with_capacity(SHEETS.len());
    for sheet in &SHEETS {
        entries.push(extract_sheet(&dir, sheet)?);
    }
    let table = DoorAnimationTable::new(records())
        .map_err(|error| mismatch(&dir, format!("builds a door table that {error}")))?;
    for entry in entries {
        writer.push(entry);
    }
    push_raw_bytes(TABLE_ID.to_owned(), table.encode(), writer);
    Ok(())
}

fn extract_sheet(dir: &Path, sheet: &Sheet) -> Result<PackEntry, ExtractError> {
    let path = dir.join(format!("{}.png", sheet.stem));
    let bytes = read_file(&path)?;
    let mut image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    let width = sheet_width(sheet.stem);
    if (image.width, image.height) != (width, sheet.height) {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {width}x{}",
                image.width, image.height, sheet.height
            ),
        ));
    }
    if image.bit_depth != sheet.source_depth {
        return Err(mismatch(
            &path,
            format!(
                "is {}-bit: expected {}-bit",
                image.bit_depth, sheet.source_depth
            ),
        ));
    }
    for pixel in &mut image.pixels {
        *pixel &= INDEX_MASK;
    }
    image.bit_depth = BIT_DEPTH;
    build_image_entry(&path, image_id(sheet.stem), image)
}

/// The width of `stem`'s sheet: its size class's. Every door sharing a sheet
/// has the same class.
fn sheet_width(stem: &str) -> u32 {
    DOORS
        .iter()
        .find(|door| door.sheet == stem)
        .map_or(0, |door| door.size.sheet_width())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::DoorAnimationAssetMismatch {
        path: PathBuf::from(path),
        detail,
    }
}

#[cfg(test)]
mod tests;
