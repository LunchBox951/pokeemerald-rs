//! The player avatar OBJ: sprite tileset/palette plus per-frame
//! facing/step selection (I-3, issue #126).
//!
//! Ports the observable behaviour of upstream
//! `src/data/object_events/object_event_pic_tables.h`'s
//! `sPicTable_BrendanNormal`/`sPicTable_MayNormal` (frame layout: 9
//! consecutive 16x32 frames in `walking.png`, upstream `overworld_frame`'s
//! `width=2, height=4` tile counts) and
//! `src/data/object_events/object_event_anims.h`'s
//! `sAnim_Face{South,North,West,East}`/`sAnim_Go{South,North,West,East}`
//! (which frame index each facing/step state selects, and which reuse the
//! west frame `hFlip`-ed for east) `(behavioral-fidelity)`:
//!
//! | frame index | content                    |
//! |-------------|-----------------------------|
//! | 0           | face south (standing)       |
//! | 1           | face north (standing)       |
//! | 2           | face west (standing); east reuses this, h-flipped |
//! | 3           | south walk, forward foot    |
//! | 4           | south walk, other foot (not modeled -- see below) |
//! | 5           | north walk, forward foot    |
//! | 6           | north walk, other foot (not modeled) |
//! | 7           | west walk, forward foot; east reuses this, h-flipped |
//! | 8           | west/east walk, other foot (not modeled) |
//!
//! # The player OBJ's fixed screen position
//!
//! Transcribed from `src/event_object_movement.c`'s
//! `SetSpritePosToMapCoords`: `*destX = ((mapX - gSaveBlock1Ptr->pos.x) <<
//! 4) + dx`. For the player's own object event, `mapX`/`mapY` *is*
//! `gSaveBlock1Ptr->pos` (upstream keeps the two in lock-step), so that
//! term is always exactly `0` -- the player's OBJ screen position never
//! moves during ordinary walking; only `dx`/`dy` (a small, at-rest-zero
//! camera-pan term this port's bike-free v1 scope never sets away from 0)
//! would move it, and the *background* scrolls instead
//! (`super::viewport`'s job) `(behavioral-fidelity)`. [`PLAYER_OBJ_X`]/
//! [`PLAYER_OBJ_Y`] are that resulting constant, derived from the same
//! screen-center metatile [`super::viewport`] anchors its camera on, offset
//! up by one metatile so the 32px-tall sprite's "feet" (its bottom 16px)
//! align with the player's own tile rather than its head.
//!
//! # Documented fidelity delta: no foot-alternation across steps
//!
//! Upstream's `sAnim_GoSouth` is `ANIMCMD_FRAME(3, 8), ANIMCMD_FRAME(0, 8),
//! ANIMCMD_FRAME(4, 8), ANIMCMD_FRAME(0, 8), ANIMCMD_JUMP(0)` -- a 4-command,
//! 32-tick loop that only completes (and so shows frame 4, the *other*
//! forward foot) every second tile crossing, tracked by the anim command's
//! own persistent position across calls. [`engine::overworld::PlayerState`]
//! tracks only the *current* step's progress (S-5's own documented scope --
//! see `engine::overworld::player`'s module docs), not a cross-step parity
//! counter, so this module always shows the cycle's *first* forward-foot
//! frame (3/5/7) rather than alternating with the second (4/6/8) on
//! alternating tile crossings.

use assets::{ImageRef, PaletteRef};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::save::PlayerGender;
use rendering::{Bgr555, BitDepth, OamEntry, ObjShape, Palette};

use super::{OverworldSceneError, METATILE_PX, PLAYER_VIEW_COL, PLAYER_VIEW_ROW};

