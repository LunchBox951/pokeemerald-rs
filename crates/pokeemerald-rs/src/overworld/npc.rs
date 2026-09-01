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
//! [`rendering::SpriteLayer`] has no subpriority, so its lower OAM index wins
//! and the player draws in front of every same-priority NPC.

use std::collections::HashMap;

use assets::pack::{AssetPack, PaletteRef};
use assets::ObjectEvent;
use engine::event_data::EventData;
use engine::overworld::{
    initial_facing_direction, object_event_is_in_view, visible_object_events, PlayerState,
};
use rendering::{Bgr555, BitDepth, OamEntry, Palette};

use super::avatar::{self, PlayerCharacter};
use super::{OverworldSceneError, METATILE_PX};

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
    avatar::fill_palette_bank(&mut colors, usize::from(PLAYER_PALETTE_BANK), player_bank0);
    for tag in [
        NpcPaletteTag::Npc1,
        NpcPaletteTag::Npc2,
        NpcPaletteTag::Npc3,
        NpcPaletteTag::Npc4,
    ] {
        let raw = pack.sprite_palette(tag.pack_name())?;
        avatar::fill_palette_bank(&mut colors, usize::from(tag.bank()), raw);
    }
    let other = pack.sprite_palette(player.other().palette_name())?;
    avatar::fill_palette_bank(&mut colors, usize::from(OTHER_PROTAGONIST_BANK), other);
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

