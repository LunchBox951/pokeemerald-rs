//! NPC/prop object-event rendering (I-3, issue #161): generalizes
//! [`super::avatar`]'s player OBJ machinery to the current map's other
//! [`assets::ObjectEvent`]s.
//!
//! # Scope: which graphics ids actually draw a sprite
//!
//! Every visible object event is *tracked* (hide-flag filtered, available to
//! [`engine::overworld::facing_object_event`] for interaction) regardless of
//! its `graphics_id`, but only a bounded, explicitly resolved set of ids
//! actually contribute an [`OamEntry`] here -- [`resolve_sprite_source`]'s
//! own match arms are the full list. Two shapes are modelled:
//!
//! - **`PlayerCharacter`**: the rival's own-house bedroom/downstairs objects
//!   (`OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL`/`_RIVAL_MAY_NORMAL`) reuse
//!   upstream's own `sPicTable_BrendanNormal`/`MayNormal` -- the *same*
//!   walking sheet and palette bank 0 [`super::OverworldScene`] already
//!   loaded for the player avatar, so no extra sprite is decoded for them.
//!   Only the variant matching *this run's* [`PlayerCharacter`] resolves
//!   (this port fixes the player to [`PlayerCharacter::Brendan`] --
//!   `super::load_default_room`'s own doc comment -- so the opposite-gender
//!   rival graphic, reachable only by visiting the other protagonist's
//!   house, is untracked here).
//! - **`Standard`**: a dozen ordinary 16x32 "standing" NPCs (Mom, the twin,
//!   Professor Birch, ...) that share the exact upstream
//!   `overworld_frame(<pic>, 2, 4, n)` 9-frame layout
//!   [`super::avatar::pack_people_sheet_frames`] already knows how to pack,
//!   drawn from one of the four generic `npc_1..4` palettes
//!   (`OBJ_EVENT_PAL_TAG_NPC_1..4`).
//!
//! **Not drawn** (still hide-flag tracked, just no [`OamEntry`]):
//! decorations (`OBJ_EVENT_GFX_VAR_*`, upstream's variable-graphics
//! decoration system -- no decoration-placement save state or graphics-id
//! resolution table exists here), inanimate props/dolls/the moving truck
//! (different OAM shapes and sizes -- `48x48`/`16x16` -- than the uniform
//! 16x32 this module's OAM building assumes), and the two non-16x32-vertical
//! NPCs in scope's own reachable maps (Vigoroth, `32x32`; the ninja boy,
//! `16x16`). A future slice can extend [`resolve_sprite_source`] and this
//! module's OAM building to cover them; nothing here silently mis-renders
//! them as 16x32.
//!
//! # Screen position: resting only
//!
//! [`resting_screen_position`] generalizes [`super::avatar::PLAYER_OBJ_X`]/
//! `PLAYER_OBJ_Y`'s derivation (upstream `SetSpritePosToMapCoords`) from the
//! player's own always-zero `mapX - gSaveBlock1Ptr->pos.x` identity to the
//! general case, where an NPC's map position differs from the player's own.
//! It deliberately does **not** additionally apply
//! [`super::viewport::build_tilemaps`]'s own mid-step `scroll_x`/`scroll_y`
//! lag term the way upstream's `gSpriteCoordOffsetX`/`Y` would (every real
//! object event sprite -- not just the player's -- is nudged by that shared
//! camera-pixel offset, so a stationary NPC visibly slides in sync with the
//! background as the player walks past it) -- documented fidelity delta:
//! an NPC's on-screen position here is only exactly correct while the player
//! is **at rest** (between steps); mid-transit it can be off by up to one
//! metatile (snapping to its final position instead of sliding smoothly).
//! Never affects tile-position logic (hide-flag filtering, interaction) --
//! only this module's own pixel placement.

use std::collections::HashMap;

use assets::pack::{AssetPack, PaletteRef};
use assets::ObjectEvent;
use engine::event_data::EventData;
use engine::overworld::{initial_facing_direction, PlayerState};
use rendering::{Bgr555, BitDepth, OamEntry, Palette};

