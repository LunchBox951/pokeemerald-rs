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
//! - **`Standard`**: eight ordinary 16x32 "standing" NPCs (Mom, the twin,
//!   the fat man, boy 2, Professor Birch, woman 4, Norman, scientist 1)
//!   that share the exact upstream
//!   `overworld_frame(<pic>, 2, 4, n)` 9-frame layout
//!   [`super::avatar::pack_people_sheet_frames`] already knows how to pack,
//!   drawn from one of the four generic `npc_1..4` palettes
//!   (`OBJ_EVENT_PAL_TAG_NPC_1..4`) -- **plus** the two rival variants
//!   (`OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL`/`_RIVAL_MAY_NORMAL`), which
//!   upstream gives the *protagonists'* own walking sheets and palettes
//!   rather than any rival-specific art, at
//!   `.paletteSlot = PALSLOT_NPC_SPECIAL` ([`OTHER_PROTAGONIST_BANK`]).
//! - **`PlayerCharacter`**: the degenerate case of the above -- a rival
//!   variant that happens to *be* this run's own protagonist, whose sheet
//!   and palette [`super::OverworldScene`] has already loaded at base tile
//!   `0` / bank `0`, so nothing extra is decoded. Not reachable in the
//!   bundled data (a house's resident rival object is hidden while you live
//!   there), but resolved rather than special-cased --
//!   [`protagonist_source`] has the full reasoning, including why the rival
//!   you can actually meet is always the *opposite* protagonist.
//!
//! Ten [`resolve_sprite_source`] arms in total, **all ten of which resolve
//! for either [`PlayerCharacter`]** -- covering ten of the **32** distinct
//! `graphics_id`s the seven bundled maps' object events actually reference
//! (Route 101, since issue #177, adds three unresolved ids -- the
//! youngster, Birch's starter bag, and the wild Zigzagoon -- to the six
//! Littleroot-family maps' own 29; `resolve_sprite_source_covers_the_reachable_graphics_ids`
//! pins that exact partition, and that it is gender-independent, so a newly
//! reachable standard NPC cannot silently stop drawing).
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
//!
//! # Documented fidelity delta: no y-derived sprite subpriority
//!
//! Upstream orders overlapping object-event sprites by a *subpriority*
//! derived from each sprite's own screen y, not by OAM index:
//! `SetObjectSubpriorityByElevation`
//! (`event_object_movement.c:7773-7779`) computes
//! `y = (16 - (screenY >> 4)) << 1` and adds it to
//! `sElevationToSubpriority[elevation]`, so a *lower* (further south)
//! sprite gets a numerically smaller subpriority and therefore draws in
//! front -- an NPC standing one tile south of the player overlaps and
//! covers the player's head. `rendering::SpriteLayer` has no subpriority
//! concept: same-priority ties go to the lower OAM index
//! (`rendering::sprite`'s own docs), and
//! [`super::OverworldScene::compose`] always puts the player at index 0,
//! so **here the player always draws in front of every NPC**, whichever
//! side of them it stands on. Only ever visible where the two 16x32
//! sprites' pixels actually overlap (adjacent tiles); never affects
//! tile-position logic. A future slice porting subpriority would also want
//! `ObjectEventUpdateSubpriority`'s `previousElevation` input, which this
//! port's stationary object events don't track yet.

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

