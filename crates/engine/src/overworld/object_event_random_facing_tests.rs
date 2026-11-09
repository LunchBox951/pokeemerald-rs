//! Seeded and scripted checks of the two random-facing movement types against the draw order and
//! frame transitions of `event_object_movement.c:2844-2892,3434-3481`.

use assets::{MovementType, TrainerType};

use super::super::Lifecycle;
use crate::overworld::direction::Direction;
use crate::overworld::object_event::{FrameContext, ObjectEventCollection};
use crate::overworld::object_event_harness::{leaked, template, Rig, ScriptedRng};
use crate::rng::{RandomSource, Rng};

const NEVER_HIDDEN: &str = "0";
const SPOT: (i32, i32) = (20, 20);
const FAR: (i32, i32) = (20, 25);

fn rig(types: &[MovementType]) -> Rig {
    let templates = types
        .iter()
        .zip(1u8..)
        .map(|(movement_type, id)| template(id, 20, 20, *movement_type, NEVER_HIDDEN))
        .collect();
    Rig::new(templates, FAR)
}

fn trainer_rig(
    movement_type: MovementType,
    trainer_type: TrainerType,
    range: &'static str,
    player: (i32, i32),
) -> Rig {
    let mut event = template(1, 20, 20, movement_type, NEVER_HIDDEN);
    event.trainer_type = trainer_type;
    event.trainer_sight_or_berry_tree_id = range;
    Rig::new(vec![event], player)
}

/// Runs `frames` frames of a single entry at (20, 20) against any field RNG.
fn run(
    collection: &mut ObjectEventCollection,
    rng: &mut impl RandomSource,
    player: (i32, i32),
    dashing: bool,
    frames: usize,
) {
    let event_data = crate::event_data::EventData::new();
    let terrain = crate::overworld::object_event_harness::Terrain::flat();
    for _ in 0..frames {
        collection.run_frame(
            &FrameContext {
                player_position: player,
                event_data: &event_data,
                frozen: false,
                player_dashing: dashing,
                terrain: terrain.runtime(),
                player_previous_position: player,
                player_elevation: crate::overworld::object_event_harness::FLOOR_ELEVATION,
            },
            rng,
        );
    }
}

fn seeded(movement_type: MovementType, seed: u32) -> (ObjectEventCollection, Rng) {
    let collection = ObjectEventCollection::from_templates(leaked(vec![template(
        1,
        20,
        20,
        movement_type,
        NEVER_HIDDEN,
    )]));
    (collection, Rng::new(seed))
}

fn facing(collection: &ObjectEventCollection, id: u8) -> Direction {
    collection.get(id).unwrap().state().facing()
}

fn phase(collection: &ObjectEventCollection, id: u8) -> u8 {
    collection.get(id).unwrap().type_func_id()
}

fn delay(collection: &ObjectEventCollection, id: u8) -> i16 {
    match collection.get(id).unwrap().lifecycle {
        Lifecycle::Active { delay, .. } => delay,
        other => panic!("not active: {other:?}"),
    }
}

/// How many `next_u16` draws a fresh seeded generator has made when its state is `state`.
fn draws_made(seed: u32, rng: &Rng, state_after: impl Fn(&mut Rng)) -> bool {
    let mut expected = Rng::new(seed);
    state_after(&mut expected);
    expected.next_u16() == rng.clone().next_u16()
}

fn advance(rng: &mut Rng, draws: usize) {
    for _ in 0..draws {
        rng.next_u16();
    }
}

#[test]
fn the_first_frame_runs_steps_0_to_2_and_draws_only_the_delay() {
    let (mut collection, mut rng) = seeded(MovementType::LookAround, 0);
    run(&mut collection, &mut rng, FAR, false, 1);

    // Seed 0 draws 0, 59774, 21105, ...; `0 % 4` selects `sMovementDelaysMedium[0]`.
    assert_eq!(phase(&collection, 1), 3);
    assert_eq!(delay(&collection, 1), 32);
    assert_eq!(facing(&collection, 1), Direction::South);
    assert!(draws_made(0, &rng, |r| advance(r, 1)));
}

