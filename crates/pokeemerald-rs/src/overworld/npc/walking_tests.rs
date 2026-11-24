//! A live normal-speed step draws its interpolated position and its `Go*` animation cell.
//!
//! The sheets are bindings only: the tests read OAM entries, never pixels, so no game art is
//! involved. Cell numbers are sheet cells; a frame's tile is `BASE_TILE` plus its packed rank
//! times the frame's tile count.

use super::sheets::{object_sheet, WalkCycle};
use super::test_support::{admitted, object, open_runtime, DEFAULT_ELEVATION};
use super::*;
use assets::MovementType;
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::Direction;

const BASE_TILE: u16 = 32;
/// Tiles in one 16x32 frame.
const PEOPLE_FRAME_TILES: u16 = 8;
const MOM: &str = "OBJ_EVENT_GFX_MOM";

fn people_bindings() -> HashMap<&'static str, SpriteBinding> {
    HashMap::from([(MOM, SpriteBinding::people(BASE_TILE, 4))])
}

fn mom_at(x: i16, y: i16) -> ObjectEventCollection {
    let events = vec![object(MOM, x, y, MovementType::FaceDown)];
    admitted(Box::leak(events.into_boxed_slice()))
}

fn player_at(tile: (i32, i32)) -> PlayerState {
    PlayerState::new(tile, DEFAULT_ELEVATION, Direction::South)
}

fn drawn(
    objects: &ObjectEventCollection,
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
) -> SheetSprite {
    let entries = oam_entries(objects, bindings, player, &EventData::new());
    assert_eq!(entries.len(), 1);
    entries[0]
}

fn state_of(
    objects: &mut ObjectEventCollection,
) -> &mut engine::overworld::object_event::ObjectEventState {
    objects.get_mut(1).unwrap().state_mut()
}

/// Runs the step until `frames` of its frames have elapsed.
fn run_to(objects: &mut ObjectEventCollection, frames: u8) {
    while state_of(objects).step().unwrap().frames_elapsed() < frames {
        assert!(!state_of(objects).advance_step());
    }
}

/// `(x, y, tile, h_flip)` of an entry.
fn view(entry: SheetSprite) -> (i16, u8, u16, bool) {
    (entry.x(), entry.y(), entry.tile_index(), entry.h_flip())
}

fn people_tile(cell: u16) -> u16 {
    BASE_TILE + cell * PEOPLE_FRAME_TILES
}

/// The NPC starts on (7, 5) beside a player on (5, 5): its resting top-left is (144, 56). A
/// first step starts on command 0, so frames 1 to 8 show the first foot and 9 to 15 the neutral
/// pose; the sixteenth frame lands and the stand cell returns.
#[test]
fn a_first_step_draws_each_intermediate_frame_and_the_landing_for_all_four_directions() {
    // direction, positions at elapsed 1, 8, 9, 15 and landing, the foot, the neutral cell,
    // the stand cell, the flip.
    #[rustfmt::skip]
    let cases = [
        (Direction::South, [(144, 57), (144, 64), (144, 65), (144, 71), (144, 72)], 3, 0, 0, false),
        (Direction::North, [(144, 55), (144, 48), (144, 47), (144, 41), (144, 40)], 5, 1, 1, false),
        (Direction::West,  [(143, 56), (136, 56), (135, 56), (129, 56), (128, 56)], 7, 2, 2, false),
        (Direction::East,  [(145, 56), (152, 56), (153, 56), (159, 56), (160, 56)], 7, 2, 2, true),
    ];
    let bindings = people_bindings();
    let player = player_at((5, 5));
    for (direction, positions, foot, neutral, stand, flip) in cases {
        let mut objects = mom_at(7, 5);
        assert_eq!(
            view(drawn(&objects, &bindings, &player)),
            (144, 56, BASE_TILE, false)
        );
        state_of(&mut objects).begin_normal_walk(direction);
        for (elapsed, expected, cell) in [
            (1, positions[0], foot),
            (8, positions[1], foot),
            (9, positions[2], neutral),
            (15, positions[3], neutral),
        ] {
            run_to(&mut objects, elapsed);
            assert_eq!(
                view(drawn(&objects, &bindings, &player)),
                (expected.0, expected.1, people_tile(cell), flip),
                "{direction:?} elapsed {elapsed}"
            );
        }
        assert!(state_of(&mut objects).advance_step());
        assert_eq!(state_of(&mut objects).step(), None);
        assert_eq!(
            view(drawn(&objects, &bindings, &player)),
            (positions[4].0, positions[4].1, people_tile(stand), flip),
            "{direction:?} landed"
        );
    }
}