/// The scene's combined sprite [`Palette`] bank holding the *other*
/// protagonist's own palette (`graphics/object_events/palettes/{brendan,may}.pal`,
/// upstream `OBJ_EVENT_PAL_TAG_BRENDAN`/`_MAY`) -- what an
/// `OBJ_EVENT_GFX_RIVAL_*_NORMAL` object event draws from when it is not
/// this run's own player character (see [`resolve_sprite_source`]).
///
/// A distinct bank rather than a reuse of bank 0 because upstream gives the
/// rival infos `.paletteSlot = PALSLOT_NPC_SPECIAL` while the player's own
/// use `PALSLOT_PLAYER`
/// (`src/data/object_events/object_event_graphics_info.h:1920-1937` and
/// `:1-18`) -- precisely so a rival and the player can be on screen at once
/// with different palettes.
const OTHER_PROTAGONIST_BANK: u8 = 5;

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
    /// A standalone 9-frame 16x32 sheet plus the palette bank it draws
    /// from.
    Standard {
        sprite_path: &'static str,
        palette_bank: u8,
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
    use NpcSpriteSource::Standard;

    match graphics_id {
        "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL" => {
            Some(protagonist_source(PlayerCharacter::Brendan, player))
        }
        "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL" => Some(protagonist_source(PlayerCharacter::May, player)),
        "OBJ_EVENT_GFX_MOM" => Some(Standard {
            sprite_path: "mom",
            palette_bank: Npc4.bank(),
        }),
        "OBJ_EVENT_GFX_TWIN" => Some(Standard {
            sprite_path: "twin",
            palette_bank: Npc2.bank(),
        }),
        "OBJ_EVENT_GFX_FAT_MAN" => Some(Standard {
            sprite_path: "fat_man",
            palette_bank: Npc1.bank(),
        }),
        "OBJ_EVENT_GFX_BOY_2" => Some(Standard {
            sprite_path: "boy_2",
            palette_bank: Npc1.bank(),
        }),
        "OBJ_EVENT_GFX_PROF_BIRCH" => Some(Standard {
            sprite_path: "prof_birch",
            palette_bank: Npc3.bank(),
        }),
        "OBJ_EVENT_GFX_WOMAN_4" => Some(Standard {
            sprite_path: "woman_4",
            palette_bank: Npc1.bank(),
        }),
        "OBJ_EVENT_GFX_NORMAN" => Some(Standard {
            sprite_path: "gym_leaders/norman",
            palette_bank: Npc4.bank(),
        }),
        "OBJ_EVENT_GFX_SCIENTIST_1" => Some(Standard {
            sprite_path: "scientist_1",
            palette_bank: Npc3.bank(),
        }),
        _ => None,
    }
}