#[test]
fn look_around_seed_0_holds_for_the_delay_then_turns_on_the_frame_after() {
    let (mut collection, mut rng) = seeded(MovementType::LookAround, 0);

    run(&mut collection, &mut rng, FAR, false, 32);
    assert_eq!((phase(&collection, 1), delay(&collection, 1)), (3, 1));
    assert_eq!(facing(&collection, 1), Direction::South);
    assert!(draws_made(0, &rng, |r| advance(r, 1)));

    // Frame 33: `59774 & 3 = 2` is West, then `21105 % 4 = 1` loads delay 64.
    run(&mut collection, &mut rng, FAR, false, 1);
    assert_eq!(facing(&collection, 1), Direction::West);
    assert_eq!((phase(&collection, 1), delay(&collection, 1)), (3, 64));
    assert!(draws_made(0, &rng, |r| advance(r, 3)));

    // Frames 34..=96 wait without drawing; frame 97 draws `12720 & 3 = 0` (South), `36418 % 4 = 2`.
    run(&mut collection, &mut rng, FAR, false, 63);
    assert_eq!(facing(&collection, 1), Direction::West);
    assert!(draws_made(0, &rng, |r| advance(r, 3)));
    run(&mut collection, &mut rng, FAR, false, 1);
    assert_eq!(facing(&collection, 1), Direction::South);
    assert_eq!(delay(&collection, 1), 96);
    assert!(draws_made(0, &rng, |r| advance(r, 5)));
}

#[test]
fn face_down_and_right_uses_the_short_delays_and_a_two_way_facing() {
    let (mut collection, mut rng) = seeded(MovementType::FaceDownAndRight, 0);
    run(&mut collection, &mut rng, FAR, false, 1);
    assert_eq!(delay(&collection, 1), 32);

    // `59774 & 1 = 0` is South again; `21105 % 4 = 1` is `sMovementDelaysShort[1]`.
    run(&mut collection, &mut rng, FAR, false, 32);
    assert_eq!(facing(&collection, 1), Direction::South);
    assert_eq!(delay(&collection, 1), 48);
    assert!(draws_made(0, &rng, |r| advance(r, 3)));

    // 48 more frames: the turn is the 49th, drawing `12720 & 1 = 0` and `36418 % 4 = 2` -> 64.
    run(&mut collection, &mut rng, FAR, false, 48);
    assert_eq!(delay(&collection, 1), 64);
    assert!(draws_made(0, &rng, |r| advance(r, 5)));
}

#[test]
fn seed_0x1234_pins_the_longest_delay_boundary_and_each_types_second_facing() {
    // Seed 0x1234 draws 19915, 57697, 17216: `19915 % 4 = 3` is the longest delay of each table.
    for (movement_type, longest, turned) in [
        (MovementType::LookAround, 128, Direction::North),
        (MovementType::FaceDownAndRight, 80, Direction::East),
    ] {
        let (mut collection, mut rng) = seeded(movement_type, 0x1234);
        run(&mut collection, &mut rng, FAR, false, 1);
        assert_eq!(delay(&collection, 1), longest, "{movement_type:?}");

        run(
            &mut collection,
            &mut rng,
            FAR,
            false,
            usize::try_from(longest).unwrap() - 1,
        );
        assert_eq!(
            facing(&collection, 1),
            Direction::South,
            "{movement_type:?}"
        );
        assert!(draws_made(0x1234, &rng, |r| advance(r, 1)));

        // `57697 & 3 = 1` is North (`& 1 = 1` East); `17216 % 4 = 0` loads the shortest delay.
        run(&mut collection, &mut rng, FAR, false, 1);
        assert_eq!(facing(&collection, 1), turned, "{movement_type:?}");
        assert_eq!(delay(&collection, 1), 32, "{movement_type:?}");
        assert!(draws_made(0x1234, &rng, |r| advance(r, 3)));
    }
}