#[test]
fn consecutive_steps_alternate_the_foot_and_a_second_step_starts_on_the_other_one() {
    let bindings = people_bindings();
    let player = player_at((5, 5));
    let mut objects = mom_at(7, 5);
    let mut first_frames = Vec::new();
    for _ in 0..3 {
        state_of(&mut objects).begin_normal_walk(Direction::South);
        first_frames.push(drawn(&objects, &bindings, &player).tile_index());
        run_to(&mut objects, 9);
        first_frames.push(drawn(&objects, &bindings, &player).tile_index());
        while !state_of(&mut objects).advance_step() {}
    }
    let cells = [3, 0, 4, 0, 3, 0].map(people_tile);
    assert_eq!(first_frames, cells);
}

#[test]
fn a_face_action_before_the_first_walk_gives_the_second_foot() {
    // The wanderer's face action normalises command 0 to the neutral pose, so its first walk
    // is the foot a patrol takes second (`SetStepAnim`, `event_object_movement.c:4601-4618`).
    let bindings = people_bindings();
    let player = player_at((5, 5));
    let mut objects = mom_at(7, 5);
    state_of(&mut objects).face_in_place();
    state_of(&mut objects).begin_normal_walk(Direction::West);
    assert_eq!(
        drawn(&objects, &bindings, &player).tile_index(),
        people_tile(8)
    );
}

#[test]
fn a_walk_in_place_changes_the_cell_and_not_the_position() {
    let bindings = people_bindings();
    let player = player_at((5, 5));
    let mut objects = mom_at(7, 5);
    state_of(&mut objects).begin_normal_walk_in_place(Direction::North);
    for (elapsed, cell) in [(1, 5), (8, 5), (9, 1), (15, 1)] {
        run_to(&mut objects, elapsed);
        assert_eq!(
            view(drawn(&objects, &bindings, &player)),
            (144, 56, people_tile(cell), false),
            "elapsed {elapsed}"
        );
    }
    assert!(state_of(&mut objects).advance_step());
    state_of(&mut objects).begin_normal_walk_in_place(Direction::North);
    assert_eq!(
        drawn(&objects, &bindings, &player).tile_index(),
        people_tile(6),
        "the next in-place step takes the other foot"
    );
}

#[test]
fn faceplayer_keeps_the_foot_that_follows() {
    let bindings = people_bindings();
    let player = player_at((5, 5));
    let mut objects = mom_at(7, 5);
    state_of(&mut objects).begin_normal_walk(Direction::South);
    while !state_of(&mut objects).advance_step() {}
    state_of(&mut objects).face_with_animation(Direction::South);
    state_of(&mut objects).begin_normal_walk(Direction::South);
    assert_eq!(
        drawn(&objects, &bindings, &player).tile_index(),
        people_tile(4),
        "the second foot: a face action does not restart the animation"
    );
}

