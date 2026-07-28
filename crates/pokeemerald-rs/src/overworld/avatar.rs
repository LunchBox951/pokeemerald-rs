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
use rendering::{Bgr555, BitDepth, OamEntry, ObjShape, Palette, Tileset};

use super::{OverworldSceneError, METATILE_PX, PLAYER_VIEW_COL, PLAYER_VIEW_ROW};

/// One walking-sheet frame's pixel size (module docs' table).
const FRAME_W: usize = 16;
const FRAME_H: usize = 32;
/// `sPicTable_BrendanNormal`/`MayNormal`'s first 9 entries -- the on-foot
/// (not-running) frames this slice uses. `walking.png`'s remaining content
/// (if any) is never referenced (no running in v1 scope).
const NUM_WALK_FRAMES: usize = 9;
/// Tiles per frame: `(FRAME_W / 8) * (FRAME_H / 8)`.
const FRAME_TILES: u16 = 8;

const FRAME_SOUTH_STAND: u16 = 0;
const FRAME_NORTH_STAND: u16 = 1;
const FRAME_WEST_STAND: u16 = 2; // east reuses this, h-flipped.
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
/// `rendering::oam::obj_dimensions`'s table) at `.priority = 2`, the same
/// priority the middle BG layer uses (`super::viewport::MIDDLE_PRIORITY`) --
/// a sprite wins same-priority ties against a BG (`rendering::compositor`'s
/// rules), so the player draws in front of the middle/bottom layers but
/// behind the top one, matching `DrawMetatile`'s own "covers object event
/// sprites" comment on the top layer.
const PLAYER_OBJ_SHAPE: ObjShape = ObjShape::Vertical;
const PLAYER_OBJ_SIZE: u8 = 2;
const PLAYER_OBJ_PRIORITY: u8 = 2;

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
}

/// Build the player's combined 9-frame walking [`Tileset`] out of the pack's
/// `sprite/{brendan,may}/walking` image (module docs' frame table).
///
/// # Errors
///
/// [`OverworldSceneError::SpriteSheetWrongDimensions`] if `image`'s
/// dimensions aren't exactly `NUM_WALK_FRAMES * FRAME_W` x `FRAME_H`
/// (never true for the real upstream art); the same
/// [`OverworldSceneError::ImagePixelCountMismatch`]/
/// [`OverworldSceneError::ImageNotTileAligned`] cases as
/// [`super::pack_4bpp_region`].
pub(super) fn build_sprite_tileset(image: ImageRef<'_>) -> Result<Tileset, OverworldSceneError> {
    let expected_width = u32::try_from(NUM_WALK_FRAMES * FRAME_W).unwrap_or(u32::MAX);
    #[allow(clippy::cast_possible_truncation)] // FRAME_H (32) always fits u32.
    let expected_height = FRAME_H as u32;
    if image.width != expected_width || image.height != expected_height {
        return Err(OverworldSceneError::SpriteSheetWrongDimensions {
            id: "sprite/*/walking",
            expected: (expected_width, expected_height),
            actual: (image.width, image.height),
        });
    }

    let mut bytes = Vec::with_capacity(NUM_WALK_FRAMES * FRAME_TILES as usize * 32);
    for frame in 0..NUM_WALK_FRAMES {
        bytes.extend(super::pack_4bpp_region(
            "sprite/*/walking",
            image,
            frame * FRAME_W,
            0,
            FRAME_W,
            FRAME_H,
        )?);
    }
    Tileset::decode(BitDepth::Bpp4, &bytes).map_err(OverworldSceneError::from)
}

/// Build the player's dedicated sprite [`Palette`]: `raw`'s colors flat at
/// bank 0 (the only bank [`player_entry`]'s OAM entries reference).
pub(super) fn sprite_palette(raw: PaletteRef<'_>) -> Palette {
    let mut colors = [Bgr555::default(); Palette::LEN];
    let count = usize::from(raw.color_count).min(Palette::BANK_LEN);
    for (slot, color) in colors[..Palette::BANK_LEN]
        .iter_mut()
        .zip(raw.colors())
        .take(count)
    {
        *slot = Bgr555::from_raw(color);
    }
    Palette::new(colors)
}

/// `player`'s current tile index into the 9-frame walking tileset, plus
/// whether it should be drawn h-flipped (module docs' table + cadence).
fn frame_for(player: &PlayerState) -> (u16, bool) {
    let (stand, step, h_flip) = match player.facing() {
        Direction::South => (FRAME_SOUTH_STAND, FRAME_SOUTH_STEP, false),
        Direction::North => (FRAME_NORTH_STAND, FRAME_NORTH_STEP, false),
        Direction::West => (FRAME_WEST_STAND, FRAME_WEST_STEP, false),
        Direction::East => (FRAME_WEST_STAND, FRAME_WEST_STEP, true),
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
/// ([`frame_for`]), always enabled.
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
        PLAYER_OBJ_PRIORITY,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::overworld::TilePos;

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
    fn build_sprite_tileset_rejects_the_wrong_sheet_size() {
        let pixels = vec![0u8; 8 * 8];
        let image = ImageRef {
            width: 8,
            height: 8,
            bit_depth: 8,
            pixels: &pixels,
        };
        let err = build_sprite_tileset(image).unwrap_err();
        #[allow(clippy::cast_possible_truncation)]
        let expected = OverworldSceneError::SpriteSheetWrongDimensions {
            id: "sprite/*/walking",
            expected: ((NUM_WALK_FRAMES * FRAME_W) as u32, FRAME_H as u32),
            actual: (8, 8),
        };
        assert_eq!(err, expected);
    }

    #[test]
    fn build_sprite_tileset_packs_each_frame_into_its_own_tile_range() {
        let pixels = synthetic_walking_sheet();
        let image = walking_sheet_image(&pixels);
        let tileset = build_sprite_tileset(image).unwrap();
        assert_eq!(tileset.len(), NUM_WALK_FRAMES * usize::from(FRAME_TILES));

        // Frame 3's first tile (index 3 * FRAME_TILES) must read frame 3's
        // pixel value (3) at every pixel.
        let tile = tileset.tile(3 * FRAME_TILES).unwrap();
        assert_eq!(tile.index(0, 0), 3);
        assert_eq!(tile.index(7, 7), 3);
    }

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
            player.step(Some(Direction::South), &runtime, &no_connections),
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
}