#[test]
fn every_draw_selects_its_table_entries_by_modulo_for_delays_and_mask_for_facings() {
    let medium = [32, 64, 96, 128];
    let short = [32, 48, 64, 80];
    for (movement_type, delays) in [
        (MovementType::LookAround, medium),
        (MovementType::FaceDownAndRight, short),
    ] {
        for (draw, expected) in [0u16, 1, 2, 3, 4, 65_535].into_iter().zip([
            delays[0], delays[1], delays[2], delays[3], delays[0], delays[3],
        ]) {
            let mut rig = rig(&[movement_type]);
            rig.rng = ScriptedRng::new([draw]);
            rig.tick(1);
            assert_eq!(
                delay(&rig.collection, 1),
                expected,
                "{movement_type:?} draw {draw}"
            );
        }
    }

    for (movement_type, directions) in [
        (
            MovementType::LookAround,
            vec![
                Direction::South,
                Direction::North,
                Direction::West,
                Direction::East,
                Direction::South,
                Direction::East,
            ],
        ),
        (
            MovementType::FaceDownAndRight,
            vec![
                Direction::South,
                Direction::East,
                Direction::South,
                Direction::East,
                Direction::South,
                Direction::East,
            ],
        ),
    ] {
        for (draw, expected) in [0u16, 1, 2, 3, 4, 65_535].into_iter().zip(directions) {
            let mut rig = rig(&[movement_type]);
            // Delay 32 (draw 0), then the direction draw, then the next delay (draw 0).
            rig.rng = ScriptedRng::new([0, draw, 0]);
            rig.tick(33);
            assert_eq!(
                facing(&rig.collection, 1),
                expected,
                "{movement_type:?} draw {draw}"
            );
            assert_eq!(rig.rng.draws(), 3);
        }
    }
}

#[test]
fn both_types_share_one_stream_in_collection_order() {
    let mut rig = rig(&[MovementType::LookAround, MovementType::FaceDownAndRight]);
    // Frame 1: A delay 32 (0), B delay 48 (1). Frame 33: A East (3) then 96 (2).
    // Frame 49: B East (1) then 80 (3).
    rig.rng = ScriptedRng::new([0, 1, 3, 2, 1, 3]);

    rig.tick(1);
    assert_eq!(rig.rng.draws(), 2);
    assert_eq!(
        (delay(&rig.collection, 1), delay(&rig.collection, 2)),
        (32, 48)
    );

    rig.tick(32);
    assert_eq!(rig.rng.draws(), 4);
    assert_eq!(facing(&rig.collection, 1), Direction::East);
    assert_eq!(facing(&rig.collection, 2), Direction::South);
    assert_eq!(delay(&rig.collection, 1), 96);

    rig.tick(16);
    assert_eq!(rig.rng.draws(), 6);
    assert_eq!(facing(&rig.collection, 2), Direction::East);
    assert_eq!(delay(&rig.collection, 2), 80);
}

#[test]
fn turning_never_moves_the_object() {
    let mut rig = rig(&[MovementType::LookAround]);
    rig.rng = ScriptedRng::new([0, 3, 0, 2, 0, 1, 0]);
    rig.tick(32 * 4);

    let state = rig.collection.get(1).unwrap().state();
    assert_eq!(state.position(), SPOT);
    assert_eq!(state.previous_position(), SPOT);
    assert_eq!(state.elevation(), 3);
    assert_eq!(state.movement_type(), MovementType::LookAround);
    assert_eq!(rig.rng.draws(), 7);
}

#[test]
fn the_first_callback_settles_a_walked_object_and_keeps_its_current_facing() {
    let mut rig = rig(&[MovementType::LookAround]);
    {
        let state = rig.collection.get_mut(1).unwrap().state_mut();
        state.walk(Direction::East);
    }
    rig.rng = ScriptedRng::new([2]);
    rig.tick(1);

    let state = rig.collection.get(1).unwrap().state();
    assert_eq!(state.facing(), Direction::East);
    assert_eq!(state.position(), (21, 20));
    assert_eq!(state.previous_position(), (21, 20));
    assert_eq!(rig.rng.draws(), 1);
}

#[test]
fn a_frozen_frame_neither_counts_down_nor_draws() {
    let mut rig = rig(&[MovementType::LookAround]);
    rig.rng = ScriptedRng::new([0, 2, 1]);
    rig.tick(32);
    assert_eq!(delay(&rig.collection, 1), 1);

    rig.frozen = true;
    rig.tick(10);
    assert_eq!(delay(&rig.collection, 1), 1);
    assert_eq!(phase(&rig.collection, 1), 3);
    assert_eq!(rig.rng.draws(), 1);

    rig.frozen = false;
    rig.tick(1);
    assert_eq!(facing(&rig.collection, 1), Direction::West);
    assert_eq!(rig.rng.draws(), 3);
}