/// Where an `OBJ_EVENT_GFX_RIVAL_<who>_NORMAL` object event draws from,
/// given this run's `player`.
///
/// Upstream's two rival infos are *not* gender-switched at runtime: each
/// simply reuses the corresponding protagonist's own walking sheet and
/// palette --
/// `gObjectEventGraphicsInfo_RivalBrendanNormal` has
/// `.images = sPicTable_BrendanNormal`, `.paletteTag = OBJ_EVENT_PAL_TAG_BRENDAN`
/// (`src/data/object_events/object_event_graphics_info.h:1920-1937`) and
/// `..._RivalMayNormal` has `sPicTable_MayNormal` /
/// `OBJ_EVENT_PAL_TAG_MAY` (`:2015-2032`). There are no `rival_*` PNGs or
/// palettes at all; the only fields differing from the player's own infos
/// are `.size` and `.paletteSlot`.
///
/// **The rival is always the opposite protagonist**, which is what the
/// previous binding got backwards. The two Littleroot houses are mirrored
/// maps that each hardcode their own resident's id
/// (`LittlerootTown_BrendansHouse_2F/map.json:19` ->
/// `OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL`;
/// `LittlerootTown_MaysHouse_2F/map.json:19` ->
/// `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL`), and the intro warp sends a male player
/// to Brendan's house and a female player to May's
/// (`data/maps/LittlerootTown/scripts.inc:116`/`:127`). So playing as
/// Brendan, the rival object you can actually meet is May's, in *her* house
/// -- the one the old `if player == PlayerCharacter::Brendan` guard resolved
/// to `None`, leaving it undrawn once its hide flag cleared. (The mirrored
/// script confirms the intent:
/// `RivalsHouse_2F_EventScript_Rival` does `checkplayergender` and greets
/// May for a `MALE` player -- `LittlerootTown_MaysHouse_2F/scripts.inc:236-241`.)
///
/// `who == player` is still resolved to [`NpcSpriteSource::PlayerCharacter`]:
/// that sheet and palette are already loaded at base tile 0 / bank 0, so
/// there is nothing to decode. It is not reachable in the bundled data (the
/// resident's own id is always hidden while you live there), but it is the
/// correct binding rather than a special case, and it keeps
/// [`resolve_bindings`] from decoding the player's sheet a second time if a
/// future map ever does place one.
fn protagonist_source(who: PlayerCharacter, player: PlayerCharacter) -> NpcSpriteSource {
    if who == player {
        NpcSpriteSource::PlayerCharacter
    } else {
        NpcSpriteSource::Standard {
            sprite_path: who.sprite_path(),
            palette_bank: OTHER_PROTAGONIST_BANK,
        }
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

#[cfg(test)]
impl SpriteBinding {
    /// This binding's frame block within the scene's combined sprite
    /// tileset. Exposed for the scene's own tests, which assert *which*
    /// block a resolved graphics id addresses (notably that a rival's is
    /// its own, not the player's at `0`).
    pub(super) const fn base_tile(self) -> u16 {
        self.base_tile
    }

    /// This binding's palette bank. Exposed for the same tests -- bank `0`
    /// is the player's own, so a non-zero bank is what distinguishes an
    /// independently-loaded sheet.
    pub(super) const fn palette_bank(self) -> u8 {
        self.palette_bank
    }
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
                palette_bank,
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
                        palette_bank,
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
/// [`NpcSpriteSource::PlayerCharacter`] entries draw from), all four
/// generic `npc_1..4` palettes at banks 1..=4, and the *other*
/// protagonist's own palette at [`OTHER_PROTAGONIST_BANK`] for a rival
/// object event (module docs).
///
/// All six non-player banks are loaded unconditionally, regardless of
/// whether this particular map's object events reference any of them: they
/// are a handful of 16-color banks and every map still needs *a* combined
/// palette either way.
///
/// # Errors
///
/// [`OverworldSceneError::Pack`] if `sprite/palette/npc_1..4` or the other
/// protagonist's `sprite/palette/{brendan,may}` is missing from the pack
/// (never true for a real pack `cargo xtask extract` produces -- it
/// extracts both protagonists' palettes, see [`avatar::PlayerCharacter`]).
pub(super) fn build_combined_palette(
    pack: &AssetPack,
    player: PlayerCharacter,
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
    let other = pack.sprite_palette(player.other().palette_name())?;
    avatar::fill_palette_bank(&mut colors, usize::from(OTHER_PROTAGONIST_BANK), other);
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
        // Upstream's spawn window (`engine::overworld::object_event_is_in_view`
        // -- `TrySpawnObjectEvents`/`RemoveObjectEventIfOutsideView`), applied
        // *before* the wrapped OAM entry is constructed. Load-bearing, not an
        // optimization: `wrap_oam_y` reduces modulo 256, so an object event
        // exactly 16 metatiles away in y would otherwise land on the player's
        // own screen row and draw on top of them. See that function's own docs
        // for why the window is both the faithful gate and a sufficient one.
        .filter(|(event, _)| object_event_is_in_view(event, player_pos))
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
/// ([`engine::overworld::visible_object_events`] -- the engine's own
/// hide-flag filter, shared with [`engine::overworld::facing_object_event`]
/// rather than re-implemented here) and has a [`SpriteBinding`] this module
/// can actually draw, paired with that binding.
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

    /// **Replaces** an earlier test that asserted the inverse (only the
    /// variant matching *this run's* player resolved, everything else
    /// `None`). That encoded the bug, not upstream: the rival object event a
    /// player can actually meet is always the *opposite* protagonist's, so
    /// the old binding resolved the one id that stays hidden while you live
    /// there and dropped the one that does not. See
    /// [`protagonist_source`]'s doc comment for the citations.
    ///
    /// Both gender configurations, both ids -- four cases, none `None`.
    #[test]
    fn both_rival_variants_resolve_for_either_player_gender() {
        // Playing as Brendan: May's House declares OBJ_EVENT_GFX_RIVAL_MAY_NORMAL,
        // and that object *is* the rival -- it must draw from May's own sheet
        // and palette.
        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_RIVAL_MAY_NORMAL", PlayerCharacter::Brendan),
            Some(NpcSpriteSource::Standard {
                sprite_path: "may/walking",
                palette_bank: OTHER_PROTAGONIST_BANK,
            }),
            "the rival of a Brendan player is May, drawn from May's sheet"
        );
        // The mirror image.
        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL", PlayerCharacter::May),
            Some(NpcSpriteSource::Standard {
                sprite_path: "brendan/walking",
                palette_bank: OTHER_PROTAGONIST_BANK,
            }),
            "the rival of a May player is Brendan, drawn from Brendan's sheet"
        );

        // The same-gender ids still resolve -- to the already-loaded player
        // sheet at bank 0, not to `None`.
        assert_eq!(
            resolve_sprite_source(
                "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
                PlayerCharacter::Brendan
            ),
            Some(NpcSpriteSource::PlayerCharacter)
        );
        assert_eq!(
            resolve_sprite_source("OBJ_EVENT_GFX_RIVAL_MAY_NORMAL", PlayerCharacter::May),
            Some(NpcSpriteSource::PlayerCharacter)
        );
    }

    /// The rival's palette bank must be distinct from the player's own
    /// (bank 0) and from all four generic NPC banks -- upstream gives the
    /// rival infos `PALSLOT_NPC_SPECIAL` precisely so both protagonists'
    /// palettes can be resident at once
    /// (`object_event_graphics_info.h:1920-1937` vs `:1-18`).
    #[test]
    fn the_other_protagonist_palette_bank_collides_with_nothing() {
        assert_ne!(OTHER_PROTAGONIST_BANK, 0, "bank 0 is the player's own");
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
        let source = resolve_sprite_source("OBJ_EVENT_GFX_MOM", PlayerCharacter::Brendan).unwrap();
        assert_eq!(
            source,
            NpcSpriteSource::Standard {
                sprite_path: "mom",
                palette_bank: NpcPaletteTag::Npc4.bank(),
            }
        );
    }

    /// The maps `crates/xtask/src/extract/mod.rs`'s `LAYOUTS` bundles --
    /// every map whose object events this port can ever be asked to
    /// render. Mirrored here (this crate cannot depend on `xtask`); that
    /// module's own `the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it`
    /// is the tripwire that fails loudly when the list grows.
    const BUNDLED_MAPS: [&str; 7] = [
        "MAP_LITTLEROOT_TOWN",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        "MAP_ROUTE101",
    ];

    /// Every distinct `graphics_id` referenced by an object event on one of
    /// [`BUNDLED_MAPS`], sorted -- straight from the generated
    /// [`assets::MapEventsTable`], no pack needed.
    fn reachable_graphics_ids() -> Vec<&'static str> {
        let table = assets::MapEventsTable::new();
        let mut ids: Vec<&'static str> = Vec::new();
        for map in BUNDLED_MAPS {
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

    /// The module docs' scope claim, pinned as an exact partition of the
    /// *reachable* graphics-id set: which of the 32 distinct ids the seven
    /// bundled maps reference draw a sprite, and which are only hide-flag/
    /// interaction tracked. Dropping an arm from
    /// [`resolve_sprite_source`] -- or a map-data change making a new
    /// standard NPC reachable -- fails here rather than silently making
    /// that NPC invisible.
    #[test]
    fn resolve_sprite_source_covers_the_reachable_graphics_ids() {
        let reachable = reachable_graphics_ids();
        assert_eq!(
            reachable.len(),
            32,
            "the seven bundled maps reference 32 distinct graphics ids"
        );

        let (drawn, not_drawn): (Vec<&'static str>, Vec<&'static str>) = reachable
            .iter()
            .partition(|id| resolve_sprite_source(id, PlayerCharacter::Brendan).is_some());

        assert_eq!(
            drawn,
            [
                "OBJ_EVENT_GFX_BOY_2",
                "OBJ_EVENT_GFX_FAT_MAN",
                "OBJ_EVENT_GFX_MOM",
                "OBJ_EVENT_GFX_NORMAN",
                "OBJ_EVENT_GFX_PROF_BIRCH",
                "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
                "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL",
                "OBJ_EVENT_GFX_SCIENTIST_1",
                "OBJ_EVENT_GFX_TWIN",
                "OBJ_EVENT_GFX_WOMAN_4",
            ],
            "the eight `Standard` NPCs plus *both* rival variants -- the \
             opposite-gender one is the rival the player can actually meet \
             (`protagonist_source`), and it must draw whichever way round \
             this run's `PlayerCharacter` is"
        );
        assert_eq!(
            not_drawn,
            [
                "OBJ_EVENT_GFX_BIRCHS_BAG",
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
            "module docs' own 'not drawn' list: decorations, props/dolls, \
             the truck, the two non-16x32 NPCs, and (issue #177) Route 101's \
             own unresolved youngster/bag/Zigzagoon ids"
        );

        // The *partition* is gender-independent -- every id that draws for a
        // Brendan player draws for a May player too. Only which side of the
        // rival pair reuses bank 0 changes, and that is
        // `both_rival_variants_resolve_for_either_player_gender`'s job.
        let drawn_as_may: Vec<_> = reachable
            .iter()
            .filter(|id| resolve_sprite_source(id, PlayerCharacter::May).is_some())
            .copied()
            .collect();
        assert_eq!(
            drawn_as_may, drawn,
            "no graphics id may draw for one protagonist and not the other -- \
             that asymmetry was the opposite-gender rival bug"
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

    /// The OAM y-wrap regression, on the real bundled `MAP_LITTLEROOT_TOWN`
    /// data: with the player at the map's north edge, the always-visible boy
    /// at `(14, 17)` is exactly 16 metatiles (256px) south -- the one
    /// distance an 8-bit OAM `y` field aliases onto the player's own row. He
    /// must produce no entry at all.
    ///
    /// The test first *demonstrates* the alias rather than asserting around
    /// it: `resting_screen_position` for that offset really does return
    /// [`avatar::PLAYER_OBJ_Y`], so without the in-view gate the boy would
    /// have been drawn standing on the player. Then it checks he walks back
    /// into view as the player approaches, so the gate is a distance test
    /// and not an accidental blanket skip.
    #[test]
    fn a_distant_object_event_produces_no_oam_entry_instead_of_wrapping_onto_the_player() {
        let events = assets::MapEventsTable::new()
            .resolve(assets::MapId("MAP_LITTLEROOT_TOWN"))
            .expect("a bundled map must resolve in the generated table");
        let boy = events
            .object_events
            .iter()
            .find(|o| o.graphics_id == "OBJ_EVENT_GFX_BOY_2")
            .expect("Littleroot Town's object events include the boy");
        assert_eq!((boy.x, boy.y), (14, 17), "his real map.json position");

        // The alias this gate exists to prevent, shown explicitly.
        let (_, wrapped_y) = resting_screen_position((14, 17), (14, 1));
        assert_eq!(
            wrapped_y,
            avatar::PLAYER_OBJ_Y,
            "16 metatiles of separation wraps to the player's own screen row \
             -- exactly why culling must happen before the OAM entry is built"
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

        // Player at the map's north edge: the boy is far out of view.
        let north_edge = PlayerState::new((14, 1), 3, Direction::South);
        let entries = oam_entries(events.object_events, &bindings, &north_edge, &data);
        assert!(
            entries.iter().all(|e| e.y() != avatar::PLAYER_OBJ_Y),
            "no NPC may be drawn on the player's own row from 16 tiles away"
        );
        assert!(
            entries.is_empty(),
            "the boy is the only bound object event here, and he is out of view"
        );

        // Walk south until he is genuinely on screen: now he draws, at his
        // true (unwrapped) offset rather than an aliased one.
        let near = PlayerState::new((14, 12), 3, Direction::South);
        let entries = oam_entries(events.object_events, &bindings, &near, &data);
        assert_eq!(entries.len(), 1, "in view from five tiles away");
        let expected_y = avatar::PLAYER_OBJ_Y + u8::try_from(5 * super::METATILE_PX).unwrap();
        assert_eq!(entries[0].y(), expected_y);
    }
}
