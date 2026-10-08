//! Renders stationary object events backed by supported 16x32 people sprites.
//!
//! [`resolve_sprite_source`] is the complete graphics-id binding table. Hidden
//! events are filtered through [`visible_object_events`], the same visibility
//! used for interaction. A visible unsupported event remains interaction-ready
//! but produces no [`OamEntry`]. `OBJ_EVENT_GFX_VAR_0` binds only when its
//! event variable contains one of the two Route 103 rival ids; decoration slots
//! remain unsupported. A decoration slice that clears a `FLAG_DECORATION_n`
//! hide flag must first write that slot's `VAR_OBJ_GFX_ID_0 + n`, or the
//! persistent rival id resolves the decoration to a rival sprite.
//!
//! Object events retain their initial movement-facing frame. Their positions
//! share [`super::viewport::camera_lag_px`] with the background during a step.
//!
//! Upstream resolves equal-priority overlap with a y-derived subpriority
//! (`SetObjectSubpriorityByElevation`, `event_object_movement.c:7773-7779`).
//! [`rendering::SpriteLayer`] has no subpriority, so [`order_by_depth`]
//! reproduces it by giving the lower-on-screen object the lower OAM index.

use std::collections::HashMap;

use assets::pack::{AssetPack, PaletteRef};
use assets::ObjectEvent;
use engine::event_data::EventData;
use engine::overworld::{
    initial_facing_direction, object_event_is_in_view, visible_object_events, PlayerState,
};
use rendering::{Bgr555, BitDepth, OamEntry, Palette};

use super::avatar::{self, PlayerCharacter};
use super::{fill_palette_bank, OverworldSceneError, METATILE_PX};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NpcPaletteTag {
    Npc1,
    Npc2,
    Npc3,
    Npc4,
}

const PLAYER_PALETTE_BANK: u8 = 0;

/// Separate because upstream gives rivals `PALSLOT_NPC_SPECIAL`, not the
/// player's `PALSLOT_PLAYER` (`object_event_graphics_info.h:1-18,1920-2032`).
const OTHER_PROTAGONIST_BANK: u8 = 5;

const TILE_BYTES: usize = 32;

impl NpcPaletteTag {
    const fn bank(self) -> u8 {
        match self {
            Self::Npc1 => 1,
            Self::Npc2 => 2,
            Self::Npc3 => 3,
            Self::Npc4 => 4,
        }
    }

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Npc1 => "npc_1",
            Self::Npc2 => "npc_2",
            Self::Npc3 => "npc_3",
            Self::Npc4 => "npc_4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NpcSpriteSource {
    /// Reuses the player sheet and palette already loaded by the scene.
    PlayerCharacter,
    People16x32 {
        sprite_path: &'static str,
        palette_bank: u8,
    },
}

/// `VAR_OBJ_GFX_ID_0` (`include/constants/vars.h:32`).
const VAR_OBJ_GFX_ID_0: u16 = 0x4010;

/// `OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL` (`event_objects.h:107`).
const RIVAL_BRENDAN_NORMAL_GFX_ID: u16 = 100;

/// `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL` (`event_objects.h:112`).
const RIVAL_MAY_NORMAL_GFX_ID: u16 = 105;