use super::avatar::{self, PlayerCharacter};
use super::{OverworldSceneError, METATILE_PX};

/// One of the four generic NPC palette banks (`OBJ_EVENT_PAL_TAG_NPC_1..4`,
/// `graphics/object_events/palettes/npc_{1..4}.pal`) -- see
/// [`build_combined_palette`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NpcPaletteTag {
    Npc1,
    Npc2,
    Npc3,
    Npc4,
}

impl NpcPaletteTag {
    /// This tag's fixed palette bank in the scene's combined sprite
    /// [`Palette`] -- bank 0 is always the player's own (`avatar::sprite_palette`),
    /// so these start at 1.
    const fn bank(self) -> u8 {
        match self {
            Self::Npc1 => 1,
            Self::Npc2 => 2,
            Self::Npc3 => 3,
            Self::Npc4 => 4,
        }
    }

    /// The pack entry name (`sprite/palette/<name>`, via
    /// [`AssetPack::sprite_palette`]) for this tag.
    const fn pack_name(self) -> &'static str {
        match self {
            Self::Npc1 => "npc_1",
            Self::Npc2 => "npc_2",
            Self::Npc3 => "npc_3",
            Self::Npc4 => "npc_4",
        }
    }
}

/// Where a `graphics_id` this module recognizes draws its sprite from
/// (module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NpcSpriteSource {
    /// Reuses this run's player-character sheet/palette (bank 0), already
    /// loaded by [`super::OverworldScene::from_pack`] -- no extra decode.
    PlayerCharacter,
    /// A standalone 9-frame 16x32 sheet plus one of the four generic NPC
    /// palette banks.
    Standard {
        sprite_path: &'static str,
        palette: NpcPaletteTag,
    },
}

/// Resolve `graphics_id` into a sprite source, or `None` if this slice
/// doesn't render a sprite for it (module docs' "not drawn" list).
///
/// `sprite_path` values are pack sprite ids (`AssetPack::sprite`, i.e. the
/// upstream `graphics/object_events/pics/people/<path>.png` file, minus
/// extension) -- transcribed from `object_event_graphics_info_pointers.h`'s
/// `OBJ_EVENT_GFX_*` -> `gObjectEventGraphicsInfo_*` table and each of those
/// structs' own `.images`/`.paletteTag` fields
/// (`object_event_pic_tables.h`/`object_event_graphics.h`).
fn resolve_sprite_source(graphics_id: &str, player: PlayerCharacter) -> Option<NpcSpriteSource> {
    use NpcPaletteTag::{Npc1, Npc2, Npc3, Npc4};
    use NpcSpriteSource::{PlayerCharacter as PlayerLike, Standard};

    match graphics_id {
        "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL" if player == PlayerCharacter::Brendan => {
            Some(PlayerLike)
        }
        "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL" if player == PlayerCharacter::May => Some(PlayerLike),
        "OBJ_EVENT_GFX_MOM" => Some(Standard {
            sprite_path: "mom",
            palette: Npc4,
        }),
        "OBJ_EVENT_GFX_TWIN" => Some(Standard {
            sprite_path: "twin",
            palette: Npc2,
        }),
        "OBJ_EVENT_GFX_FAT_MAN" => Some(Standard {
            sprite_path: "fat_man",
            palette: Npc1,
        }),
        "OBJ_EVENT_GFX_BOY_2" => Some(Standard {
            sprite_path: "boy_2",
            palette: Npc1,
        }),
        "OBJ_EVENT_GFX_PROF_BIRCH" => Some(Standard {
            sprite_path: "prof_birch",
            palette: Npc3,
        }),
        "OBJ_EVENT_GFX_WOMAN_4" => Some(Standard {
            sprite_path: "woman_4",
            palette: Npc1,
        }),
        "OBJ_EVENT_GFX_NORMAN" => Some(Standard {
            sprite_path: "gym_leaders/norman",
            palette: Npc4,
        }),
        "OBJ_EVENT_GFX_SCIENTIST_1" => Some(Standard {
            sprite_path: "scientist_1",
            palette: Npc3,
        }),
        _ => None,
    }
}