#[test]
fn an_unadmitted_or_removed_entry_draws_nothing_and_a_respawn_starts_over() {
    let mut rig = rig(&[MovementType::LookAround]);
    rig.player = (20, 80);
    rig.tick(40);
    assert!(!rig.collection.get(1).unwrap().is_active());
    assert!(rig.rng.is_untouched());

    rig.player = FAR;
    rig.rng = ScriptedRng::new([1, 2, 1, 0]);
    // The camera pass admits it at id 0; its first callback is the next frame.
    rig.tick(1);
    assert_eq!(phase(&rig.collection, 1), 0);
    assert!(rig.rng.is_untouched());
    rig.tick(32);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::North);

    // Out of view and back: facing resets from the template and the delay is drawn afresh.
    rig.player = (20, 80);
    rig.tick(1);
    assert!(!rig.collection.get(1).unwrap().is_active());
    rig.player = FAR;
    rig.tick(1);
    assert_eq!(facing(&rig.collection, 1), Direction::South);
    assert_eq!(phase(&rig.collection, 1), 0);
    let before = rig.rng.draws();
    rig.tick(1);
    assert_eq!(rig.rng.draws(), before + 1);
}

#[test]
fn a_changed_movement_type_restarts_the_callback_table() {
    let mut rig = rig(&[MovementType::LookAround]);
    rig.rng = ScriptedRng::new([3]);
    rig.tick(5);
    assert_eq!(delay(&rig.collection, 1), 124);

    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .set_movement_type(MovementType::FaceLeft);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::West);
    rig.tick(3);
    assert_eq!(facing(&rig.collection, 1), Direction::West);
    assert_eq!(phase(&rig.collection, 1), 2);
    assert_eq!(rig.rng.draws(), 1);
}

mod trainer_proximity {
    use super::*;

    /// Frame 1 loads `[3]` -> the longest delay; a dash then wakes the trainer on frame 2.
    fn wake(
        movement_type: MovementType,
        trainer_type: TrainerType,
        range: &'static str,
        player: (i32, i32),
        dashing: bool,
        facing_draw: Option<u16>,
    ) -> Rig {
        let mut rig = trainer_rig(movement_type, trainer_type, range, FAR);
        rig.rng = ScriptedRng::new([3, 1, facing_draw.unwrap_or(0), 0]);
        rig.tick(1);
        rig.player = player;
        rig.dashing = dashing;
        rig.tick(1);
        rig
    }

    #[test]
    fn a_dashing_player_in_range_wakes_the_trainer_without_a_facing_draw() {
        // Frame 2: step 3 predecrements to 127 (awake anyway), step 4 faces the player (East,
        // dx 3 > dy 2) with no draw, step 2 draws the next delay (`1 % 4 = 1` -> 64).
        let rig = wake(
            MovementType::LookAround,
            TrainerType::Normal,
            "3",
            (23, 22),
            true,
            None,
        );
        assert_eq!(facing(&rig.collection, 1), Direction::East);
        assert_eq!(delay(&rig.collection, 1), 64);
        assert_eq!(phase(&rig.collection, 1), 3);
        assert_eq!(rig.rng.draws(), 2);
    }

    #[test]
    fn without_a_dash_the_trainer_keeps_waiting() {
        let rig = wake(
            MovementType::LookAround,
            TrainerType::Normal,
            "3",
            (23, 22),
            false,
            None,
        );
        assert_eq!(facing(&rig.collection, 1), Direction::South);
        assert_eq!(delay(&rig.collection, 1), 127);
        assert_eq!(rig.rng.draws(), 1);
    }

    #[test]
    fn only_normal_and_buried_trainers_in_the_inclusive_range_square_wake() {
        let cases = [
            (TrainerType::Normal, "3", (23, 23), true),
            (TrainerType::Buried, "3", (17, 17), true),
            (TrainerType::Normal, "3", (24, 20), false),
            (TrainerType::Normal, "3", (20, 16), false),
            (TrainerType::None, "3", (21, 20), false),
            (TrainerType::SeeAllDirections, "3", (21, 20), false),
            (TrainerType::Normal, "0", (20, 20), true),
            (TrainerType::Normal, "0", (21, 20), false),
        ];
        for (trainer_type, range, player, wakes) in cases {
            let rig = wake(
                MovementType::LookAround,
                trainer_type,
                range,
                player,
                true,
                Some(2),
            );
            let expected_draws = if wakes { 2 } else { 1 };
            assert_eq!(
                rig.rng.draws(),
                expected_draws,
                "{trainer_type:?} range {range} player {player:?}"
            );
        }
    }