/// Builds OAM entries for visible events with resolved sprite bindings.
#[must_use]
pub(super) fn oam_entries(
    object_events: &'static [ObjectEvent],
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
    event_data: &EventData,
) -> Vec<OamEntry> {
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
            OamEntry::new(
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
            )
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
mod tests {
    use super::*;
    use assets::{MovementType, TrainerType};
    use engine::overworld::Direction;

    const DEFAULT_ELEVATION: u8 = 3;
    const RAISED_ELEVATION: u8 = 4;
    const RAISED_PRIORITY: u8 = 1;
    const HIDE_BRENDAN_BEDROOM_RIVAL: u16 = 0x2F8;
    const NON_RIVAL_GFX_ID: u16 = 1;
    const EXPECTED_WALK_FRAMES_PER_TILE: u8 = 16;

    fn object(
        graphics_id: &'static str,
        x: i16,
        y: i16,
        movement_type: MovementType,
    ) -> ObjectEvent {
        ObjectEvent {
            local_id: 1,
            graphics_id,
            x,
            y,
            elevation: DEFAULT_ELEVATION,
            movement_type,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag: "0",
        }
    }

    #[test]
    fn both_rival_variants_resolve_for_either_player_gender() {
        let no_flags = EventData::new();
        assert_eq!(
            resolve_sprite_source(
                "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL",
                PlayerCharacter::Brendan,
                &no_flags
            ),
            Some(NpcSpriteSource::People16x32 {
                sprite_path: "may/walking",
                palette_bank: OTHER_PROTAGONIST_BANK,
            }),
            "the rival of a Brendan player is May, drawn from May's sheet"
        );
        assert_eq!(
            resolve_sprite_source(
                "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
                PlayerCharacter::May,
                &no_flags
            ),
            Some(NpcSpriteSource::People16x32 {
                sprite_path: "brendan/walking",
                palette_bank: OTHER_PROTAGONIST_BANK,
            }),
            "the rival of a May player is Brendan, drawn from Brendan's sheet"
        );
        assert_eq!(
            resolve_sprite_source(
                "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
                PlayerCharacter::Brendan,
                &no_flags
            ),
            Some(NpcSpriteSource::PlayerCharacter)
        );
        assert_eq!(
            resolve_sprite_source(
                "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL",
                PlayerCharacter::May,
                &no_flags
            ),
            Some(NpcSpriteSource::PlayerCharacter)
        );
    }

    #[test]
    fn var_0_resolves_to_a_rival_only_when_the_var_holds_a_real_rival_gfx_id() {
        let mut event_data = EventData::new();

        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan, &event_data),
            None
        );

        event_data
            .var_set(VAR_OBJ_GFX_ID_0, RIVAL_MAY_NORMAL_GFX_ID)
            .unwrap();
        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan, &event_data),
            Some(NpcSpriteSource::People16x32 {
                sprite_path: "may/walking",
                palette_bank: OTHER_PROTAGONIST_BANK,
            }),
            "a male player's Route 103 rival is May"
        );

        event_data
            .var_set(VAR_OBJ_GFX_ID_0, RIVAL_BRENDAN_NORMAL_GFX_ID)
            .unwrap();
        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::May, &event_data),
            Some(NpcSpriteSource::People16x32 {
                sprite_path: "brendan/walking",
                palette_bank: OTHER_PROTAGONIST_BANK,
            }),
            "a female player's Route 103 rival is Brendan"
        );

        event_data
            .var_set(VAR_OBJ_GFX_ID_0, NON_RIVAL_GFX_ID)
            .unwrap();
        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan, &event_data),
            None
        );
    }

    #[test]
    fn the_other_protagonist_palette_bank_collides_with_nothing() {
        assert_ne!(OTHER_PROTAGONIST_BANK, PLAYER_PALETTE_BANK);
        for tag in [
            NpcPaletteTag::Npc1,
            NpcPaletteTag::Npc2,
            NpcPaletteTag::Npc3,
            NpcPaletteTag::Npc4,
        ] {
            assert_ne!(OTHER_PROTAGONIST_BANK, tag.bank(), "{tag:?} bank clash");
        }
    }

    #[test]
    fn resolve_sprite_source_resolves_mom_to_the_npc4_palette() {
        let source = resolve_sprite_source(
            "OBJ_EVENT_GFX_MOM",
            PlayerCharacter::Brendan,
            &EventData::new(),
        )
        .unwrap();
        assert_eq!(
            source,
            NpcSpriteSource::People16x32 {
                sprite_path: "mom",
                palette_bank: NpcPaletteTag::Npc4.bank(),
            }
        );
    }

    #[test]
    fn resolve_sprite_source_resolves_the_oldale_and_route_103_background_npcs() {
        use NpcPaletteTag::{Npc1, Npc2, Npc3, Npc4};

        let no_flags = EventData::new();
        let cases = [
            ("OBJ_EVENT_GFX_MART_EMPLOYEE", "mart_employee", Npc1),
            ("OBJ_EVENT_GFX_GIRL_3", "girl_3", Npc2),
            ("OBJ_EVENT_GFX_MANIAC", "maniac", Npc4),
            ("OBJ_EVENT_GFX_MAN_3", "man_3", Npc2),
            ("OBJ_EVENT_GFX_WOMAN_2", "woman_2", Npc3),
            ("OBJ_EVENT_GFX_BOY_1", "boy_1", Npc3),
            ("OBJ_EVENT_GFX_POKEFAN_M", "pokefan_m", Npc2),
            ("OBJ_EVENT_GFX_BLACK_BELT", "black_belt", Npc3),
            ("OBJ_EVENT_GFX_MAN_5", "man_5", Npc2),
            ("OBJ_EVENT_GFX_SWIMMER_F", "swimmer_f", Npc2),
            ("OBJ_EVENT_GFX_SWIMMER_M", "swimmer_m", Npc1),
            ("OBJ_EVENT_GFX_FISHERMAN", "fisherman", Npc2),
        ];
        for (id, sprite_path, tag) in cases {
            assert_eq!(
                resolve_sprite_source(id, PlayerCharacter::Brendan, &no_flags),
                Some(NpcSpriteSource::People16x32 {
                    sprite_path,
                    palette_bank: tag.bank(),
                }),
                "{id}"
            );
        }
    }

    const EXTRACTED_MAPS: [&str; 9] = [
        "MAP_LITTLEROOT_TOWN",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        "MAP_ROUTE101",
        "MAP_OLDALE_TOWN",
        "MAP_ROUTE103",
    ];

    fn extracted_map_graphics_ids() -> Vec<&'static str> {
        let table = assets::MapEventsTable::new();
        let mut ids: Vec<&'static str> = Vec::new();
        for map in EXTRACTED_MAPS {
            let events = table.resolve(assets::MapId(map)).unwrap();
            for event in events.object_events {
                if !ids.contains(&event.graphics_id) {
                    ids.push(event.graphics_id);
                }
            }
        }
        ids.sort_unstable();
        ids
    }

    #[test]
    fn resolve_sprite_source_partitions_every_extracted_map_graphics_id() {
        const GRAPHICS_ID_COUNT: usize = 46;

        let reachable = extracted_map_graphics_ids();
        assert_eq!(
            reachable.len(),
            GRAPHICS_ID_COUNT,
            "the extracted map set changed"
        );

        let no_flags = EventData::new();
        let (drawn, not_drawn): (Vec<&'static str>, Vec<&'static str>) =
            reachable.iter().partition(|id| {
                resolve_sprite_source(id, PlayerCharacter::Brendan, &no_flags).is_some()
            });

        assert_eq!(
            drawn,
            [
                "OBJ_EVENT_GFX_BLACK_BELT",
                "OBJ_EVENT_GFX_BOY_1",
                "OBJ_EVENT_GFX_BOY_2",
                "OBJ_EVENT_GFX_FAT_MAN",
                "OBJ_EVENT_GFX_FISHERMAN",
                "OBJ_EVENT_GFX_GIRL_3",
                "OBJ_EVENT_GFX_MANIAC",
                "OBJ_EVENT_GFX_MAN_3",
                "OBJ_EVENT_GFX_MAN_5",
                "OBJ_EVENT_GFX_MART_EMPLOYEE",
                "OBJ_EVENT_GFX_MOM",
                "OBJ_EVENT_GFX_NORMAN",
                "OBJ_EVENT_GFX_POKEFAN_M",
                "OBJ_EVENT_GFX_PROF_BIRCH",
                "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
                "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL",
                "OBJ_EVENT_GFX_SCIENTIST_1",
                "OBJ_EVENT_GFX_SWIMMER_F",
                "OBJ_EVENT_GFX_SWIMMER_M",
                "OBJ_EVENT_GFX_TWIN",
                "OBJ_EVENT_GFX_WOMAN_2",
                "OBJ_EVENT_GFX_WOMAN_4",
            ],
            "the supported graphics-id set changed"
        );
        assert_eq!(
            not_drawn,
            [
                "OBJ_EVENT_GFX_BERRY_TREE",
                "OBJ_EVENT_GFX_BIRCHS_BAG",
                "OBJ_EVENT_GFX_CUTTABLE_TREE",
                "OBJ_EVENT_GFX_ITEM_BALL",
                "OBJ_EVENT_GFX_NINJA_BOY",
                "OBJ_EVENT_GFX_PICHU_DOLL",
                "OBJ_EVENT_GFX_SWABLU_DOLL",
                "OBJ_EVENT_GFX_TRUCK",
                "OBJ_EVENT_GFX_VAR_0",
                "OBJ_EVENT_GFX_VAR_1",
                "OBJ_EVENT_GFX_VAR_2",
                "OBJ_EVENT_GFX_VAR_3",
                "OBJ_EVENT_GFX_VAR_4",
                "OBJ_EVENT_GFX_VAR_5",
                "OBJ_EVENT_GFX_VAR_6",
                "OBJ_EVENT_GFX_VAR_7",
                "OBJ_EVENT_GFX_VAR_8",
                "OBJ_EVENT_GFX_VAR_9",
                "OBJ_EVENT_GFX_VAR_A",
                "OBJ_EVENT_GFX_VAR_B",
                "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
                "OBJ_EVENT_GFX_VIGOROTH_FACING_AWAY",
                "OBJ_EVENT_GFX_YOUNGSTER",
                "OBJ_EVENT_GFX_ZIGZAGOON_1",
            ],
            "the unsupported graphics-id set changed"
        );

        let drawn_as_may: Vec<_> = reachable
            .iter()
            .filter(|id| resolve_sprite_source(id, PlayerCharacter::May, &no_flags).is_some())
            .copied()
            .collect();
        assert_eq!(
            drawn_as_may, drawn,
            "graphics-id support must not depend on the player character"
        );
    }

    #[test]
    fn resolve_sprite_source_returns_none_for_decorations_and_props() {
        let no_flags = EventData::new();
        assert!(
            resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan, &no_flags)
                .is_none()
        );
        assert!(
            resolve_sprite_source("OBJ_EVENT_GFX_TRUCK", PlayerCharacter::Brendan, &no_flags)
                .is_none()
        );
        assert!(resolve_sprite_source(
            "OBJ_EVENT_GFX_ITEM_BALL",
            PlayerCharacter::Brendan,
            &no_flags
        )
        .is_none());
        assert!(resolve_sprite_source(
            "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
            PlayerCharacter::Brendan,
            &no_flags
        )
        .is_none());
    }

    #[test]
    fn npc_palette_tags_use_distinct_banks_starting_at_one() {
        assert_eq!(NpcPaletteTag::Npc1.bank(), 1);
        assert_eq!(NpcPaletteTag::Npc2.bank(), 2);
        assert_eq!(NpcPaletteTag::Npc3.bank(), 3);
        assert_eq!(NpcPaletteTag::Npc4.bank(), 4);
    }

    #[test]
    fn object_screen_position_matches_the_player_obj_position_when_colocated_and_at_rest() {
        let (x, y) = object_screen_position((5, 5), (5, 5), (0, 0));
        assert_eq!(x, avatar::PLAYER_OBJ_X);
        assert_eq!(y, avatar::PLAYER_OBJ_Y);
    }

    #[test]
    fn object_screen_position_offsets_by_one_metatile_per_tile_of_distance_at_rest() {
        let metatile_px = u16::try_from(super::METATILE_PX).unwrap();

        let (x, y) = object_screen_position((6, 5), (5, 5), (0, 0));
        assert_eq!(x, avatar::PLAYER_OBJ_X + metatile_px);
        assert_eq!(y, avatar::PLAYER_OBJ_Y);

        let (x2, y2) = object_screen_position((5, 4), (5, 5), (0, 0));
        assert_eq!(x2, avatar::PLAYER_OBJ_X);
        let expected_y = avatar::PLAYER_OBJ_Y - u8::try_from(metatile_px).unwrap();
        assert_eq!(y2, expected_y);
    }

    #[test]
    fn object_screen_position_applies_the_camera_lag_before_wrapping() {
        let metatile_px = i32::from(u16::try_from(super::METATILE_PX).unwrap());
        let (x, _) = object_screen_position((6, 5), (5, 5), (-metatile_px, 0));
        assert_eq!(
            x,
            avatar::PLAYER_OBJ_X,
            "one metatile of distance offset by one metatile of opposite-signed lag \
             must cancel back to the player's own screen column"
        );
    }

    #[test]
    fn camera_placement_uses_the_upstream_walk_duration() {
        assert_eq!(
            engine::overworld::WALK_FRAMES_PER_TILE,
            EXPECTED_WALK_FRAMES_PER_TILE
        );
    }

    #[test]
    fn the_spawn_window_keeps_every_admitted_sprite_clear_of_the_oam_y_wrap() {
        const CANDIDATE_ROW_RADIUS: i32 = 32;
        const EXPECTED_ADMITTED_ROWS: usize = 17;
        const EXPECTED_UNWRAPPED_Y_BOUNDS: (i32, i32) = (-64, 224);

        let player = (40, 40);
        let max_lag = i32::from(engine::overworld::WALK_FRAMES_PER_TILE);
        let screen_rows = i32::try_from(rendering::Framebuffer::HEIGHT).unwrap();

        let mut bindings = HashMap::new();
        bindings.insert(
            "OBJ_EVENT_GFX_MOM",
            SpriteBinding {
                base_tile: avatar::FRAME_BLOCK_TILES,
                palette_bank: NpcPaletteTag::Npc4.bank(),
            },
        );
        let data = EventData::new();
        let here: &'static [ObjectEvent] = Box::leak(Box::new([object(
            "OBJ_EVENT_GFX_MOM",
            40,
            40,
            MovementType::FaceDown,
        )]));
        let drawn = oam_entries(
            here,
            &bindings,
            &PlayerState::new(player, DEFAULT_ELEVATION, Direction::South),
            &data,
        );
        let sprite_height = i32::try_from(drawn[0].dimensions().1).unwrap();

        let mut admitted = 0_usize;
        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;
        for event_y in (player.1 - CANDIDATE_ROW_RADIUS)..=(player.1 + CANDIDATE_ROW_RADIUS) {
            let event = object(
                "OBJ_EVENT_GFX_MOM",
                i16::try_from(player.0).unwrap(),
                i16::try_from(event_y).unwrap(),
                MovementType::FaceDown,
            );
            if !engine::overworld::object_event_is_in_view(&event, player) {
                continue;
            }
            admitted += 1;
            for lag_y in -max_lag..=max_lag {
                let unwrapped = i32::from(avatar::PLAYER_OBJ_Y)
                    + (event_y - player.1) * super::METATILE_PX
                    + lag_y;
                min_y = min_y.min(unwrapped);
                max_y = max_y.max(unwrapped);

                let (_, wrapped) = object_screen_position((player.0, event_y), player, (0, lag_y));
                assert_eq!(
                    i32::from(wrapped),
                    unwrapped.rem_euclid(OAM_Y_MODULUS),
                    "object_screen_position must be the wrapped unwrapped position"
                );
            }
        }

        assert_eq!(
            admitted, EXPECTED_ADMITTED_ROWS,
            "the object-event view window changed"
        );
        assert_eq!(
            (min_y, max_y),
            EXPECTED_UNWRAPPED_Y_BOUNDS,
            "the admitted y range with camera lag changed"
        );

        assert!(
            max_y + sprite_height <= OAM_Y_MODULUS,
            "a {sprite_height}px-tall object event at the bottom of the spawn \
             window ({max_y}) would wrap across the top of the screen"
        );

        assert!(
            OAM_Y_MODULUS + min_y >= screen_rows,
            "the top of the spawn window ({min_y}) aliases to row {}, which \
             must stay below the {screen_rows}-row screen",
            OAM_Y_MODULUS + min_y
        );
        assert!(
            min_y + sprite_height <= 0,
            "and its true position must be entirely above row 0"
        );
    }

    #[test]
    fn oam_entries_skips_a_hidden_object_and_one_with_no_binding() {
        let mut data = EventData::new();
        data.flag_set(HIDE_BRENDAN_BEDROOM_RIVAL).unwrap();

        let hidden = {
            let mut o = object(
                "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
                7,
                1,
                MovementType::FaceDown,
            );
            o.flag = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM";
            o
        };
        let unrecognized = object("OBJ_EVENT_GFX_VAR_0", 0, 0, MovementType::LookAround);
        let events: &'static [ObjectEvent] = Box::leak(Box::new([hidden, unrecognized]));

        let mut bindings = HashMap::new();
        bindings.insert(
            "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
            SpriteBinding {
                base_tile: 0,
                palette_bank: PLAYER_PALETTE_BANK,
            },
        );

        let player = PlayerState::new((7, 2), DEFAULT_ELEVATION, Direction::North);
        let entries = oam_entries(events, &bindings, &player, &data);
        assert!(
            entries.is_empty(),
            "the hidden rival and the unrecognized decoration must both be skipped"
        );
    }

    #[test]
    fn oam_entries_draws_a_visible_recognized_object() {
        let data = EventData::new();
        let mom = object("OBJ_EVENT_GFX_MOM", 2, 6, MovementType::FaceRight);
        let events: &'static [ObjectEvent] = Box::leak(Box::new([mom]));

        let mut bindings = HashMap::new();
        bindings.insert(
            "OBJ_EVENT_GFX_MOM",
            SpriteBinding {
                base_tile: avatar::FRAME_BLOCK_TILES,
                palette_bank: NpcPaletteTag::Npc4.bank(),
            },
        );

        let player = PlayerState::new((2, 6), DEFAULT_ELEVATION, Direction::South);
        let entries = oam_entries(events, &bindings, &player, &data);
        assert_eq!(entries.len(), 1);
        let entry = entries[0];
        assert_eq!(entry.palette_bank(), NpcPaletteTag::Npc4.bank());
        assert!(entry.enabled());
        assert!(entry.h_flip());
        let (frame_west_stand, _) = avatar::stand_frame_for(Direction::West);
        assert_eq!(
            entry.tile_index(),
            avatar::FRAME_BLOCK_TILES + frame_west_stand * avatar::FRAME_TILES
        );
    }

    #[test]
    fn oam_entries_take_their_priority_from_the_templates_elevation() {
        let data = EventData::new();
        let mut bindings = HashMap::new();
        bindings.insert(
            "OBJ_EVENT_GFX_MOM",
            SpriteBinding {
                base_tile: avatar::FRAME_BLOCK_TILES,
                palette_bank: NpcPaletteTag::Npc4.bank(),
            },
        );
        let player = PlayerState::new((2, 6), DEFAULT_ELEVATION, Direction::South);

        let flat = object("OBJ_EVENT_GFX_MOM", 2, 6, MovementType::FaceDown);
        assert_eq!(
            flat.elevation, DEFAULT_ELEVATION,
            "fixture precondition: ordinary floor"
        );
        let flat: &'static [ObjectEvent] = Box::leak(Box::new([flat]));
        assert_eq!(
            oam_entries(flat, &bindings, &player, &data)[0].priority(),
            avatar::PLAYER_OBJ_PRIORITY,
        );

        let raised = {
            let mut o = object("OBJ_EVENT_GFX_MOM", 2, 6, MovementType::FaceDown);
            o.elevation = RAISED_ELEVATION;
            o
        };
        let raised: &'static [ObjectEvent] = Box::leak(Box::new([raised]));
        assert_eq!(
            oam_entries(raised, &bindings, &player, &data)[0].priority(),
            RAISED_PRIORITY,
            "a raised object must not draw at the flat priority"
        );
    }

    #[test]
    fn a_distant_object_event_produces_no_oam_entry_instead_of_wrapping_onto_the_player() {
        const BOY_POSITION: (i32, i32) = (14, 17);
        const NORTH_EDGE: (i32, i32) = (14, 1);
        const NEAR_BOY: (i32, i32) = (14, 12);

        let events = assets::MapEventsTable::new()
            .resolve(assets::MapId("MAP_LITTLEROOT_TOWN"))
            .expect("a bundled map must resolve in the generated table");
        let boy = events
            .object_events
            .iter()
            .find(|o| o.graphics_id == "OBJ_EVENT_GFX_BOY_2")
            .expect("Littleroot Town's object events include the boy");
        assert_eq!(
            (i32::from(boy.x), i32::from(boy.y)),
            BOY_POSITION,
            "the bundled boy moved"
        );

        let (_, wrapped_y) = object_screen_position(BOY_POSITION, NORTH_EDGE, (0, 0));
        assert_eq!(
            wrapped_y,
            avatar::PLAYER_OBJ_Y,
            "the distant coordinate must alias before view culling"
        );

        let mut bindings = HashMap::new();
        bindings.insert(
            "OBJ_EVENT_GFX_BOY_2",
            SpriteBinding {
                base_tile: avatar::FRAME_BLOCK_TILES,
                palette_bank: NpcPaletteTag::Npc1.bank(),
            },
        );
        let data = EventData::new();

        let north_edge = PlayerState::new(NORTH_EDGE, DEFAULT_ELEVATION, Direction::South);
        let entries = oam_entries(events.object_events, &bindings, &north_edge, &data);
        assert!(
            entries.iter().all(|e| e.y() != avatar::PLAYER_OBJ_Y),
            "a distant NPC must not alias onto the player's row"
        );
        assert!(
            entries.is_empty(),
            "the boy is the only bound object event here, and he is out of view"
        );

        let near = PlayerState::new(NEAR_BOY, DEFAULT_ELEVATION, Direction::South);
        let entries = oam_entries(events.object_events, &bindings, &near, &data);
        assert_eq!(entries.len(), 1, "in view from five tiles away");
        let tile_distance = BOY_POSITION.1 - NEAR_BOY.1;
        let expected_y =
            avatar::PLAYER_OBJ_Y + u8::try_from(tile_distance * super::METATILE_PX).unwrap();
        assert_eq!(entries[0].y(), expected_y);
    }

    fn open_runtime(width: u16, height: u16) -> engine::overworld::MapRuntime<'static> {
        let cell_count = usize::from(width) * usize::from(height);
        let mut bytes = Vec::with_capacity(cell_count * std::mem::size_of::<u16>());
        for _ in 0..(u32::from(width) * u32::from(height)) {
            let raw = assets::MetatileCell {
                metatile_id: 0,
                collision: 0,
                elevation: DEFAULT_ELEVATION,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let header: &'static assets::MapHeader = Box::leak(Box::new(assets::MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: assets::Weather::None,
            map_type: assets::MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: assets::BattleScene::Normal,
            connections: &[] as &'static [assets::MapConnection],
        }));
        let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));
        let layout: &'static assets::MapLayout = Box::leak(Box::new(assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }));
        let grid = layout.grid(bytes).unwrap();
        engine::overworld::MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            grid,
            assets::MetatileAttributeTable::new(&[]),
            assets::MetatileAttributeTable::new(&[]),
        )
    }

    #[test]
    fn oam_entries_glues_a_stationary_npc_to_the_camera_through_every_direction_of_a_step() {
        let midpoint_ticks = EXPECTED_WALK_FRAMES_PER_TILE / 2;
        let remaining_transit_ticks = EXPECTED_WALK_FRAMES_PER_TILE - midpoint_ticks - 1;
        let data = EventData::new();
        let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

        let mut bindings = HashMap::new();
        bindings.insert(
            "OBJ_EVENT_GFX_MOM",
            SpriteBinding {
                base_tile: avatar::FRAME_BLOCK_TILES,
                palette_bank: NpcPaletteTag::Npc4.bank(),
            },
        );

        for direction in [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ] {
            let runtime = open_runtime(10, 10);
            let start = (5, 5);
            let mom = object("OBJ_EVENT_GFX_MOM", 7, 5, MovementType::FaceDown);
            let events: &'static [ObjectEvent] = Box::leak(Box::new([mom]));

            let mut player = PlayerState::new(start, DEFAULT_ELEVATION, direction);
            let at_rest = oam_entries(events, &bindings, &player, &data);
            assert_eq!(
                at_rest.len(),
                1,
                "{direction:?}: Mom must be in view at rest"
            );
            let (rest_x, rest_y) = (at_rest[0].x(), at_rest[0].y());

            let outcome = player.step(Some(direction), &runtime, &no_connections, &data);
            assert!(
                matches!(outcome, engine::overworld::StepOutcome::Advanced { .. }),
                "{direction:?}: an open runtime must let the step through"
            );

            let progress0 = oam_entries(events, &bindings, &player, &data);
            assert_eq!(
                (progress0[0].x(), progress0[0].y()),
                (rest_x, rest_y),
                "{direction:?}: a stationary NPC must not jump when a step starts"
            );

            for _ in 0..midpoint_ticks {
                player.tick();
            }
            assert!(
                player.in_transit(),
                "{direction:?}: the midpoint must remain in transit"
            );
            let mid = oam_entries(events, &bindings, &player, &data);

            for _ in 0..remaining_transit_ticks {
                player.tick();
            }
            assert!(
                player.in_transit(),
                "{direction:?}: the last transit frame must remain in transit"
            );
            let last = oam_entries(events, &bindings, &player, &data);

            player.tick();
            assert!(
                !player.in_transit(),
                "{direction:?}: the final tick must end the transit"
            );
            let settled = oam_entries(events, &bindings, &player, &data);

            let (dx, dy) = direction.delta();
            let step = |a: rendering::OamEntry, b: rendering::OamEntry| {
                (
                    i32::from(b.x()) - i32::from(a.x()),
                    i32::from(b.y()) - i32::from(a.y()),
                )
            };
            assert_eq!(
                step(progress0[0], mid[0]),
                (
                    -dx * i32::from(midpoint_ticks),
                    -dy * i32::from(midpoint_ticks)
                ),
                "{direction:?}: progress zero to midpoint"
            );
            assert_eq!(
                step(mid[0], last[0]),
                (
                    -dx * i32::from(remaining_transit_ticks),
                    -dy * i32::from(remaining_transit_ticks),
                ),
                "{direction:?}: midpoint to last transit frame"
            );
            assert_eq!(
                step(last[0], settled[0]),
                (-dx, -dy),
                "{direction:?}: frame 15 to the first resting frame"
            );

            let fresh_at_destination =
                PlayerState::new(player.position(), DEFAULT_ELEVATION, direction);
            let fresh = oam_entries(events, &bindings, &fresh_at_destination, &data);
            assert_eq!(
                (settled[0].x(), settled[0].y()),
                (fresh[0].x(), fresh[0].y()),
                "{direction:?}: the settled position must match a fresh player already there"
            );
        }
    }
}