/// One recognized graphics id's resolved rendering binding: which frame
/// block of the scene's combined sprite [`rendering::Tileset`] to draw from,
/// and which palette bank.
#[derive(Debug, Clone, Copy)]
pub(super) struct SpriteBinding {
    base_tile: u16,
    palette_bank: u8,
}

/// Resolve every distinct `graphics_id` `object_events` references into a
/// [`SpriteBinding`], appending each distinct [`NpcSpriteSource::Standard`]
/// sheet's packed frame bytes to `sprite_bytes` in turn (module docs: the
/// scene's sprite [`rendering::Tileset`] is one combined decode, so this
/// only ever *appends*; the caller has already seeded `sprite_bytes` with
/// the player's own frames at base tile `0`).
///
/// # Errors
///
/// [`OverworldSceneError::Pack`]/[`OverworldSceneError::Asset`] if a
/// resolved sprite's pack entry is missing; the same
/// [`OverworldSceneError::SpriteSheetWrongDimensions`]/
/// [`OverworldSceneError::ImagePixelCountMismatch`]/
/// [`OverworldSceneError::ImageNotTileAligned`] cases as
/// [`avatar::pack_people_sheet_frames`].
pub(super) fn resolve_bindings(
    pack: &AssetPack,
    player: PlayerCharacter,
    object_events: &'static [ObjectEvent],
    sprite_bytes: &mut Vec<u8>,
) -> Result<HashMap<&'static str, SpriteBinding>, OverworldSceneError> {
    let mut bindings = HashMap::new();
    for event in object_events {
        if bindings.contains_key(event.graphics_id) {
            continue;
        }
        match resolve_sprite_source(event.graphics_id, player) {
            Some(NpcSpriteSource::PlayerCharacter) => {
                bindings.insert(
                    event.graphics_id,
                    SpriteBinding {
                        base_tile: 0,
                        palette_bank: 0,
                    },
                );
            }
            Some(NpcSpriteSource::Standard {
                sprite_path,
                palette,
            }) => {
                let base_bytes = sprite_bytes.len();
                #[allow(clippy::cast_possible_truncation)] // bounded by a handful of small sheets.
                let base_tile = (base_bytes / 32) as u16;
                debug_assert!(
                    base_tile.is_multiple_of(avatar::FRAME_BLOCK_TILES),
                    "every prior block is a whole `FRAME_BLOCK_TILES`-tile sheet, player's own \
                     included, so every new base tile must land on that same stride"
                );
                let image = pack.sprite(sprite_path)?;
                sprite_bytes.extend(avatar::pack_people_sheet_frames(sprite_path, image)?);
                bindings.insert(
                    event.graphics_id,
                    SpriteBinding {
                        base_tile,
                        palette_bank: palette.bank(),
                    },
                );
            }
            None => {}
        }
    }
    Ok(bindings)
}

/// Build the scene's combined sprite [`Palette`]: `player_bank0`'s colors at
/// bank 0 (the player's own -- also what
/// [`NpcSpriteSource::PlayerCharacter`] entries draw from), plus all four
/// generic `npc_1..4` palettes at banks 1..=4 (module docs) -- loaded
/// unconditionally, regardless of whether this particular map's object
/// events reference any of them, since they're a handful of 16-color banks
/// each and every map still needs *a* combined palette either way.
///
/// # Errors
///
/// [`OverworldSceneError::Pack`] if `sprite/palette/npc_1..4` is missing
/// from the pack (never true for a real pack `cargo xtask extract`
/// produces, once [`crate::overworld`]'s extraction covers them).
pub(super) fn build_combined_palette(
    pack: &AssetPack,
    player_bank0: PaletteRef<'_>,
) -> Result<Palette, OverworldSceneError> {
    let mut colors = [Bgr555::default(); Palette::LEN];
    avatar::fill_palette_bank(&mut colors, 0, player_bank0);
    for tag in [
        NpcPaletteTag::Npc1,
        NpcPaletteTag::Npc2,
        NpcPaletteTag::Npc3,
        NpcPaletteTag::Npc4,
    ] {
        let raw = pack.sprite_palette(tag.pack_name())?;
        avatar::fill_palette_bank(&mut colors, usize::from(tag.bank()), raw);
    }
    Ok(Palette::new(colors))
}