    #[test]
    fn look_around_faces_the_dominant_axis_toward_the_player_with_ties_vertical() {
        let cases = [
            ((23, 23), Direction::South),
            ((17, 17), Direction::North),
            ((20, 20), Direction::South),
            ((17, 19), Direction::West),
            ((22, 21), Direction::East),
            ((19, 17), Direction::North),
        ];
        for (player, expected) in cases {
            let rig = wake(
                MovementType::LookAround,
                TrainerType::Normal,
                "3",
                player,
                true,
                None,
            );
            assert_eq!(facing(&rig.collection, 1), expected, "player {player:?}");
            assert_eq!(rig.rng.draws(), 2, "player {player:?}");
        }
    }

    #[test]
    fn face_down_and_right_never_turns_toward_north_or_west() {
        let cases = [
            ((20, 19), Direction::East),
            ((19, 20), Direction::South),
            ((22, 17), Direction::East),
            ((18, 19), Direction::South),
            ((17, 18), Direction::South),
            ((23, 22), Direction::East),
            ((21, 23), Direction::South),
        ];
        for (player, expected) in cases {
            let rig = wake(
                MovementType::FaceDownAndRight,
                TrainerType::Normal,
                "3",
                player,
                true,
                None,
            );
            assert_eq!(facing(&rig.collection, 1), expected, "player {player:?}");
            assert_eq!(rig.rng.draws(), 2, "player {player:?}");
            // Short delays: `1 % 4 = 1` -> 48.
            assert_eq!(delay(&rig.collection, 1), 48, "player {player:?}");
        }
    }
}

#[test]
fn a_later_admitted_earlier_template_runs_after_the_older_sprite_slot() {
    // A (id 1) sits outside the spawn window; B (id 2) is admitted at map load and so takes the
    // lower sprite slot. Upstream `AnimateSprites` (sprite.c:308-317) runs callbacks by slot, and
    // `CreateSprite` (sprite.c:502-508) gives A the next free slot when it enters view later.
    let mut rig = Rig::new(
        vec![
            template(1, 20, 30, MovementType::LookAround, NEVER_HIDDEN),
            template(2, 20, 20, MovementType::LookAround, NEVER_HIDDEN),
        ],
        (20, 20),
    );
    // Frame 1: B draws 0 -> delay 32. Frames 2..=32 count down.
    rig.rng = ScriptedRng::new([0, 1, 2, 3]);
    rig.tick(31);
    // Frame 32 moves the camera so A is admitted at its end; frame 33 is A's first callback and
    // B's turn frame.
    rig.player = (20, 21);
    rig.tick(1);
    assert_eq!(rig.collection.get(1).unwrap().type_func_id(), 0);
    rig.tick(1);

    // Slot order: B facing `1 & 3` North, B delay `2 % 4` 96, then A delay `3 % 4` 128.
    assert_eq!(facing(&rig.collection, 2), Direction::North);
    assert_eq!(delay(&rig.collection, 2), 96);
    assert_eq!(delay(&rig.collection, 1), 128);
}

#[test]
fn a_sprite_slot_freed_by_a_despawn_goes_to_the_next_admission_ahead_of_older_slots() {
    // Map load gives the leaver (id 1) slot 0 and the stayer (id 2) slot 1; the late entry (id 3)
    // lies just below the window. `RemoveObjectEventInternal` frees slot 0 when the leaver drops
    // out of view, and `CreateSprite`'s lowest-free scan hands it to the late entry next.
    let mut rig = Rig::new(
        vec![
            template(1, 20, 13, MovementType::FaceUp, NEVER_HIDDEN),
            template(2, 20, 20, MovementType::LookAround, NEVER_HIDDEN),
            template(3, 20, 31, MovementType::LookAround, NEVER_HIDDEN),
        ],
        (20, 20),
    );
    // Frame 1: the stayer draws 0 -> delay 32, so frame 33 is its turn.
    rig.rng = ScriptedRng::new([0, 1, 2, 3]);
    rig.tick(30);
    // Frame 31's camera update removes the leaver; frame 32's admits the late entry.
    rig.player = (20, 21);
    rig.tick(1);
    assert!(!rig.collection.get(1).unwrap().is_active());
    rig.player = (20, 22);
    rig.tick(1);
    assert!(rig.collection.get(3).unwrap().is_active());
    rig.tick(1);

    // Slot order: late delay `1 % 4` 64, then stayer facing `2 & 3` West and delay `3 % 4` 128.
    assert_eq!(delay(&rig.collection, 3), 64);
    assert_eq!(facing(&rig.collection, 2), Direction::West);
    assert_eq!(delay(&rig.collection, 2), 128);
}