/// One walking-sheet frame's pixel size (module docs' table). `pub(super)`:
/// [`super::npc`] packs NPC "standing" sheets against the same 9-frame,
/// 16x32-per-frame layout.
pub(super) const FRAME_W: usize = 16;
pub(super) const FRAME_H: usize = 32;
/// `sPicTable_BrendanNormal`/`MayNormal`'s first 9 entries -- the on-foot
/// (not-running) frames this slice uses. `walking.png`'s remaining content
/// (if any) is never referenced (no running in v1 scope). Also the frame
/// count every NPC "people" sheet [`super::npc`] resolves shares (same
/// `overworld_frame(..., 2, 4, n)` layout upstream's own pic tables use).
pub(super) const NUM_WALK_FRAMES: usize = 9;
/// Tiles per frame: `(FRAME_W / 8) * (FRAME_H / 8)`.
pub(super) const FRAME_TILES: u16 = 8;
/// One frame block's byte length in the packed 4bpp tile stream
/// (`NUM_WALK_FRAMES * FRAME_TILES` tiles, 32 bytes each) -- the stride
/// [`super::npc::resolve_bindings`] advances by per distinct sprite it packs
/// into the scene's combined sprite tileset.
#[allow(clippy::cast_possible_truncation)] // NUM_WALK_FRAMES (9) always fits u16.
pub(super) const FRAME_BLOCK_TILES: u16 = NUM_WALK_FRAMES as u16 * FRAME_TILES;

pub(super) const FRAME_SOUTH_STAND: u16 = 0;
pub(super) const FRAME_NORTH_STAND: u16 = 1;
pub(super) const FRAME_WEST_STAND: u16 = 2; // east reuses this, h-flipped.
const FRAME_SOUTH_STEP: u16 = 3;
const FRAME_NORTH_STEP: u16 = 5;
const FRAME_WEST_STEP: u16 = 7; // east reuses this, h-flipped.

/// A walk step's forward-foot frame shows for the first half of
/// [`WALK_FRAMES_PER_TILE`], the standing frame for the second half
/// (upstream `sAnim_GoSouth`'s `ANIMCMD_FRAME(3, 8), ANIMCMD_FRAME(0, 8)`
/// pair -- module docs).
const STEP_FRAME_HALF: u8 = WALK_FRAMES_PER_TILE / 2;

/// `gObjectEventBaseOam_16x32` (`src/data/object_events/base_oam.h`): shape
/// `SPRITE_SHAPE(16x32)` (`ObjShape::Vertical`, size 2 --
/// `rendering::oam::obj_dimensions`'s table) at a *default* `.priority = 2`,
/// the same priority the middle BG layer uses (`super::viewport::MIDDLE_PRIORITY`)
/// -- a sprite wins same-priority ties against a BG (`rendering::compositor`'s
/// rules), so an object event draws in front of the middle/bottom layers but
/// behind the top one, matching `DrawMetatile`'s own "covers object event
/// sprites" comment on the top layer. `gObjectEventBaseOam_16x32` is shared
/// by every 16x32 object event, not just the player, so [`super::npc`]
/// reuses the shape/size constants for its own "standard" NPC OAM entries.
/// [`PLAYER_OBJ_PRIORITY`] itself is only the OAM template's *default*
/// value, not a claim about any drawn object: both the player
/// ([`player_entry`]) and NPCs ([`super::npc::oam_entries`]) select their
/// real priority from an elevation via [`priority_for_elevation`], and this
/// constant survives as that function's out-of-range fallback and as the
/// name for "the default an unelevated object lands on anyway".
pub(super) const PLAYER_OBJ_SHAPE: ObjShape = ObjShape::Vertical;
pub(super) const PLAYER_OBJ_SIZE: u8 = 2;
pub(super) const PLAYER_OBJ_PRIORITY: u8 = 2;

