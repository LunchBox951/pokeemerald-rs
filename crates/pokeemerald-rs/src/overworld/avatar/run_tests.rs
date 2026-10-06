//! Running-sheet frame selection over a held-B crossing.

use super::tests::{flat_runtime, flat_test_map, player_at};
use super::*;
use engine::event_data::EventData;
use engine::overworld::{StepOutcome, RUN_FRAMES_PER_TILE};

const FLAG_SYS_B_DASH: u16 = 0x8C0;

fn no_connections(_: assets::MapId) -> Option<(u16, u16)> {
    None
}

fn shoes() -> EventData {
    let mut data = EventData::new();
    data.flag_set(FLAG_SYS_B_DASH).unwrap();
    data
}

/// Frames drawn over `facing`'s crossings, one `step_with_run` plus `tick` per
/// rendered frame, as the field loop composes them.
fn rendered_run(facing: Direction, crossings: usize) -> Vec<(u16, bool, bool)> {
    let (bytes, header, events) = flat_test_map();
    let runtime = flat_runtime(&bytes, &header, &events);
    let data = shoes();
    let mut player = player_at((2, 2), facing);
    let mut rendered = Vec::new();
    for _ in 0..crossings * usize::from(RUN_FRAMES_PER_TILE) {
        player.step_with_run(Some(facing), true, &runtime, &no_connections, &data);
        player.tick();
        let (frame, flip) = frame_for(&player);
        rendered.push((frame, flip, run::draws_running_sheet(&player)));
    }
    rendered
}

/// `sAnim_Run*` shows its leading foot for five frames then the neutral cell
/// for three, alternating the foot per tile; East flips West
/// (`object_event_anims.h:346-379`).
#[test]
fn a_run_crossing_draws_five_foot_frames_then_three_neutral_from_the_running_sheet() {
    for (facing, stand, step, flipped) in [
        (Direction::South, FRAME_SOUTH_STAND, FRAME_SOUTH_STEP, false),
        (Direction::North, FRAME_NORTH_STAND, FRAME_NORTH_STEP, false),
        (Direction::West, FRAME_WEST_STAND, FRAME_WEST_STEP, false),
        (Direction::East, FRAME_WEST_STAND, FRAME_WEST_STEP, true),
    ] {
        let rendered = rendered_run(facing, 1);
        let expected: Vec<_> = [step, step, step, step, step, stand, stand, stand]
            .into_iter()
            .map(|frame| (frame, flipped, true))
            .collect();
        assert_eq!(rendered, expected, "{facing:?}");
    }
}

#[test]
fn consecutive_run_crossings_alternate_the_foot() {
    let rendered = rendered_run(Direction::South, 2);
    assert_eq!(rendered[0].0, FRAME_SOUTH_STEP);
    assert_eq!(rendered[8].0, FRAME_SOUTH_STEP + 1);
}

/// Releasing B at the next tile draws the walking sheet again, and a finished
/// run holds only its neutral running cell until the next poll.
#[test]
fn releasing_b_returns_to_the_walking_sheet_and_standing_frame() {
    let (bytes, header, events) = flat_test_map();
    let runtime = flat_runtime(&bytes, &header, &events);
    let data = shoes();
    let mut player = player_at((2, 1), Direction::South);
    player.step_with_run(
        Some(Direction::South),
        true,
        &runtime,
        &no_connections,
        &data,
    );
    for _ in 0..RUN_FRAMES_PER_TILE {
        player.tick();
    }
    assert!(run::draws_running_sheet(&player), "neutral cell is held");
    assert_eq!(frame_for(&player), (FRAME_SOUTH_STAND, false));

    let outcome = player.step_with_run(
        Some(Direction::South),
        false,
        &runtime,
        &no_connections,
        &data,
    );
    assert!(matches!(outcome, StepOutcome::Advanced { .. }));
    assert!(!run::draws_running_sheet(&player));
    assert_eq!(frame_for(&player).0, FRAME_SOUTH_STEP + 1);

    for _ in 0..engine::overworld::WALK_FRAMES_PER_TILE {
        player.tick();
    }
    player.step_with_run(None, true, &runtime, &no_connections, &data);
    assert!(
        !run::draws_running_sheet(&player),
        "idle poll drops the pose"
    );
    assert_eq!(frame_for(&player), (FRAME_SOUTH_STAND, false));
}