/// Wrap a computed screen coordinate into the GBA OAM 9-bit X field's raw
/// representation (`0..=511`, upper half sign-extending to negative --
/// [`OamEntry::new`]'s own doc comment already masks/decodes this, so any
/// `u16` here round-trips correctly even when the true position is off the
/// left edge of the screen).
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)] // rem_euclid(512) is always 0..512.
fn wrap_oam_x(x: i32) -> u16 {
    x.rem_euclid(512) as u16
}

/// Wrap a computed screen coordinate into the GBA OAM 8-bit Y field (wraps
/// modulo 256, matching hardware -- `OamEntry::y`'s own doc comment).
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)] // rem_euclid(256) is always 0..256.
fn wrap_oam_y(y: i32) -> u8 {
    y.rem_euclid(256) as u8
}

/// The screen position an object event standing at `npc` (map tile
/// coordinates) draws at while `player` is at rest at `player_pos` -- module
/// docs' generalization of [`avatar::PLAYER_OBJ_X`]/`PLAYER_OBJ_Y`.
fn resting_screen_position(npc: (i32, i32), player_pos: (i32, i32)) -> (u16, u8) {
    let dx = (npc.0 - player_pos.0) * METATILE_PX;
    let dy = (npc.1 - player_pos.1) * METATILE_PX;
    let x = i32::from(avatar::PLAYER_OBJ_X) + dx;
    let y = i32::from(avatar::PLAYER_OBJ_Y) + dy;
    (wrap_oam_x(x), wrap_oam_y(y))
}

/// Build one [`OamEntry`] per currently-visible object event this module
/// recognizes a sprite for (module docs), to append after the player's own
/// entry in [`super::OverworldScene::compose`]'s
/// [`rendering::SpriteLayer`] -- lower array index wins a same-priority tie
/// (`rendering::sprite`'s own docs), so the player (index 0) always wins
/// over any NPC standing on the exact same pixel.
///
/// Each entry's tile index is [`SpriteBinding::base_tile`] plus the standing
/// frame [`avatar::stand_frame_for`] selects for the object event's
/// [`engine::overworld::initial_facing_direction`] (derived from its
/// [`MovementType`] -- module docs on why this is always the *initial*
/// facing, never updated). Objects with no [`SpriteBinding`] (an
/// unrecognized `graphics_id`) are skipped -- tracked for interaction, not
/// drawn (module docs).
#[must_use]
pub(super) fn oam_entries(
    object_events: &'static [ObjectEvent],
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
    event_data: &EventData,
) -> Vec<OamEntry> {
    let player_pos = player.position();
    visible_object_events_with_binding(object_events, bindings, event_data)
        .map(|(event, binding)| {
            let facing = initial_facing_direction(event.movement_type);
            let (frame, h_flip) = avatar::stand_frame_for(facing);
            let tile_index = binding.base_tile + frame * avatar::FRAME_TILES;
            let (x, y) =
                resting_screen_position((i32::from(event.x), i32::from(event.y)), player_pos);
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
                avatar::PLAYER_OBJ_PRIORITY,
                true,
            )
        })
        .collect()
}