/// `sElevationToPriority` (`event_object_movement.c:7729-7731`):
/// `UpdateObjectEventElevationAndPriority` (`:7737-7746`) selects an object
/// event's OAM priority from its *retained* elevation (upstream
/// `objEvent->previousElevation` -- this port's
/// [`PlayerState::previous_elevation`](engine::overworld::PlayerState::previous_elevation)),
/// not the raw current elevation collision consults. Used for the player
/// here ([`player_entry`]) and, via each template's own elevation, for
/// stationary NPCs in [`super::npc::oam_entries`].
///
/// Ordinary floor levels (0-3, 5, 7, 9, 11, and 15 =
/// `ELEVATION_MULTI_LEVEL`) resolve to the same default
/// [`PLAYER_OBJ_PRIORITY`] (2) this OAM entry always used before issue
/// #218; the "raised" **even** elevations upstream uses for counters, stair
/// landings, and the protagonist's own bed edges (4, 6, 8, 10, 12) resolve
/// one step higher (numerically lower = more in front,
/// `rendering::compositor`'s rules) -- level with
/// [`super::viewport::TOP_PRIORITY`], which the same-priority-favors-the-
/// sprite tie rule (module docs above) then draws the sprite *in front of*
/// rather than behind, matching a raised surface visually standing above
/// whatever the top BG layer would otherwise occlude it with. Indices 13
/// and 14 -- unnamed upstream, with no `ELEVATION_*` constant and no
/// bundled map's cell using them -- are the only ones that resolve to `0`
/// (frontmost of all). The table is transcribed complete rather than
/// truncated to the reachable indices, so a future map that does use one
/// inherits a correct value rather than a gap.
///
/// **A whole-sprite priority swap is the *complete* model here, not a
/// partial one.** `UpdateObjectEventElevationAndPriority` also assigns
/// `sprite->subspriteTableNum = sElevationToSubspriteTableNum[previousElevation]`
/// (`:7733-7735, 7744`), which this port has no equivalent for --
/// [`OamEntry`] is a single hardware OBJ with one priority field. That
/// costs nothing for a 16x32 object event: the table's values (`1` for
/// every flat elevation, `2` for the raised ones, `0` for indices 13/14)
/// index `sOamTables_16x32` (`object_event_subsprites.h:176-183`), whose
/// entry 1 is `sOamTable_16x32_0` -- *one* full-size 16x32 subsprite at
/// priority 2 -- entry 2 is `sOamTable_16x32_1` -- one full-size 16x32 at
/// priority 1 -- and entry 0 is the empty table, which
/// `AddSubspritesToOamBuffer` (`sprite.c:1690-1695`) copies through as the
/// plain OBJ at `sprite->oam.priority`. Every selected table is therefore a
/// single OBJ covering the same 16x32 area at exactly the priority this
/// array already gives, so emitting one OBJ at
/// [`priority_for_elevation`]'s value reproduces the hardware result
/// pixel for pixel. The multi-piece split tables that *would* need a real
/// subsprite model are only ever selected by
/// `SetObjectEventSpriteOamTableForLongGrass` (`:7690-7705`, table nums
/// 4/5) -- a separate, unported ground effect, not this array.
const ELEVATION_TO_PRIORITY: [u8; 16] = [2, 2, 2, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 0, 0, 2];

/// [`ELEVATION_TO_PRIORITY`]`[elevation]`, defaulting to
/// [`PLAYER_OBJ_PRIORITY`] for any index past the table's end -- never
/// reachable from a real [`MetatileCell`](assets::MetatileCell) (its own
/// elevation field is a packed 4-bit value, always `0..=15`), but
/// `PlayerState::new`'s `elevation` parameter is a bare `u8` with no such
/// invariant enforced at the type level, so this stays a total function
/// rather than an indexing panic waiting on a future caller.
#[must_use]
pub(super) fn priority_for_elevation(elevation: u8) -> u8 {
    ELEVATION_TO_PRIORITY
        .get(usize::from(elevation))
        .copied()
        .unwrap_or(PLAYER_OBJ_PRIORITY)
}

/// The player OBJ's fixed screen position (module docs).
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)] // small, positive, compile-time.
pub(super) const PLAYER_OBJ_X: u16 = (PLAYER_VIEW_COL * METATILE_PX) as u16;
/// `FRAME_H` (32) is twice [`METATILE_PX`] (16): the sprite extends one
/// extra metatile upward from the tile the player stands on, so its bottom
/// half (the "feet") lines up with that tile (module docs).
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)] // FRAME_H (32) always fits i32; the whole expression is small, positive, compile-time.
pub(super) const PLAYER_OBJ_Y: u8 =
    (PLAYER_VIEW_ROW * METATILE_PX - (FRAME_H as i32 - METATILE_PX)) as u8;

