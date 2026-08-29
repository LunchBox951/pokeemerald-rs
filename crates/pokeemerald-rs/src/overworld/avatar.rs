//! Player-avatar sprite packing and per-frame OAM selection.
//!
//! Emerald's `sAnim_Go{South,North,West,East}` sequences in
//! `src/data/object_events/object_event_anims.h` retain their command cursor
//! across steps and alternate forward feet. [`PlayerState`] exposes only the
//! current step's progress, so this renderer always uses the first forward-foot
//! frame.

use assets::{ImageRef, PaletteRef};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::save::PlayerGender;
use rendering::{Bgr555, BitDepth, OamEntry, ObjShape, Palette};

use super::{OverworldSceneError, METATILE_PX, PLAYER_VIEW_COL, PLAYER_VIEW_ROW};

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
const FRAME_SOUTH_STEP: u16 = 3;
const FRAME_NORTH_STEP: u16 = 5;
const FRAME_WEST_STEP: u16 = 7;

const STEP_FRAME_HALF: u8 = WALK_FRAMES_PER_TILE / 2;

pub(super) const PLAYER_OBJ_SHAPE: ObjShape = ObjShape::Vertical;
pub(super) const PLAYER_OBJ_SIZE: u8 = 2;
pub(super) const PLAYER_OBJ_PRIORITY: u8 = 2;
const RAISED_OBJ_PRIORITY: u8 = 1;
const FRONTMOST_OBJ_PRIORITY: u8 = 0;
const PLAYER_PALETTE_BANK: u8 = 0;

const RAISED_ELEVATIONS: [usize; 5] = [4, 6, 8, 10, 12];
const FRONTMOST_ELEVATIONS: [usize; 2] = [13, 14];

/// Emerald's `UpdateObjectEventElevationAndPriority` also selects a
/// subsprite table from the retained elevation. Its 16x32 tables contain one
/// full-size subsprite at the same priority, so one OAM entry is equivalent.
/// The multi-piece tables belong to the separate long-grass field effect
/// (`src/event_object_movement.c:7690-7746`).
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

#[expect(
    clippy::cast_possible_truncation,
    reason = "the visible-screen coordinate is positive and fits u16"
)]
pub(super) const PLAYER_OBJ_X: u16 = (PLAYER_VIEW_COL * METATILE_PX) as u16;
#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "the visible-screen coordinate is positive and fits u8"
)]
pub(super) const PLAYER_OBJ_Y: u8 =
    (PLAYER_VIEW_ROW * METATILE_PX - (FRAME_H as i32 - METATILE_PX)) as u8;

/// Selects the player avatar's sprite sheet and palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerCharacter {
    Brendan,
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
    };
    let frame = if player.in_transit() && player.step_progress() < STEP_FRAME_HALF {
        step
    } else {
        stand
    };
    (frame, h_flip)
}

pub(super) fn player_entry(player: &PlayerState) -> OamEntry {
    let (frame, h_flip) = frame_for(player);
    OamEntry::new(
        PLAYER_OBJ_X,
        PLAYER_OBJ_Y,
        frame * FRAME_TILES,
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
    use engine::overworld::TilePos;

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
            "the second half of a step shows the standing frame"
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

    #[test]
    fn priority_for_elevation_matches_the_upstream_selevationtopriority_table() {
        const EXPECTED_BY_ELEVATION: [u8; 16] = [
            PLAYER_OBJ_PRIORITY,
            PLAYER_OBJ_PRIORITY,
            PLAYER_OBJ_PRIORITY,
            PLAYER_OBJ_PRIORITY,
            RAISED_OBJ_PRIORITY,
            PLAYER_OBJ_PRIORITY,
            RAISED_OBJ_PRIORITY,
            PLAYER_OBJ_PRIORITY,
            RAISED_OBJ_PRIORITY,
            PLAYER_OBJ_PRIORITY,
            RAISED_OBJ_PRIORITY,
            PLAYER_OBJ_PRIORITY,
            RAISED_OBJ_PRIORITY,
            FRONTMOST_OBJ_PRIORITY,
            FRONTMOST_OBJ_PRIORITY,
            PLAYER_OBJ_PRIORITY,
        ];
        for (elevation, &expected) in EXPECTED_BY_ELEVATION.iter().enumerate() {
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
        assert_eq!(player_entry(&on_the_floor).priority(), PLAYER_OBJ_PRIORITY);

        let on_the_bed_edge = PlayerState::new((0, 0), 4, Direction::South);
        assert_eq!(
            player_entry(&on_the_bed_edge).priority(),
            RAISED_OBJ_PRIORITY,
            "elevation 4 (the protagonist bedroom bed's raised edge tiles) \
             must draw at the raised priority, not the flat default"
        );
    }
}
