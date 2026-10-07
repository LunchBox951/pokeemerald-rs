//! Player-avatar sprite packing and per-frame OAM selection.
//!
//! Nine 16x32 frames per sheet, upstream's `overworld_frame(<pic>, 2, 4, n)`
//! in `object_event_pic_tables.h`; `FRAME_*` follow `object_event_anims.h`.

use assets::{ImageRef, PaletteRef};
use engine::overworld::{Direction, PlayerState, TURN_IN_PLACE_FRAMES};
use engine::save::PlayerGender;
use rendering::{Bgr555, BitDepth, OamEntry, ObjShape, Palette};

mod run;
#[cfg(test)]
mod run_tests;

use super::{OverworldSceneError, METATILE_PX, PLAYER_VIEW_COL, PLAYER_VIEW_ROW, RESTING_SCROLL_Y};

pub(super) const FRAME_W: usize = 16;
pub(super) const FRAME_H: usize = 32;
pub(super) const NUM_WALK_FRAMES: usize = 9;
#[expect(
    clippy::cast_possible_truncation,
    reason = "a 16x32 frame contains eight GBA tiles"
)]
pub(super) const FRAME_TILES: u16 =
    (FRAME_W / BitDepth::TILE_DIM * FRAME_H / BitDepth::TILE_DIM) as u16;
#[expect(
    clippy::cast_possible_truncation,
    reason = "the nine-frame block fits in u16"
)]
pub(super) const FRAME_BLOCK_TILES: u16 = NUM_WALK_FRAMES as u16 * FRAME_TILES;

pub(super) const FRAME_SOUTH_STAND: u16 = 0;
pub(super) const FRAME_NORTH_STAND: u16 = 1;
pub(super) const FRAME_WEST_STAND: u16 = 2;
/// The first forward-foot cell of `sAnim_Go*`/`sAnim_GoFast*`; the second
/// foot is the next cell.
const FRAME_SOUTH_STEP: u16 = 3;
const FRAME_NORTH_STEP: u16 = 5;
const FRAME_WEST_STEP: u16 = 7;

/// A standstill turn runs `sAnim_GoFast*`, whose cells last half as long as
/// `sAnim_Go*`'s: the forward foot, then the standing pose.
const TURN_FRAME_HALF: u8 = TURN_IN_PLACE_FRAMES / 2;

pub(super) const PLAYER_OBJ_SHAPE: ObjShape = ObjShape::Vertical;
pub(super) const PLAYER_OBJ_SIZE: u8 = 2;
pub(super) const PLAYER_OBJ_PRIORITY: u8 = 2;
const RAISED_OBJ_PRIORITY: u8 = 1;
const FRONTMOST_OBJ_PRIORITY: u8 = 0;
const PLAYER_PALETTE_BANK: u8 = 0;

const RAISED_ELEVATIONS: [usize; 5] = [4, 6, 8, 10, 12];
const FRONTMOST_ELEVATIONS: [usize; 2] = [13, 14];

/// Emerald's elevation map selects one-piece 16x32 subsprite tables. Elevations
/// 13 and 14 select empty table 0, whose sprite-buffer fallback copies the plain
/// OAM entry instead. The multi-piece tables belong to the separate long-grass
/// field effect (`src/event_object_movement.c:7690-7705`).
const ELEVATION_TO_PRIORITY: [u8; 16] = {
    let mut priorities = [PLAYER_OBJ_PRIORITY; 16];
    let mut index = 0;
    while index < RAISED_ELEVATIONS.len() {
        priorities[RAISED_ELEVATIONS[index]] = RAISED_OBJ_PRIORITY;
        index += 1;
    }
    let mut index = 0;
    while index < FRONTMOST_ELEVATIONS.len() {
        priorities[FRONTMOST_ELEVATIONS[index]] = FRONTMOST_OBJ_PRIORITY;
        index += 1;
    }
    priorities
};

#[must_use]
pub(super) fn priority_for_elevation(elevation: u8) -> u8 {
    ELEVATION_TO_PRIORITY
        .get(usize::from(elevation))
        .copied()
        .unwrap_or(PLAYER_OBJ_PRIORITY)
}