/// The player character to draw -- the pack extracts both playable
/// protagonists' walking sheets and real in-game palettes
/// (`sprite/{brendan,may}/walking`, `sprite/palette/{brendan,may}`, per
/// `crates/xtask/src/extract/mod.rs`'s module docs). [`super::load_default_room`]
/// always uses [`Self::Brendan`] -- no character-select flow exists yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerCharacter {
    /// `sprite/brendan/walking`, `sprite/palette/brendan`.
    Brendan,
    /// `sprite/may/walking`, `sprite/palette/may`.
    May,
}

impl PlayerCharacter {
    pub(super) const fn sprite_path(self) -> &'static str {
        match self {
            Self::Brendan => "brendan/walking",
            Self::May => "may/walking",
        }
    }

    pub(super) const fn palette_name(self) -> &'static str {
        match self {
            Self::Brendan => "brendan",
            Self::May => "may",
        }
    }

    /// The other protagonist — whoever *this* run's player is not.
    ///
    /// Upstream has no single "the rival" graphics id: the two Littleroot
    /// houses are mirrored maps, each hardcoding its own resident's
    /// `OBJ_EVENT_GFX_RIVAL_{BRENDAN,MAY}_NORMAL`
    /// (`data/maps/LittlerootTown_BrendansHouse_2F/map.json:19` vs
    /// `LittlerootTown_MaysHouse_2F/map.json:19`), and the intro warp decides
    /// which one is home (`data/maps/LittlerootTown/scripts.inc:116` male ->
    /// Brendan's house, `:127` female -> May's). So the object event the
    /// player can actually meet as the rival is always the *other*
    /// protagonist's — see [`super::npc::resolve_sprite_source`].
    pub(super) const fn other(self) -> Self {
        match self {
            Self::Brendan => Self::May,
            Self::May => Self::Brendan,
        }
    }
}

impl From<PlayerGender> for PlayerCharacter {
    fn from(gender: PlayerGender) -> Self {
        match gender {
            PlayerGender::Female => Self::May,
            PlayerGender::Male | PlayerGender::Other(_) => Self::Brendan,
        }
    }
}

/// Validate and pack a 9-frame "people" sheet's raw pixels into the GBA's
/// packed 4bpp tile byte stream (module docs' frame table): every one of
/// upstream's `overworld_frame(<pic>, 2, 4, n)` walking-sheet pics --
/// `sPicTable_BrendanNormal`/`MayNormal` and every "standard" NPC pic table
/// [`super::npc`] resolves -- shares this exact 144x32 (9 frames of 16x32),
/// tiled-left-to-right layout, so this one packer serves both.
///
/// Returns the raw concatenated bytes rather than a decoded [`Tileset`] so
/// [`super::npc::resolve_bindings`] can concatenate several sheets'
/// worth before decoding the scene's *one* combined sprite [`Tileset`]
/// ([`SpriteLayer`](rendering::SpriteLayer) draws every sprite from a single
/// shared tileset -- see that module's docs).
///
/// # Errors
///
/// [`OverworldSceneError::SpriteSheetWrongDimensions`] if `image`'s
/// dimensions aren't exactly `NUM_WALK_FRAMES * FRAME_W` x `FRAME_H` (never
/// true for the real upstream art, `label` identifies which pack entry
/// failed); the same [`OverworldSceneError::ImagePixelCountMismatch`]/
/// [`OverworldSceneError::ImageNotTileAligned`] cases as
/// [`super::pack_4bpp_region`].
pub(super) fn pack_people_sheet_frames(
    label: &'static str,
    image: ImageRef<'_>,
) -> Result<Vec<u8>, OverworldSceneError> {
    let expected_width = u32::try_from(NUM_WALK_FRAMES * FRAME_W).unwrap_or(u32::MAX);
    #[allow(clippy::cast_possible_truncation)] // FRAME_H (32) always fits u32.
    let expected_height = FRAME_H as u32;
    if image.width != expected_width || image.height != expected_height {
        return Err(OverworldSceneError::SpriteSheetWrongDimensions {
            id: label,
            expected: (expected_width, expected_height),
            actual: (image.width, image.height),
        });
    }

    let mut bytes = Vec::with_capacity(NUM_WALK_FRAMES * FRAME_TILES as usize * 32);
    for frame in 0..NUM_WALK_FRAMES {
        bytes.extend(super::pack_4bpp_region(
            label,
            image,
            frame * FRAME_W,
            0,
            FRAME_W,
            FRAME_H,
        )?);
    }
    Ok(bytes)
}