#[test]
fn a_step_of_a_graphic_without_a_walk_cycle_moves_but_keeps_its_stand_cell() {
    let sheet = object_sheet("OBJ_EVENT_GFX_PICHU_DOLL").unwrap();
    assert_eq!(sheet.walk, None);
    let binding = SpriteBinding {
        base_tile: 50,
        palette_bank: sheet.palette_bank,
        width: 16,
        height: 16,
        facing: sheet.facing,
        walk: sheet.walk,
        packed_cells: 1,
    };
    let bindings = HashMap::from([("OBJ_EVENT_GFX_PICHU_DOLL", binding)]);
    let events = vec![object(
        "OBJ_EVENT_GFX_PICHU_DOLL",
        7,
        5,
        MovementType::FaceDown,
    )];
    let mut objects = admitted(Box::leak(events.into_boxed_slice()));
    state_of(&mut objects).begin_normal_walk(Direction::East);
    let entry = drawn(&objects, &bindings, &player_at((5, 5)));
    assert_eq!(view(entry), (145, 72, 50, false));
}

/// The ninja boy's 16x16 frame is four tiles and anchors 16 pixels lower than a 16x32 one; its
/// walk uses the people cells.
#[test]
fn the_ninja_boy_walks_the_people_cells_in_a_sixteen_by_sixteen_frame() {
    let sheet = object_sheet("OBJ_EVENT_GFX_NINJA_BOY").unwrap();
    assert_eq!(sheet.walk, Some(WalkCycle::PEOPLE));
    assert_eq!(sheet.reachable_cells(), 0x1FF);
    let binding = SpriteBinding {
        base_tile: BASE_TILE,
        palette_bank: sheet.palette_bank,
        width: 16,
        height: 16,
        facing: sheet.facing,
        walk: sheet.walk,
        packed_cells: sheet.reachable_cells(),
    };
    let bindings = HashMap::from([("OBJ_EVENT_GFX_NINJA_BOY", binding)]);
    let events = vec![object(
        "OBJ_EVENT_GFX_NINJA_BOY",
        7,
        5,
        MovementType::FaceDown,
    )];
    let mut objects = admitted(Box::leak(events.into_boxed_slice()));
    let player = player_at((5, 5));
    assert_eq!(
        view(drawn(&objects, &bindings, &player)),
        (144, 72, BASE_TILE, false)
    );

    state_of(&mut objects).begin_normal_walk(Direction::East);
    for (elapsed, x, cell) in [(1, 145, 7), (8, 152, 7), (9, 153, 2)] {
        run_to(&mut objects, elapsed);
        assert_eq!(
            view(drawn(&objects, &bindings, &player)),
            (x, 72, BASE_TILE + cell * 4, true),
            "elapsed {elapsed}"
        );
    }
}

/// The carrying Vigoroth shows source cells 1, 0, 2, 0 whatever it faces, 32x32 and sixteen
/// tiles a frame; east flips.
#[test]
fn the_carrying_vigoroth_walks_cells_one_zero_two_zero_and_flips_east() {
    let sheet = object_sheet("OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX").unwrap();
    assert_eq!(sheet.reachable_cells(), 0b111);
    let binding = SpriteBinding {
        base_tile: BASE_TILE,
        palette_bank: sheet.palette_bank,
        width: 32,
        height: 32,
        facing: sheet.facing,
        walk: sheet.walk,
        packed_cells: sheet.reachable_cells(),
    };
    let key = "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX";
    let bindings = HashMap::from([(key, binding)]);
    let events = vec![object(key, 7, 5, MovementType::WalkRightAndLeft)];
    let mut objects = admitted(Box::leak(events.into_boxed_slice()));
    let player = player_at((5, 5));
    // Centred on the tile, 8 pixels left of a 16-wide frame, bottom on the tile's bottom; it
    // starts facing east, which flips the stand cell.
    assert_eq!(
        view(drawn(&objects, &bindings, &player)),
        (136, 56, BASE_TILE, true)
    );

    state_of(&mut objects).begin_normal_walk(Direction::East);
    for (elapsed, x, cell) in [(1, 137, 1), (8, 144, 1), (9, 145, 0)] {
        run_to(&mut objects, elapsed);
        assert_eq!(
            view(drawn(&objects, &bindings, &player)),
            (x, 56, BASE_TILE + cell * 16, true),
            "east elapsed {elapsed}"
        );
    }
    while !state_of(&mut objects).advance_step() {}
    assert_eq!(
        view(drawn(&objects, &bindings, &player)),
        (152, 56, BASE_TILE, true)
    );

    // The next leg walks back west on the other foot, cell 2, unflipped.
    state_of(&mut objects).begin_normal_walk(Direction::West);
    for (elapsed, x, cell) in [(1, 151, 2), (9, 143, 0)] {
        run_to(&mut objects, elapsed);
        assert_eq!(
            view(drawn(&objects, &bindings, &player)),
            (x, 56, BASE_TILE + cell * 16, false),
            "west elapsed {elapsed}"
        );
    }
}