/// Upstream's `SetSpritePosToMapCoords` keeps the player's own object event at
/// `mapX - gSaveBlock1Ptr->pos.x == 0`: the OBJ stays put, the BG scrolls.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the visible-screen coordinate is positive and fits u16"
)]
pub(super) const PLAYER_OBJ_X: u16 = (PLAYER_VIEW_COL * METATILE_PX) as u16;
/// One metatile above the viewport anchor, minus [`RESTING_SCROLL_Y`]'s
/// resting scroll baseline the BG always carries, so the 32px sprite's
/// lower half covers the tile the player stands on -- at the same screen
/// row the BG itself rests at -- rather than its head (module docs' "camera
/// model" section has the full upstream derivation).
#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "the visible-screen coordinate is positive and fits u8"
)]
pub(super) const PLAYER_OBJ_Y: u8 =
    (PLAYER_VIEW_ROW * METATILE_PX - RESTING_SCROLL_Y - (FRAME_H as i32 - METATILE_PX)) as u8;

/// Selects the player avatar's sprite sheet and palette.
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

pub(super) fn pack_people_sheet_frames(
    label: &'static str,
    image: ImageRef<'_>,
) -> Result<Vec<u8>, OverworldSceneError> {
    let expected_width = u32::try_from(NUM_WALK_FRAMES * FRAME_W).unwrap_or(u32::MAX);
    let expected_height = u32::try_from(FRAME_H).unwrap_or(u32::MAX);
    if image.width != expected_width || image.height != expected_height {
        return Err(OverworldSceneError::SpriteSheetWrongDimensions {
            id: label,
            expected: (expected_width, expected_height),
            actual: (image.width, image.height),
        });
    }

    let mut bytes = Vec::with_capacity(
        NUM_WALK_FRAMES * usize::from(FRAME_TILES) * BitDepth::Bpp4.tile_byte_len(),
    );
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

pub(super) const fn stand_frame_for(facing: Direction) -> (u16, bool) {
    match facing {
        Direction::South => (FRAME_SOUTH_STAND, false),
        Direction::North => (FRAME_NORTH_STAND, false),
        Direction::West => (FRAME_WEST_STAND, false),
        Direction::East => (FRAME_WEST_STAND, true),
    }
}

fn frame_for(player: &PlayerState) -> (u16, bool) {
    let (stand, h_flip) = stand_frame_for(player.facing());
    let step = match player.facing() {
        Direction::South => FRAME_SOUTH_STEP,
        Direction::North => FRAME_NORTH_STEP,
        Direction::West | Direction::East => FRAME_WEST_STEP,
    } + u16::from(player.second_foot_leads());
    let walking_foot_forward = player.slide_pose_held()
        || run::running_foot_forward(player)
        || (player.in_transit()
            && !player.transit_running()
            && (player.transit_animation_disabled()
                || player.step_progress() <= player.transit_duration() / 2));
    let turning_foot_forward = player.turn_frames_remaining() >= TURN_FRAME_HALF
        || (player.bump_active() && player.bump_foot_forward());
    let frame = if walking_foot_forward || turning_foot_forward {
        step
    } else {
        stand
    };
    (frame, h_flip)
}

/// `run_base_tile` is where the player's running sheet starts in the
/// combined tileset; it is drawn from instead of the walking sheet at tile 0
/// while [`run::draws_running_sheet`].
pub(super) fn player_entry(player: &PlayerState, run_base_tile: u16) -> OamEntry {
    let (frame, h_flip) = frame_for(player);
    let sheet_base = if run::draws_running_sheet(player) {
        run_base_tile
    } else {
        0
    };
    OamEntry::new(
        PLAYER_OBJ_X,
        PLAYER_OBJ_Y,
        sheet_base + frame * FRAME_TILES,
        PLAYER_PALETTE_BANK,
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
    use engine::overworld::{TilePos, WALK_FRAMES_PER_TILE};

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
        ImageRef {
            width: u32::try_from(NUM_WALK_FRAMES * FRAME_W).unwrap(),
            height: u32::try_from(FRAME_H).unwrap(),
            bit_depth: 8,
            pixels,
        }
    }

    fn synthetic_walking_sheet() -> Vec<u8> {
        let mut pixels = vec![0u8; NUM_WALK_FRAMES * FRAME_W * FRAME_H];
        for frame in 0..NUM_WALK_FRAMES {
            let value = u8::try_from(frame).unwrap() & 0x0F;
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
        let expected = OverworldSceneError::SpriteSheetWrongDimensions {
            id: "sprite/*/walking",
            expected: (
                u32::try_from(NUM_WALK_FRAMES * FRAME_W).unwrap(),
                u32::try_from(FRAME_H).unwrap(),
            ),
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

        let tile = tileset.tile(FRAME_SOUTH_STEP * FRAME_TILES).unwrap();
        let expected_pixel = u8::try_from(FRAME_SOUTH_STEP).unwrap();
        assert_eq!(tile.index(0, 0), expected_pixel);
        assert_eq!(tile.index(7, 7), expected_pixel);
    }

    pub(super) fn player_at(position: TilePos, facing: Direction) -> PlayerState {
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

    pub(super) fn flat_runtime<'a>(
        bytes: &'a [u8],
        header: &'a assets::MapHeader,
        events: &'a assets::MapEvents,
    ) -> engine::overworld::MapRuntime<'a> {
        engine::overworld::MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            assets::MapLayout {
                id: assets::LayoutId("MAP_TEST"),
                name: "MapTest",
                width: 5,
                height: 5,
                primary_tileset: "gTileset_General",
                secondary_tileset: "gTileset_General",
            }
            .grid(bytes)
            .unwrap(),
            assets::MetatileAttributeTable::new(&[]),
            assets::MetatileAttributeTable::new(&[]),
        )
    }

    /// Each Go pose is held 8 presented frames, ticking before composing
    /// (`sAnim_Go*`, `object_event_anims.h:202-235`).
    #[test]
    fn frame_for_draws_eight_foot_frames_then_eight_standing_frames_in_every_direction() {
        let (bytes, header, events) = flat_test_map();
        let runtime = flat_runtime(&bytes, &header, &events);
        let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

        for (facing, stand, step, flipped) in [
            (Direction::South, FRAME_SOUTH_STAND, FRAME_SOUTH_STEP, false),
            (Direction::North, FRAME_NORTH_STAND, FRAME_NORTH_STEP, false),
            (Direction::West, FRAME_WEST_STAND, FRAME_WEST_STEP, false),
            (Direction::East, FRAME_WEST_STAND, FRAME_WEST_STEP, true),
        ] {
            let mut player = player_at((2, 2), facing);
            let mut rendered = Vec::new();
            for presented in 1..=WALK_FRAMES_PER_TILE {
                let outcome =
                    player.step_with_run(Some(facing), false, &runtime, &no_connections, &NO_FLAGS);
                player.tick();
                if presented == 1 {
                    assert!(matches!(
                        outcome,
                        engine::overworld::StepOutcome::Advanced { .. }
                    ));
                    assert_eq!(player.step_progress(), 1);
                }
                rendered.push(frame_for(&player));
            }
            let expected: Vec<_> = [(step, flipped); 8]
                .into_iter()
                .chain([(stand, flipped); 8])
                .collect();
            assert_eq!(rendered, expected, "{facing:?}");
            assert!(
                !player.in_transit(),
                "{facing:?}: frame 16 completes the crossing"
            );
        }
    }

    /// Consecutive steps alternate the forward foot in every facing, East
    /// mirroring West's cells (`sAnim_Go*`, `object_event_anims.h:202-272`).
    #[test]
    fn frame_for_alternates_the_forward_foot_across_consecutive_steps() {
        let (bytes, header, events) = flat_test_map();
        let runtime = flat_runtime(&bytes, &header, &events);
        let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

        for (facing, first, flipped) in [
            (Direction::South, FRAME_SOUTH_STEP, false),
            (Direction::North, FRAME_NORTH_STEP, false),
            (Direction::West, FRAME_WEST_STEP, false),
            (Direction::East, FRAME_WEST_STEP, true),
        ] {
            let mut player = player_at((2, 2), facing);
            let mut forward_feet = Vec::new();
            for _ in 0..2 {
                player.step(Some(facing), &runtime, &no_connections, &NO_FLAGS);
                forward_feet.push(frame_for(&player));
                for _ in 0..WALK_FRAMES_PER_TILE {
                    player.tick();
                }
            }
            assert_eq!(
                forward_feet,
                [(first, flipped), (first + 1, flipped)],
                "{facing:?}"
            );
        }
    }

    /// A 5x5 map whose `(2, 2)` tile is `MB_SLIDE_EAST`, otherwise plain
    /// ground, for proving [`frame_for`] reads the active crossing's own
    /// duration rather than assuming [`WALK_FRAMES_PER_TILE`].
    fn slide_runtime() -> engine::overworld::MapRuntime<'static> {
        let mut bytes = Vec::new();
        for y in 0..5u16 {
            for x in 0..5u16 {
                bytes.extend_from_slice(
                    &assets::MetatileCell {
                        metatile_id: u16::from((x, y) == (2, 2)),
                        collision: 0,
                        elevation: 3,
                    }
                    .pack()
                    .to_le_bytes(),
                );
            }
        }
        let attrs = [
            u16::from(engine::overworld::metatile_behavior::MB_NORMAL).to_le_bytes(),
            u16::from(engine::overworld::metatile_behavior::MB_SLIDE_EAST).to_le_bytes(),
        ]
        .concat();
        let (_, header, events) = flat_test_map();
        let bytes = Box::leak(bytes.into_boxed_slice());
        let attrs = Box::leak(attrs.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        engine::overworld::MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            assets::MapLayout {
                id: assets::LayoutId("MAP_TEST"),
                name: "MapTest",
                width: 5,
                height: 5,
                primary_tileset: "gTileset_General",
                secondary_tileset: "gTileset_General",
            }
            .grid(bytes)
            .unwrap(),
            assets::MetatileAttributeTable::new(attrs),
            assets::MetatileAttributeTable::new(&[]),
        )
    }

    /// `ForcedMovement_Slide` sets `disableAnim` before `PlayerWalkFast`
    /// (`field_player_avatar.c:526-532`), so the go-fast animation's first,
    /// forward-foot cell is held for the whole slide crossing -- unlike an
    /// ordinary step or a dispatched walk tile, which switch to the
    /// standing frame at their own crossing's halfway point
    /// `(behavioral-fidelity)`.
    #[test]
    fn frame_for_holds_the_forward_foot_for_the_whole_slide_crossing() {
        let runtime = slide_runtime();
        let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

        let mut player = player_at((1, 2), Direction::East);
        assert!(
            matches!(
                player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
                engine::overworld::StepOutcome::Advanced { .. }
            ),
            "fixture precondition: the slide tile is entered like ordinary ground"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(
            matches!(
                player.step(None, &runtime, &no_connections, &NO_FLAGS),
                engine::overworld::StepOutcome::Advanced { .. }
            ),
            "fixture precondition: the no-input poll dispatches the slide \
             tile's own crossing"
        );

        for elapsed in 0..engine::overworld::SLIDE_FRAMES_PER_TILE {
            assert_eq!(
                frame_for(&player),
                (FRAME_WEST_STEP + 1, true),
                "slide frame {elapsed} must hold the paused forward-foot pose"
            );
            player.tick();
        }
        assert!(
            !player.in_transit(),
            "fixture precondition: an eight-frame slide crossing must drain \
             in exactly eight frames"
        );
    }

    /// Every presented slide frame, the completing eighth included, holds the
    /// paused forward foot when the tick runs before the render.
    #[test]
    fn frame_for_holds_the_slide_pose_through_tick_before_render() {
        let runtime = slide_runtime();
        let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

        let mut player = player_at((1, 2), Direction::East);
        player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        let mut rendered = Vec::new();
        for _ in 0..engine::overworld::SLIDE_FRAMES_PER_TILE {
            player.step(None, &runtime, &no_connections, &NO_FLAGS);
            player.tick();
            rendered.push(frame_for(&player));
        }
        assert_eq!(
            rendered,
            vec![
                (FRAME_WEST_STEP + 1, true);
                usize::from(engine::overworld::SLIDE_FRAMES_PER_TILE)
            ]
        );
    }

    /// A turn runs `sAnim_GoFast*` from a forward-foot cell for eight frames,
    /// four per cell (`event_object_movement.c:5780-5784`, `object_event_anims.h:238-245`).
    #[test]
    fn frame_for_animates_the_forward_foot_across_the_first_half_of_a_turn() {
        let (bytes, header, events) = flat_test_map();
        let runtime = flat_runtime(&bytes, &header, &events);
        let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

        let mut player = player_at((2, 2), Direction::South);
        let mut rendered = Vec::new();
        for _ in 0..TURN_IN_PLACE_FRAMES {
            player.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS);
            player.tick();
            rendered.push(frame_for(&player));
        }

        assert_eq!(
            rendered,
            vec![
                (FRAME_NORTH_STEP, false),
                (FRAME_NORTH_STEP, false),
                (FRAME_NORTH_STEP, false),
                (FRAME_NORTH_STEP, false),
                (FRAME_NORTH_STAND, false),
                (FRAME_NORTH_STAND, false),
                (FRAME_NORTH_STAND, false),
                (FRAME_NORTH_STAND, false),
            ]
        );
        assert!(
            matches!(
                player.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS),
                engine::overworld::StepOutcome::Advanced { .. }
            ),
            "the turn's animation drains exactly with its busy window"
        );
    }

    /// A blocked step's slow in-place walk shows its forward foot for the
    /// first sixteen presented frames and the standing cell for the rest.
    #[test]
    fn frame_for_animates_a_wall_bump_in_place() {
        let (bytes, header, events) = flat_test_map();
        let runtime = flat_runtime(&bytes, &header, &events);
        let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

        let mut player = player_at((2, 4), Direction::South);
        let mut rendered = Vec::new();
        for _ in 0..engine::overworld::BUMP_IN_PLACE_FRAMES {
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
            player.tick();
            rendered.push(frame_for(&player).0);
        }

        assert!(rendered[..16]
            .iter()
            .all(|&frame| frame == FRAME_SOUTH_STEP));
        assert!(rendered[16..]
            .iter()
            .all(|&frame| frame == FRAME_SOUTH_STAND));
        assert_eq!(player.position(), (2, 4));
    }

    pub(super) fn flat_test_map() -> (Vec<u8>, assets::MapHeader, assets::MapEvents) {
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
        let entry = player_entry(&player_at((0, 0), Direction::South), 0);
        assert_eq!(entry.x(), i16::try_from(PLAYER_OBJ_X).unwrap());
        assert_eq!(entry.y(), PLAYER_OBJ_Y);
        assert_eq!(entry.dimensions(), (16, 32));
        assert_eq!(entry.priority(), PLAYER_OBJ_PRIORITY);
        assert!(entry.enabled());
    }

    /// The crate's public [`super::super::PLAYER_AVATAR_SCREEN_BOX`] --
    /// `xtask`'s smoke map-detail mask is its consumer -- is exactly the box
    /// the player's own OAM entry occupies, so the mask can never name a
    /// rectangle the avatar has moved out of (issue #1013).
    #[test]
    fn public_screen_box_matches_the_player_entry_the_scene_emits() {
        let entry = player_entry(&player_at((0, 0), Direction::South), 0);
        let screen_box = super::super::PLAYER_AVATAR_SCREEN_BOX;
        assert_eq!(
            i16::try_from(screen_box.left).unwrap(),
            entry.x(),
            "the exported box's left edge is the player OBJ's own x"
        );
        assert_eq!(
            u8::try_from(screen_box.top).unwrap(),
            entry.y(),
            "the exported box's top edge is the player OBJ's own y"
        );
        assert_eq!(
            (screen_box.width, screen_box.height),
            entry.dimensions(),
            "the exported box is the player OBJ's own 16x32 shape"
        );
    }

    #[test]
    fn priority_for_elevation_matches_the_upstream_selevationtopriority_table() {
        const UPSTREAM_PRIORITY_BY_ELEVATION: [u8; 16] =
            [2, 2, 2, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 0, 0, 2];
        for (elevation, &expected) in UPSTREAM_PRIORITY_BY_ELEVATION.iter().enumerate() {
            assert_eq!(
                priority_for_elevation(u8::try_from(elevation).unwrap()),
                expected,
                "elevation {elevation}"
            );
        }
    }

    #[test]
    fn priority_for_elevation_defaults_out_of_range_input_to_the_ordinary_priority() {
        assert_eq!(priority_for_elevation(16), PLAYER_OBJ_PRIORITY);
        assert_eq!(priority_for_elevation(u8::MAX), PLAYER_OBJ_PRIORITY);
    }

    #[test]
    fn player_entry_raises_the_oam_priority_on_a_raised_elevation_tile() {
        let on_the_floor = PlayerState::new((0, 0), 3, Direction::South);
        assert_eq!(
            player_entry(&on_the_floor, 0).priority(),
            PLAYER_OBJ_PRIORITY
        );

        let on_the_bed_edge = PlayerState::new((0, 0), 4, Direction::South);
        assert_eq!(
            player_entry(&on_the_bed_edge, 0).priority(),
            RAISED_OBJ_PRIORITY,
            "elevation 4 (the protagonist bedroom bed's raised edge tiles) \
             must draw at the raised priority, not the flat default"
        );
    }
}