/// Fill one 16-color palette bank (upstream `PALSLOT_*`) from `raw`'s
/// colors, leaving every other bank untouched -- the shared building block
/// [`super::npc::build_combined_palette`] uses for every one of the scene's
/// five sprite palette banks (bank 0, the player's own; banks 1..=4, the
/// generic `npc_1..4` palettes).
pub(super) fn fill_palette_bank(
    colors: &mut [Bgr555; Palette::LEN],
    bank: usize,
    raw: PaletteRef<'_>,
) {
    let count = usize::from(raw.color_count).min(Palette::BANK_LEN);
    let start = bank * Palette::BANK_LEN;
    for (slot, color) in colors[start..start + Palette::BANK_LEN]
        .iter_mut()
        .zip(raw.colors())
        .take(count)
    {
        *slot = Bgr555::from_raw(color);
    }
}

/// The "standing" tile index for `facing`, plus whether it should be drawn
/// h-flipped (module docs' table): frame 0/1/2 for south/north/west, with
/// east reusing the west frame flipped. Shared by [`frame_for`] (the
/// player, which additionally shows a walk-step frame mid-transit) and
/// [`super::npc::oam_entries`] (every NPC this slice renders, which never
/// walks -- v1's "stationary + look-around only" scope -- so it only ever
/// needs this stand frame).
pub(super) const fn stand_frame_for(facing: Direction) -> (u16, bool) {
    match facing {
        Direction::South => (FRAME_SOUTH_STAND, false),
        Direction::North => (FRAME_NORTH_STAND, false),
        Direction::West => (FRAME_WEST_STAND, false),
        Direction::East => (FRAME_WEST_STAND, true),
    }
}

/// `player`'s current tile index into the 9-frame walking tileset, plus
/// whether it should be drawn h-flipped (module docs' table + cadence).
fn frame_for(player: &PlayerState) -> (u16, bool) {
    let (stand, h_flip) = stand_frame_for(player.facing());
    let step = match player.facing() {
        Direction::South => FRAME_SOUTH_STEP,
        Direction::North => FRAME_NORTH_STEP,
        Direction::West | Direction::East => FRAME_WEST_STEP,
    };
    let frame = if player.in_transit() && player.step_progress() < STEP_FRAME_HALF {
        step
    } else {
        stand
    };
    (frame, h_flip)
}