fn resolve_sprite_source(
    graphics_id: &str,
    player: PlayerCharacter,
    event_data: &EventData,
) -> Option<NpcSpriteSource> {
    use NpcPaletteTag::{Npc1, Npc2, Npc3, Npc4};
    use NpcSpriteSource::People16x32;

    match graphics_id {
        "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL" => {
            Some(protagonist_source(PlayerCharacter::Brendan, player))
        }
        "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL" => Some(protagonist_source(PlayerCharacter::May, player)),
        "OBJ_EVENT_GFX_VAR_0" => match event_data.var_get(VAR_OBJ_GFX_ID_0).unwrap_or(0) {
            id if id == RIVAL_BRENDAN_NORMAL_GFX_ID => {
                Some(protagonist_source(PlayerCharacter::Brendan, player))
            }
            id if id == RIVAL_MAY_NORMAL_GFX_ID => {
                Some(protagonist_source(PlayerCharacter::May, player))
            }
            _ => None,
        },
        "OBJ_EVENT_GFX_MOM" => Some(People16x32 {
            sprite_path: "mom",
            palette_bank: Npc4.bank(),
        }),
        "OBJ_EVENT_GFX_TWIN" => Some(People16x32 {
            sprite_path: "twin",
            palette_bank: Npc2.bank(),
        }),
        "OBJ_EVENT_GFX_FAT_MAN" => Some(People16x32 {
            sprite_path: "fat_man",
            palette_bank: Npc1.bank(),
        }),
        "OBJ_EVENT_GFX_BOY_2" => Some(People16x32 {
            sprite_path: "boy_2",
            palette_bank: Npc1.bank(),
        }),
        "OBJ_EVENT_GFX_PROF_BIRCH" => Some(People16x32 {
            sprite_path: "prof_birch",
            palette_bank: Npc3.bank(),
        }),
        "OBJ_EVENT_GFX_WOMAN_4" => Some(People16x32 {
            sprite_path: "woman_4",
            palette_bank: Npc1.bank(),
        }),
        "OBJ_EVENT_GFX_NORMAN" => Some(People16x32 {
            sprite_path: "gym_leaders/norman",
            palette_bank: Npc4.bank(),
        }),
        "OBJ_EVENT_GFX_SCIENTIST_1" => Some(People16x32 {
            sprite_path: "scientist_1",
            palette_bank: Npc3.bank(),
        }),
        "OBJ_EVENT_GFX_MART_EMPLOYEE" => Some(People16x32 {
            sprite_path: "mart_employee",
            palette_bank: Npc1.bank(),
        }),
        "OBJ_EVENT_GFX_GIRL_3" => Some(People16x32 {
            sprite_path: "girl_3",
            palette_bank: Npc2.bank(),
        }),
        "OBJ_EVENT_GFX_MANIAC" => Some(People16x32 {
            sprite_path: "maniac",
            palette_bank: Npc4.bank(),
        }),
        "OBJ_EVENT_GFX_MAN_3" => Some(People16x32 {
            sprite_path: "man_3",
            palette_bank: Npc2.bank(),
        }),
        "OBJ_EVENT_GFX_WOMAN_2" => Some(People16x32 {
            sprite_path: "woman_2",
            palette_bank: Npc3.bank(),
        }),
        "OBJ_EVENT_GFX_BOY_1" => Some(People16x32 {
            sprite_path: "boy_1",
            palette_bank: Npc3.bank(),
        }),
        "OBJ_EVENT_GFX_POKEFAN_M" => Some(People16x32 {
            sprite_path: "pokefan_m",
            palette_bank: Npc2.bank(),
        }),
        "OBJ_EVENT_GFX_BLACK_BELT" => Some(People16x32 {
            sprite_path: "black_belt",
            palette_bank: Npc3.bank(),
        }),
        "OBJ_EVENT_GFX_MAN_5" => Some(People16x32 {
            sprite_path: "man_5",
            palette_bank: Npc2.bank(),
        }),
        "OBJ_EVENT_GFX_SWIMMER_F" => Some(People16x32 {
            sprite_path: "swimmer_f",
            palette_bank: Npc2.bank(),
        }),
        "OBJ_EVENT_GFX_SWIMMER_M" => Some(People16x32 {
            sprite_path: "swimmer_m",
            palette_bank: Npc1.bank(),
        }),
        "OBJ_EVENT_GFX_FISHERMAN" => Some(People16x32 {
            sprite_path: "fisherman",
            palette_bank: Npc2.bank(),
        }),
        _ => None,
    }
}

/// Reuses the loaded player assets only when `who` is the current player.
/// Rival graphics use the corresponding protagonist's walking assets upstream
/// (`object_event_graphics_info.h:1920-2032`).
fn protagonist_source(who: PlayerCharacter, player: PlayerCharacter) -> NpcSpriteSource {
    if who == player {
        NpcSpriteSource::PlayerCharacter
    } else {
        NpcSpriteSource::People16x32 {
            sprite_path: who.sprite_path(),
            palette_bank: OTHER_PROTAGONIST_BANK,
        }
    }
}

/// Tile and palette location for a resolved graphics id.
#[derive(Debug, Clone, Copy)]
pub(super) struct SpriteBinding {
    base_tile: u16,
    palette_bank: u8,
}

#[cfg(test)]
impl SpriteBinding {
    /// Returns the first tile in this sprite's frame block.
    pub(super) const fn base_tile(self) -> u16 {
        self.base_tile
    }

    /// Returns this sprite's palette bank.
    pub(super) const fn palette_bank(self) -> u8 {
        self.palette_bank
    }
}