#[test]
fn player_entry_reads_the_running_bank_only_while_running() {
    let (bytes, header, events) = flat_test_map();
    let runtime = flat_runtime(&bytes, &header, &events);
    let data = shoes();
    let run_base = 500;
    let mut player = player_at((2, 2), Direction::North);
    assert_eq!(
        player_entry(&player, run_base).tile_index(),
        FRAME_NORTH_STAND * FRAME_TILES
    );
    player.step_with_run(
        Some(Direction::North),
        true,
        &runtime,
        &no_connections,
        &data,
    );
    assert_eq!(
        player_entry(&player, run_base).tile_index(),
        run_base + FRAME_NORTH_STEP * FRAME_TILES
    );
}

/// Both genders' running sheets pack to the same nine-cell layout as walking,
/// each from its own path.
#[test]
fn both_genders_pack_their_own_running_sheet_into_nine_cells() {
    assert_eq!(
        PlayerCharacter::Brendan.run_sprite_path(),
        "brendan/running"
    );
    assert_eq!(PlayerCharacter::May.run_sprite_path(), "may/running");
    for (index, character) in [PlayerCharacter::Brendan, PlayerCharacter::May]
        .into_iter()
        .enumerate()
    {
        let mut pixels = vec![0u8; NUM_WALK_FRAMES * FRAME_W * FRAME_H];
        let marker = u8::try_from(index + 1).unwrap();
        for y in 0..FRAME_H {
            let start = y * NUM_WALK_FRAMES * FRAME_W + (NUM_WALK_FRAMES - 1) * FRAME_W;
            pixels[start..start + FRAME_W].fill(marker);
        }
        let image = ImageRef {
            width: u32::try_from(NUM_WALK_FRAMES * FRAME_W).unwrap(),
            height: u32::try_from(FRAME_H).unwrap(),
            bit_depth: 8,
            pixels: &pixels,
        };
        let packed = pack_people_sheet_frames(character.run_sprite_path(), image).unwrap();
        assert_eq!(
            packed.len(),
            usize::from(FRAME_BLOCK_TILES) * BitDepth::Bpp4.tile_byte_len()
        );
        let last_cell = packed.len() - usize::from(FRAME_TILES) * BitDepth::Bpp4.tile_byte_len();
        assert!(
            packed[last_cell..].iter().any(|byte| *byte != 0),
            "{character:?}"
        );
        assert!(
            packed[..last_cell].iter().all(|byte| *byte == 0),
            "{character:?}"
        );
    }
}

/// Both genders' real running sheets reach the scene's tileset: the avatar's
/// OAM entry mid-run points at the running cell, which differs from the
/// walking cell of the same frame.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_scenes_draw_the_running_cells_for_both_genders() {
    let pack = assets::pack::AssetPack::load_default().unwrap();
    let (bytes, header, events) = flat_test_map();
    let runtime = flat_runtime(&bytes, &header, &events);
    let data = shoes();
    let map = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    for character in [PlayerCharacter::Brendan, PlayerCharacter::May] {
        let scene = crate::overworld::load_room(map, character, &data).unwrap();
        let mut player = player_at((2, 2), Direction::South);
        player.step_with_run(
            Some(Direction::South),
            true,
            &runtime,
            &no_connections,
            &data,
        );
        player.tick();
        let entries = scene.sprites.entries(&player, &data);
        let drawn = entries
            .iter()
            .find(|entry| {
                entry.x() == i16::try_from(PLAYER_OBJ_X).unwrap() && entry.y() == PLAYER_OBJ_Y
            })
            .expect("the player's entry");
        let run_bytes = pack_people_sheet_frames(
            character.run_sprite_path(),
            pack.sprite(character.run_sprite_path()).unwrap(),
        )
        .unwrap();
        let run_tiles = rendering::Tileset::decode(BitDepth::Bpp4, &run_bytes).unwrap();
        let cell = FRAME_SOUTH_STEP * FRAME_TILES;
        for tile in 0..FRAME_TILES {
            assert_eq!(
                scene.sprites.tiles().tile(drawn.tile_index() + tile),
                run_tiles.tile(cell + tile),
                "{character:?} tile {tile}"
            );
        }
        assert_ne!(
            drawn.tile_index(),
            cell,
            "{character:?} draws beyond the walking bank"
        );
    }
}