/// The subset of `object_events` that is both currently visible
/// ([`engine::overworld::object_event_is_visible`]) and has a
/// [`SpriteBinding`] this module can actually draw, paired with that
/// binding.
fn visible_object_events_with_binding<'a>(
    object_events: &'a [ObjectEvent],
    bindings: &'a HashMap<&'static str, SpriteBinding>,
    event_data: &'a EventData,
) -> impl Iterator<Item = (&'a ObjectEvent, &'a SpriteBinding)> {
    object_events
        .iter()
        .filter(move |event| engine::overworld::object_event_is_visible(event, event_data))
        .filter_map(move |event| {
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
            elevation: 3,
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
    fn resolve_sprite_source_matches_the_current_players_own_rival_variant_only() {
        assert_eq!(
            resolve_sprite_source(
                "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
                PlayerCharacter::Brendan
            ),
            Some(NpcSpriteSource::PlayerCharacter)
        );
        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL", PlayerCharacter::May),
            None,
            "the Brendan-shaped rival variant is untracked when playing as May"
        );
        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_RIVAL_MAY_NORMAL", PlayerCharacter::Brendan),
            None
        );
    }

    #[test]
    fn resolve_sprite_source_resolves_mom_to_the_npc4_palette() {
        let source = resolve_sprite_source("OBJ_EVENT_GFX_MOM", PlayerCharacter::Brendan).unwrap();
        assert_eq!(
            source,
            NpcSpriteSource::Standard {
                sprite_path: "mom",
                palette: NpcPaletteTag::Npc4,
            }
        );
    }

    #[test]
    fn resolve_sprite_source_returns_none_for_decorations_and_props() {
        assert!(resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan).is_none());
        assert!(resolve_sprite_source("OBJ_EVENT_GFX_TRUCK", PlayerCharacter::Brendan).is_none());
        assert!(
            resolve_sprite_source("OBJ_EVENT_GFX_ITEM_BALL", PlayerCharacter::Brendan).is_none()
        );
        assert!(resolve_sprite_source(
            "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
            PlayerCharacter::Brendan
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
    fn resting_screen_position_matches_the_player_obj_position_when_colocated() {
        let (x, y) = resting_screen_position((5, 5), (5, 5));
        assert_eq!(x, avatar::PLAYER_OBJ_X);
        assert_eq!(y, avatar::PLAYER_OBJ_Y);
    }

    #[test]
    fn resting_screen_position_offsets_by_one_metatile_per_tile_of_distance() {
        let metatile_px = u16::try_from(super::METATILE_PX).unwrap();

        let (x, y) = resting_screen_position((6, 5), (5, 5));
        assert_eq!(x, avatar::PLAYER_OBJ_X + metatile_px);
        assert_eq!(y, avatar::PLAYER_OBJ_Y);

        let (x2, y2) = resting_screen_position((5, 4), (5, 5));
        assert_eq!(x2, avatar::PLAYER_OBJ_X);
        let expected_y = avatar::PLAYER_OBJ_Y - u8::try_from(metatile_px).unwrap();
        assert_eq!(y2, expected_y);
    }

    #[test]
    fn oam_entries_skips_a_hidden_object_and_one_with_no_binding() {
        let mut data = EventData::new();
        data.flag_set(0x2F8).unwrap(); // FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM

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
                palette_bank: 0,
            },
        );

        let player = PlayerState::new((7, 2), 3, Direction::North);
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

        let player = PlayerState::new((2, 6), 3, Direction::South);
        let entries = oam_entries(events, &bindings, &player, &data);
        assert_eq!(entries.len(), 1);
        let entry = entries[0];
        assert_eq!(entry.palette_bank(), NpcPaletteTag::Npc4.bank());
        assert!(entry.enabled());
        // Mom's movement type is FaceRight (East); east reuses the west
        // stand frame, h-flipped (module docs' frame table).
        assert!(entry.h_flip());
        let (frame_west_stand, _) = avatar::stand_frame_for(Direction::West);
        assert_eq!(
            entry.tile_index(),
            avatar::FRAME_BLOCK_TILES + frame_west_stand * avatar::FRAME_TILES
        );
    }
}