/// Resolves distinct object graphics and appends their packed frames after
/// the player frames already in `sprite_bytes`.
///
/// # Errors
///
/// Returns an asset, sprite-dimension, pixel-count, or tile-alignment error
/// when a resolved sheet cannot be loaded and packed.
pub(super) fn resolve_bindings(
    pack: &AssetPack,
    player: PlayerCharacter,
    object_events: &'static [ObjectEvent],
    sprite_bytes: &mut Vec<u8>,
    event_data: &EventData,
) -> Result<HashMap<&'static str, SpriteBinding>, OverworldSceneError> {
    let mut bindings = HashMap::new();
    for event in object_events {
        if bindings.contains_key(event.graphics_id) {
            continue;
        }
        match resolve_sprite_source(event.graphics_id, player, event_data) {
            Some(NpcSpriteSource::PlayerCharacter) => {
                bindings.insert(
                    event.graphics_id,
                    SpriteBinding {
                        base_tile: 0,
                        palette_bank: PLAYER_PALETTE_BANK,
                    },
                );
            }
            Some(NpcSpriteSource::People16x32 {
                sprite_path,
                palette_bank,
            }) => {
                let base_bytes = sprite_bytes.len();
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "a scene contains only a small number of NPC sheets"
                )]
                let base_tile = (base_bytes / TILE_BYTES) as u16;
                debug_assert!(
                    base_tile.is_multiple_of(avatar::FRAME_BLOCK_TILES),
                    "each packed people sheet must occupy a whole frame block"
                );
                let image = pack.sprite(sprite_path)?;
                sprite_bytes.extend(avatar::pack_people_sheet_frames(sprite_path, image)?);
                bindings.insert(
                    event.graphics_id,
                    SpriteBinding {
                        base_tile,
                        palette_bank,
                    },
                );
            }
            None => {}
        }
    }
    Ok(bindings)
}

/// Loads the player, four generic NPC, and other-protagonist palette banks.
///
/// # Errors
///
/// Returns [`OverworldSceneError::Pack`] when a required palette is missing.
pub(super) fn build_combined_palette(
    pack: &AssetPack,
    player: PlayerCharacter,
    player_bank0: PaletteRef<'_>,
) -> Result<Palette, OverworldSceneError> {
    let mut colors = [Bgr555::default(); Palette::LEN];
    fill_palette_bank(&mut colors, usize::from(PLAYER_PALETTE_BANK), player_bank0);
    for tag in [
        NpcPaletteTag::Npc1,
        NpcPaletteTag::Npc2,
        NpcPaletteTag::Npc3,
        NpcPaletteTag::Npc4,
    ] {
        let raw = pack.sprite_palette(tag.pack_name())?;
        fill_palette_bank(&mut colors, usize::from(tag.bank()), raw);
    }
    let other = pack.sprite_palette(player.other().palette_name())?;
    fill_palette_bank(&mut colors, usize::from(OTHER_PROTAGONIST_BANK), other);
    Ok(Palette::new(colors))
}

const OAM_X_MODULUS: i32 = 512;
const OAM_Y_MODULUS: i32 = 256;

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "rem_euclid bounds the result to the nine-bit OAM x range"
)]
fn wrap_oam_x(x: i32) -> u16 {
    x.rem_euclid(OAM_X_MODULUS) as u16
}

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "rem_euclid bounds the result to the eight-bit OAM y range"
)]
fn wrap_oam_y(y: i32) -> u8 {
    y.rem_euclid(OAM_Y_MODULUS) as u8
}

fn object_screen_position(
    object_pos: (i32, i32),
    player_pos: (i32, i32),
    camera_lag: (i32, i32),
) -> (u16, u8) {
    let dx = (object_pos.0 - player_pos.0) * METATILE_PX + camera_lag.0;
    let dy = (object_pos.1 - player_pos.1) * METATILE_PX + camera_lag.1;
    let x = i32::from(avatar::PLAYER_OBJ_X) + dx;
    let y = i32::from(avatar::PLAYER_OBJ_Y) + dy;
    (wrap_oam_x(x), wrap_oam_y(y))
}

/// Height of every bound object event's 16x32 OBJ.
const SPRITE_H_PX: u16 = 32;