/// The step's pixels and the camera's lag add: a walking NPC beside a stepping player sits where
/// a still NPC on its destination would, offset by what it has left to cross.
#[test]
fn the_steps_offset_adds_to_the_camera_lag_of_a_stepping_player() {
    let runtime = open_runtime(12, 12);
    let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };
    let data = EventData::new();
    let bindings = people_bindings();
    let mut walking = mom_at(7, 5);
    state_of(&mut walking).begin_normal_walk(Direction::East);
    run_to(&mut walking, 4);
    let still = mom_at(8, 5);

    let mut player = player_at((5, 5));
    player.step(Some(Direction::South), &runtime, &no_connections, &data);
    for _ in 0..6 {
        player.tick();
    }
    assert!(player.in_transit());
    let live = drawn(&walking, &bindings, &player);
    let reference = drawn(&still, &bindings, &player);
    assert_eq!(live.x(), reference.x() - 12);
    assert_eq!(live.y(), reference.y());
    assert_ne!(reference.y(), 56, "the camera lag is part of the placement");
}

/// Composing again without an update draws the same frame: the animation clock is the step's
/// own, not the call count.
#[test]
fn drawing_twice_without_an_update_draws_the_same_walking_frame() {
    let bindings = people_bindings();
    let player = player_at((5, 5));
    let mut objects = mom_at(7, 5);
    state_of(&mut objects).begin_normal_walk(Direction::East);
    run_to(&mut objects, 10);
    let walking = (154, 56, people_tile(2), true);
    assert_eq!(view(drawn(&objects, &bindings, &player)), walking);
    assert_eq!(view(drawn(&objects, &bindings, &player)), walking);
}

#[test]
fn every_walk_cell_is_packed_and_inside_its_sheet() {
    let directions = [
        Direction::South,
        Direction::North,
        Direction::West,
        Direction::East,
    ];
    let sheets = [
        object_sheet("OBJ_EVENT_GFX_NINJA_BOY").unwrap(),
        object_sheet("OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX").unwrap(),
        ObjectSheet::people("mom", 4),
    ];
    for sheet in sheets {
        let walk = sheet.walk.unwrap();
        let packed = sheet.reachable_cells();
        assert_eq!(
            packed >> sheet.cells,
            0,
            "{}: inside the sheet",
            sheet.sprite_path
        );
        for direction in directions {
            for command in 0..4 {
                let (cell, _) = walk.cell_for(direction, command);
                assert_ne!(packed & (1 << cell), 0, "{}", sheet.sprite_path);
            }
            let (stand, _) = sheet.facing.cell_for(direction);
            assert_ne!(packed & (1 << stand), 0);
        }
    }
    // A short people-layout sheet has no walk cycle: it would index past its last cell.
    for id in ["OBJ_EVENT_GFX_NURSE", "OBJ_EVENT_GFX_MYSTERY_GIFT_MAN"] {
        assert_eq!(object_sheet(id).unwrap().walk, None, "{id}");
    }
}