/// Build the player's OBJ entry for this frame: [`PLAYER_OBJ_X`]/
/// [`PLAYER_OBJ_Y`] (fixed, module docs), the current facing/step frame
/// ([`frame_for`]), always enabled, and an OAM priority selected from the
/// player's retained elevation ([`priority_for_elevation`], issue #218) --
/// not the fixed [`PLAYER_OBJ_PRIORITY`] this entry used before that fix.
pub(super) fn player_entry(player: &PlayerState) -> OamEntry {
    let (frame, h_flip) = frame_for(player);
    OamEntry::new(
        PLAYER_OBJ_X,
        PLAYER_OBJ_Y,
        frame * FRAME_TILES,
        0, // palette bank 0 (see `sprite_palette`).
        BitDepth::Bpp4,
        h_flip,
        false,
        PLAYER_OBJ_SHAPE,
        PLAYER_OBJ_SIZE,
        priority_for_elevation(player.previous_elevation()),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::event_data::EventData;
    use engine::overworld::TilePos;

    /// A fresh event-flag store: nothing hidden. This module's fixture map
    /// carries no object events, so `PlayerState::step`'s object-event
    /// collision check never consults it.
    const NO_FLAGS: EventData = EventData::new();
    use rendering::Tileset;

    #[test]
    fn player_gender_selects_the_matching_character_with_brendan_as_fallback() {
        assert_eq!(
            PlayerCharacter::from(PlayerGender::Male),
            PlayerCharacter::Brendan
        );
        assert_eq!(
            PlayerCharacter::from(PlayerGender::Female),
            PlayerCharacter::May
        );
        assert_eq!(
            PlayerCharacter::from(PlayerGender::Other(7)),
            PlayerCharacter::Brendan
        );
    }

    fn walking_sheet_image(pixels: &[u8]) -> ImageRef<'_> {
        #[allow(clippy::cast_possible_truncation)]
        ImageRef {
            width: (NUM_WALK_FRAMES * FRAME_W) as u32,
            height: FRAME_H as u32,
            bit_depth: 8,
            pixels,
        }
    }

    /// A synthetic 9-frame sheet whose frame `n`'s every pixel is palette
    /// index `n` (mod 16), so which frame a `Tileset::tile` came from is
    /// recoverable from its decoded pixel value.
    fn synthetic_walking_sheet() -> Vec<u8> {
        let mut pixels = vec![0u8; NUM_WALK_FRAMES * FRAME_W * FRAME_H];
        for frame in 0..NUM_WALK_FRAMES {
            #[allow(clippy::cast_possible_truncation)]
            let value = (frame as u8) & 0x0F;
            for y in 0..FRAME_H {
                let row_start = y * (NUM_WALK_FRAMES * FRAME_W) + frame * FRAME_W;
                pixels[row_start..row_start + FRAME_W].fill(value);
            }
        }
        pixels
    }

    #[test]
    fn pack_people_sheet_frames_rejects_the_wrong_sheet_size() {
        let pixels = vec![0u8; 8 * 8];
        let image = ImageRef {
            width: 8,
            height: 8,
            bit_depth: 8,
            pixels: &pixels,
        };
        let err = pack_people_sheet_frames("sprite/*/walking", image).unwrap_err();
        #[allow(clippy::cast_possible_truncation)]
        let expected = OverworldSceneError::SpriteSheetWrongDimensions {
            id: "sprite/*/walking",
            expected: ((NUM_WALK_FRAMES * FRAME_W) as u32, FRAME_H as u32),
            actual: (8, 8),
        };
        assert_eq!(err, expected);
    }

    #[test]
    fn pack_people_sheet_frames_packs_each_frame_into_its_own_tile_range() {
        let pixels = synthetic_walking_sheet();
        let image = walking_sheet_image(&pixels);
        let bytes = pack_people_sheet_frames("sprite/*/walking", image).unwrap();
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        assert_eq!(tileset.len(), NUM_WALK_FRAMES * usize::from(FRAME_TILES));

        // Frame 3's first tile (index 3 * FRAME_TILES) must read frame 3's
        // pixel value (3) at every pixel.
        let tile = tileset.tile(3 * FRAME_TILES).unwrap();
        assert_eq!(tile.index(0, 0), 3);
        assert_eq!(tile.index(7, 7), 3);
    }

    // `fill_palette_bank`'s own per-bank placement is exercised end-to-end by
    // `OverworldScene::from_pack`'s real-pack test (`crate::overworld::tests`)
    // rather than a dedicated unit test here: `assets::PaletteRef`'s payload
    // field is private outside the `assets` crate (only constructible by
    // loading a real pack), so this module cannot hand-build one -- the same
    // limitation `viewport::combined_world_palette`'s own tests already
    // document.

    fn player_at(position: TilePos, facing: Direction) -> PlayerState {
        PlayerState::new(position, 3, facing)
    }

    #[test]
    fn frame_for_selects_the_standing_frame_per_facing_when_idle() {
        assert_eq!(
            frame_for(&player_at((0, 0), Direction::South)),
            (FRAME_SOUTH_STAND, false)
        );
        assert_eq!(
            frame_for(&player_at((0, 0), Direction::North)),
            (FRAME_NORTH_STAND, false)
        );
        assert_eq!(
            frame_for(&player_at((0, 0), Direction::West)),
            (FRAME_WEST_STAND, false)
        );
        assert_eq!(
            frame_for(&player_at((0, 0), Direction::East)),
            (FRAME_WEST_STAND, true),
            "east reuses the west frame, h-flipped"
        );
    }

    #[test]
    fn frame_for_shows_the_forward_foot_for_the_first_half_of_a_step() {
        let (bytes, header, events) = flat_test_map();
        let runtime = engine::overworld::MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            assets::MapLayout {
                id: assets::LayoutId("MAP_TEST"),
                name: "MapTest",
                width: 5,
                height: 5,
                primary_tileset: "gTileset_General",
                secondary_tileset: "gTileset_General",
            }
            .grid(&bytes)
            .unwrap(),
            assets::MetatileAttributeTable::new(&[]),
            assets::MetatileAttributeTable::new(&[]),
        );
        let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

        let mut player = player_at((2, 2), Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            engine::overworld::StepOutcome::Advanced { .. }
        ));
        assert_eq!(frame_for(&player), (FRAME_SOUTH_STEP, false));

        for _ in 0..STEP_FRAME_HALF {
            player.tick();
        }
        assert_eq!(
            frame_for(&player),
            (FRAME_SOUTH_STAND, false),
            "the second half of a step shows the standing frame (fidelity delta, module docs)"
        );
    }

    fn flat_test_map() -> (Vec<u8>, assets::MapHeader, assets::MapEvents) {
        let mut bytes = Vec::new();
        for _ in 0..25 {
            bytes.extend_from_slice(
                &assets::MetatileCell {
                    metatile_id: 0,
                    collision: 0,
                    elevation: 3,
                }
                .pack()
                .to_le_bytes(),
            );
        }
        let header = assets::MapHeader {
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
        };
        let events = assets::MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        (bytes, header, events)
    }

    #[test]
    fn player_entry_uses_the_fixed_screen_position_and_expected_shape() {
        let entry = player_entry(&player_at((0, 0), Direction::South));
        assert_eq!(entry.x(), i16::try_from(PLAYER_OBJ_X).unwrap());
        assert_eq!(entry.y(), PLAYER_OBJ_Y);
        assert_eq!(entry.dimensions(), (16, 32));
        assert_eq!(entry.priority(), PLAYER_OBJ_PRIORITY);
        assert!(entry.enabled());
    }

    /// `sElevationToPriority` transcribed independently from
    /// `event_object_movement.c` (not copy-pasted from
    /// [`ELEVATION_TO_PRIORITY`]'s own module-doc table): every index the
    /// upstream array declares, pinned against this port's own.
    #[test]
    fn priority_for_elevation_matches_the_upstream_selevationtopriority_table() {
        const UPSTREAM: [u8; 16] = [2, 2, 2, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 0, 0, 2];
        for (elevation, &expected) in UPSTREAM.iter().enumerate() {
            assert_eq!(
                priority_for_elevation(u8::try_from(elevation).unwrap()),
                expected,
                "elevation {elevation}"
            );
        }
    }

    /// Out-of-range input (never produced by a real
    /// [`assets::MetatileCell`], whose elevation is a packed 4-bit field,
    /// but not statically excluded by `PlayerState::new`'s bare `u8`
    /// parameter) falls back to the ordinary default rather than panicking.
    #[test]
    fn priority_for_elevation_defaults_out_of_range_input_to_the_ordinary_priority() {
        assert_eq!(priority_for_elevation(16), PLAYER_OBJ_PRIORITY);
        assert_eq!(priority_for_elevation(u8::MAX), PLAYER_OBJ_PRIORITY);
    }

    /// The issue #218 regression: the OAM entry's priority follows the
    /// player's *retained* ([`PlayerState::previous_elevation`]) elevation,
    /// not a fixed constant. `player_at`/`PlayerState::new` set both
    /// `elevation` and `previous_elevation` to the same starting value, so
    /// constructing at 4 alone is enough to exercise the raised case (a
    /// `previous_elevation` that has since drifted from `elevation` across
    /// a transition tile is [`engine::overworld::player`]'s own coverage,
    /// not this OAM-selection layer's).
    #[test]
    fn player_entry_raises_the_oam_priority_on_a_raised_elevation_tile() {
        let on_the_floor = PlayerState::new((0, 0), 3, Direction::South);
        assert_eq!(player_entry(&on_the_floor).priority(), PLAYER_OBJ_PRIORITY);

        let on_the_bed_edge = PlayerState::new((0, 0), 4, Direction::South);
        assert_eq!(
            player_entry(&on_the_bed_edge).priority(),
            1,
            "elevation 4 (the protagonist bedroom bed's raised edge tiles) \
             must draw at the raised priority, not the flat default"
        );
    }
}