/// `sElevationToSubpriority` (`event_object_movement.c:7691-7693`). Elevations
/// sharing an OBJ priority still differ here, so it is part of the draw order.
const ELEVATION_TO_SUBPRIORITY: [u16; 16] = [
    115, 115, 83, 115, 83, 115, 83, 115, 83, 115, 83, 115, 83, 0, 0, 115,
];

/// Upstream's subpriority for an OBJ at `elevation` whose rendered top is `y`:
/// `sElevationToSubpriority[elevation] + (16 - (((y + 8) & 0xFF) >> 4)) * 2 + 1`
/// (`SetObjectSubpriorityByElevation`, `:7739-7745`, called with subpriority 1),
/// where upstream's `y` is the sprite bottom, so the 32px height is added.
fn subpriority(entry: OamEntry, elevation: u8) -> u16 {
    let bottom_plus_bias = (u16::from(entry.y()) + SPRITE_H_PX + 8) & 0xFF;
    let base = ELEVATION_TO_SUBPRIORITY
        .get(usize::from(elevation))
        .copied()
        .unwrap_or(ELEVATION_TO_SUBPRIORITY[0]);
    base + (16 - (bottom_plus_bias >> 4)) * 2 + 1
}

/// Orders pairs as upstream emits OAM: ascending OBJ priority, then subpriority,
/// then lower on-screen `y` (`SortSprites`, `sprite.c:413-419`); ties keep input order.
pub(super) fn order_by_depth(entries: &[(OamEntry, u8)]) -> Vec<OamEntry> {
    let mut ordered = entries.to_vec();
    ordered.sort_by_key(|&(entry, elevation)| {
        (
            entry.priority(),
            subpriority(entry, elevation),
            std::cmp::Reverse(entry.y()),
        )
    });
    ordered.into_iter().map(|(entry, _)| entry).collect()
}

/// Builds OAM entries for visible events with resolved sprite bindings.
#[cfg(test)]
#[must_use]
pub(super) fn oam_entries(
    object_events: &'static [ObjectEvent],
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
    event_data: &EventData,
) -> Vec<OamEntry> {
    elevated_oam_entries(object_events, bindings, player, event_data)
        .into_iter()
        .map(|(entry, _)| entry)
        .collect()
}

/// [`oam_entries`] with each entry's authored elevation, which selects its
/// upstream subpriority base.
#[must_use]
pub(super) fn elevated_oam_entries(
    object_events: &'static [ObjectEvent],
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
    event_data: &EventData,
) -> Vec<(OamEntry, u8)> {
    let player_pos = player.position();
    let camera_lag = super::viewport::camera_lag_px(player);
    visible_object_events_with_binding(object_events, bindings, event_data)
        .filter(|(event, _)| object_event_is_in_view(event, player_pos))
        .map(|(event, binding)| {
            let facing = initial_facing_direction(event.movement_type);
            let (frame, h_flip) = avatar::stand_frame_for(facing);
            let tile_index = binding.base_tile + frame * avatar::FRAME_TILES;
            let (x, y) = object_screen_position(
                (i32::from(event.x), i32::from(event.y)),
                player_pos,
                camera_lag,
            );
            let entry = OamEntry::new(
                x,
                y,
                tile_index,
                binding.palette_bank,
                BitDepth::Bpp4,
                h_flip,
                false,
                avatar::PLAYER_OBJ_SHAPE,
                avatar::PLAYER_OBJ_SIZE,
                priority_for_stationary_object(event),
                true,
            );
            (entry, event.elevation)
        })
        .collect()
}

/// Uses authored elevation because the sprite resources do not retain the map
/// grid. Upstream replaces it with the grid elevation on spawn
/// (`event_object_movement.c:7737-7771`); the only bundled priority-changing
/// mismatch belongs to an unsupported Pichu doll.
fn priority_for_stationary_object(event: &ObjectEvent) -> u8 {
    avatar::priority_for_elevation(event.elevation)
}

fn visible_object_events_with_binding<'a>(
    object_events: &'a [ObjectEvent],
    bindings: &'a HashMap<&'static str, SpriteBinding>,
    event_data: &'a EventData,
) -> impl Iterator<Item = (&'a ObjectEvent, &'a SpriteBinding)> {
    visible_object_events(object_events, event_data).filter_map(move |event| {
        bindings
            .get(event.graphics_id)
            .map(|binding| (event, binding))
    })
}

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod draw_order_tests;
#[cfg(test)]
mod placement_tests;
#[cfg(test)]
mod test_support;
