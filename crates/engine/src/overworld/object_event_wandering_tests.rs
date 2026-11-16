//! Scripted checks of the wanderers and bounded patrols against the draw order and frame
//! transitions of `event_object_movement.c:2566-2629,2897-3029,3764-3822`.
//!
//! Frames are counted from one: the first `Rig::tick` admits the entries and runs their first
//! callback.

use assets::{MapId, MovementType, ObjectEvent};

use super::super::Lifecycle;
use crate::overworld::direction::Direction;
use crate::overworld::object_event::StepKind;
use crate::overworld::object_event_harness::{template, Rig, ScriptedRng};

const NEVER_HIDDEN: &str = "0";
const FAR: (i32, i32) = (20, 25);
const ORIGIN: (i32, i32) = (20, 20);

fn mover(
    local_id: u8,
    at: (i32, i32),
    movement_type: MovementType,
    range: (u8, u8),
) -> ObjectEvent {
    let mut event = template(
        local_id,
        i16::try_from(at.0).unwrap(),
        i16::try_from(at.1).unwrap(),
        movement_type,
        NEVER_HIDDEN,
    );
    event.movement_range_x = range.0;
    event.movement_range_y = range.1;
    event
}

fn rig_of(movement_type: MovementType, range: (u8, u8), script: &[u16]) -> Rig {
    let mut rig = Rig::new(vec![mover(1, ORIGIN, movement_type, range)], FAR);
    rig.rng = ScriptedRng::new(script.iter().copied());
    rig
}

fn position(rig: &Rig, id: u8) -> (i32, i32) {
    rig.collection.get(id).unwrap().state().position()
}

fn previous(rig: &Rig, id: u8) -> (i32, i32) {
    rig.collection.get(id).unwrap().state().previous_position()
}

fn facing(rig: &Rig, id: u8) -> Direction {
    rig.collection.get(id).unwrap().state().facing()
}

fn phase(rig: &Rig, id: u8) -> u8 {
    rig.collection.get(id).unwrap().type_func_id()
}

fn delay(rig: &Rig, id: u8) -> i16 {
    match rig.collection.get(id).unwrap().lifecycle {
        Lifecycle::Active { delay, .. } => delay,
        other => panic!("not active: {other:?}"),
    }
}

fn sequence_index(rig: &Rig, id: u8) -> u8 {
    match rig.collection.get(id).unwrap().lifecycle {
        Lifecycle::Active { sequence_index, .. } => sequence_index,
        other => panic!("not active: {other:?}"),
    }
}

fn step(rig: &Rig, id: u8) -> Option<(StepKind, u8)> {
    rig.collection
        .get(id)
        .unwrap()
        .state()
        .step()
        .map(|step| (step.kind(), step.frames_elapsed()))
}

/// Runs frames until frame number `frame` (one-based, cumulative) has run.
fn tick_to(rig: &mut Rig, ran: &mut usize, frame: usize) {
    rig.tick(frame - *ran);
    *ran = frame;
}

#[test]
fn a_wanderer_draws_delay_direction_then_delay_on_upstreams_frames() {
    // Draws: delay 32 (frame 1), East (frame 32), delay 32 (frame 48), North (frame 79),
    // delay 32 (frame 95).
    let mut rig = rig_of(MovementType::WanderAround, (2, 2), &[0, 3, 0, 1, 0]);
    let mut ran = 0;

    tick_to(&mut rig, &mut ran, 1);
    assert_eq!(rig.rng.draws(), 1);
    assert_eq!((phase(&rig, 1), delay(&rig, 1)), (3, 31));
    assert_eq!(position(&rig, 1), ORIGIN);
    assert_eq!(step(&rig, 1), None);

    tick_to(&mut rig, &mut ran, 31);
    assert_eq!(rig.rng.draws(), 1, "the direction waits for frame 32");
    assert_eq!((phase(&rig, 1), delay(&rig, 1)), (3, 1));

    tick_to(&mut rig, &mut ran, 32);
    assert_eq!(rig.rng.draws(), 2);
    assert_eq!(position(&rig, 1), (21, 20));
    assert_eq!(previous(&rig, 1), ORIGIN);
    assert_eq!(facing(&rig, 1), Direction::East);
    assert_eq!(step(&rig, 1), Some((StepKind::Walk, 1)));
    assert_eq!(phase(&rig, 1), 6);

    tick_to(&mut rig, &mut ran, 46);
    assert_eq!(step(&rig, 1), Some((StepKind::Walk, 15)));
    assert_eq!(
        previous(&rig, 1),
        ORIGIN,
        "the tile it left is held to the end"
    );

    tick_to(&mut rig, &mut ran, 47);
    assert_eq!(
        step(&rig, 1),
        None,
        "the sixteenth frame completes the walk"
    );
    assert_eq!(previous(&rig, 1), (21, 20));
    assert_eq!(phase(&rig, 1), 1);
    assert_eq!(rig.rng.draws(), 2);

    tick_to(&mut rig, &mut ran, 48);
    assert_eq!(rig.rng.draws(), 3);
    assert_eq!((phase(&rig, 1), delay(&rig, 1)), (3, 31));

    tick_to(&mut rig, &mut ran, 78);
    assert_eq!(rig.rng.draws(), 3);
    tick_to(&mut rig, &mut ran, 79);
    assert_eq!(rig.rng.draws(), 4);
    assert_eq!(position(&rig, 1), (21, 19));
    assert_eq!(facing(&rig, 1), Direction::North);

    tick_to(&mut rig, &mut ran, 94);
    assert_eq!(rig.rng.draws(), 4);
    assert_eq!(step(&rig, 1), None);
    tick_to(&mut rig, &mut ran, 95);
    assert_eq!(rig.rng.draws(), 5);
}

#[test]
fn the_delay_draw_is_the_value_modulo_four_into_the_medium_table() {
    for (draw, expected) in [(0, 32), (1, 64), (2, 96), (3, 128), (4, 32), (65535, 128)] {
        let mut rig = rig_of(MovementType::WanderAround, (2, 2), &[draw]);
        rig.tick(1);
        assert_eq!(
            delay(&rig, 1),
            expected - 1,
            "draw {draw}: the frame that draws also counts down once"
        );
        assert_eq!(rig.rng.draws(), 1);
    }
}

#[test]
fn each_wanderer_maps_its_direction_draw_through_its_own_table() {
    use Direction::{East, North, South, West};
    let up_down = [
        (0, South),
        (1, North),
        (2, South),
        (3, North),
        (65535, North),
    ];
    let left_right = [(0, West), (1, East), (2, West), (3, East), (65535, East)];
    let around = [
        (0, South),
        (1, North),
        (2, West),
        (3, East),
        (4, South),
        (65535, East),
    ];
    let cases: [(MovementType, &[(u16, Direction)]); 5] = [
        (MovementType::WanderAround, &around),
        (MovementType::WanderUpAndDown, &up_down),
        (MovementType::WanderDownAndUp, &up_down),
        (MovementType::WanderLeftAndRight, &left_right),
        (MovementType::WanderRightAndLeft, &left_right),
    ];
    for (movement_type, table) in cases {
        for &(draw, direction) in table {
            let mut rig = rig_of(movement_type, (2, 2), &[0, draw]);
            rig.tick(32);
            assert_eq!(rig.rng.draws(), 2, "{movement_type:?} {draw}");
            assert_eq!(facing(&rig, 1), direction, "{movement_type:?} draw {draw}");
            let (dx, dy) = direction.delta();
            assert_eq!(
                position(&rig, 1),
                (ORIGIN.0 + dx, ORIGIN.1 + dy),
                "{movement_type:?} draw {draw}"
            );
        }
    }
}

#[test]
fn a_blocked_wanderer_turns_without_walking_and_draws_a_fresh_delay() {
    // Frame 32: North (draw 1) is a wall, so the facing changes, table steps 1 and 2 run in
    // the same callback and the next delay (draw 2: 96) is drawn and counted down once.
    let mut rig = rig_of(MovementType::WanderAround, (2, 2), &[0, 1, 2]);
    rig.terrain.set_wall((20, 19));

    rig.tick(32);

    assert_eq!(rig.rng.draws(), 3);
    assert_eq!(position(&rig, 1), ORIGIN);
    assert_eq!(previous(&rig, 1), ORIGIN);
    assert_eq!(facing(&rig, 1), Direction::North);
    assert_eq!(step(&rig, 1), None, "a wanderer never walks in place");
    assert_eq!((phase(&rig, 1), delay(&rig, 1)), (3, 95));
}

type Blocker = fn(&mut Rig);

#[test]
fn a_wanderer_is_refused_by_every_kind_of_blocker() {
    let blockers: [(&str, Blocker); 4] = [
        ("wall", |rig| rig.terrain.set_wall((20, 19))),
        ("elevation", |rig| rig.terrain.set_elevation((20, 19), 5)),
        ("player", |rig| {
            rig.player = (20, 19);
            rig.player_previous = Some((20, 19));
        }),
        ("object", |rig| {
            let mut events = vec![mover(1, ORIGIN, MovementType::WanderAround, (2, 2))];
            events.push(mover(2, (20, 19), MovementType::FaceDown, (0, 0)));
            let script = [0, 1, 2];
            *rig = Rig::new(events, FAR);
            rig.rng = ScriptedRng::new(script);
        }),
    ];
    for (name, block) in blockers {
        let mut rig = rig_of(MovementType::WanderAround, (2, 2), &[0, 1, 2]);
        block(&mut rig);
        rig.tick(32);
        assert_eq!(position(&rig, 1), ORIGIN, "{name}");
        assert_eq!(facing(&rig, 1), Direction::North, "{name}");
        assert_eq!(rig.rng.draws(), 3, "{name}");
    }
}

#[test]
fn a_wanderer_stays_inside_its_range_and_may_walk_back_in() {
    // Range 1 in x: East once, then East again is refused, then West walks back.
    let mut rig = rig_of(MovementType::WanderAround, (1, 1), &[0, 3, 0, 3, 1, 2]);
    let mut ran = 0;

    tick_to(&mut rig, &mut ran, 47);
    assert_eq!(position(&rig, 1), (21, 20));

    // Frame 79: East is outside the range, so it turns in place and draws a delay (64).
    tick_to(&mut rig, &mut ran, 79);
    assert_eq!(rig.rng.draws(), 5);
    assert_eq!(position(&rig, 1), (21, 20));
    assert_eq!(facing(&rig, 1), Direction::East);
    assert_eq!((phase(&rig, 1), delay(&rig, 1)), (3, 63));

    // 63 more frames count the delay down; the direction is drawn on frame 142.
    tick_to(&mut rig, &mut ran, 141);
    assert_eq!(rig.rng.draws(), 5);
    tick_to(&mut rig, &mut ran, 142);
    assert_eq!(rig.rng.draws(), 6);
    assert_eq!(position(&rig, 1), ORIGIN);
    assert_eq!(facing(&rig, 1), Direction::West);
}

#[test]
fn an_authored_zero_range_is_one_at_admission_for_a_type_with_a_range() {
    // Range (0, 0) on a wanderer is one tile each way: South once, South again refused.
    let mut rig = rig_of(MovementType::WanderUpAndDown, (0, 0), &[0, 0, 0, 0, 0]);
    let mut ran = 0;

    tick_to(&mut rig, &mut ran, 47);
    assert_eq!(position(&rig, 1), (20, 21));
    tick_to(&mut rig, &mut ran, 79);
    assert_eq!(position(&rig, 1), (20, 21));
    assert_eq!(rig.rng.draws(), 5);
    assert_eq!(facing(&rig, 1), Direction::South);
}

#[test]
fn a_range_set_before_a_type_change_is_not_raised_to_one() {
    // A fixed-facing object keeps its authored zero, so a script that turns it into a
    // wanderer leaves that axis unbounded (`SetTrainerMovementType` touches no range).
    let mut rig = rig_of(MovementType::FaceUp, (0, 0), &[0, 0, 0, 0, 0, 0, 0, 0]);
    rig.tick(1);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .set_movement_type(MovementType::WanderUpAndDown);

    rig.tick(141);

    assert_eq!(
        position(&rig, 1),
        (20, 23),
        "three steps south, beyond range one"
    );
}

#[test]
fn a_patrol_walks_out_turns_at_its_bound_and_resets_at_its_origin() {
    use Direction::{East, North, South, West};
    let cases = [
        (MovementType::WalkUpAndDown, North),
        (MovementType::WalkDownAndUp, South),
        (MovementType::WalkLeftAndRight, West),
        (MovementType::WalkRightAndLeft, East),
    ];
    for (movement_type, base) in cases {
        let at = |steps: i32| {
            let (dx, dy) = base.delta();
            (ORIGIN.0 + dx * steps, ORIGIN.1 + dy * steps)
        };
        let mut rig = rig_of(movement_type, (2, 2), &[]);
        let mut ran = 0;
        // (frame an action starts on, tile it walks to, direction index, facing)
        let starts = [
            (1, at(1), 0, base),
            (17, at(2), 0, base),
            (33, at(1), 1, base.opposite()),
            (49, at(0), 1, base.opposite()),
            (65, at(1), 0, base),
        ];
        for (frame, tile, index, face) in starts {
            tick_to(&mut rig, &mut ran, frame);
            assert_eq!(position(&rig, 1), tile, "{movement_type:?} frame {frame}");
            assert_eq!(
                sequence_index(&rig, 1),
                index,
                "{movement_type:?} frame {frame}"
            );
            assert_eq!(facing(&rig, 1), face, "{movement_type:?} frame {frame}");
            assert_eq!(step(&rig, 1), Some((StepKind::Walk, 1)));
            tick_to(&mut rig, &mut ran, frame + 15);
            assert_eq!(step(&rig, 1), None, "{movement_type:?} frame {frame}");
            assert_eq!(previous(&rig, 1), tile);
        }
        assert_eq!(rig.rng.draws(), 0, "a patrol draws nothing");
    }
}

#[test]
fn a_blocked_patrol_walks_in_place_and_tries_again_without_reversing() {
    let mut rig = rig_of(MovementType::WalkUpAndDown, (2, 2), &[]);
    rig.terrain.set_wall((20, 19));
    let mut ran = 0;

    for frame in [1, 17, 33] {
        tick_to(&mut rig, &mut ran, frame);
        assert_eq!(step(&rig, 1), Some((StepKind::WalkInPlace, 1)));
        assert_eq!(position(&rig, 1), ORIGIN);
        assert_eq!(facing(&rig, 1), Direction::North);
        assert_eq!(sequence_index(&rig, 1), 0);
        tick_to(&mut rig, &mut ran, frame + 15);
        assert_eq!(step(&rig, 1), None);
    }
}

#[test]
fn a_patrol_that_leaves_its_range_and_finds_the_way_back_blocked_walks_in_place_reversed() {
    let mut rig = rig_of(MovementType::WalkUpAndDown, (1, 1), &[]);
    let mut ran = 0;
    tick_to(&mut rig, &mut ran, 1);
    assert_eq!(position(&rig, 1), (20, 19));
    // Wall the origin the patrol has just left.
    rig.terrain.set_wall(ORIGIN);

    for frame in [17, 33] {
        tick_to(&mut rig, &mut ran, frame);
        assert_eq!(
            step(&rig, 1),
            Some((StepKind::WalkInPlace, 1)),
            "frame {frame}"
        );
        assert_eq!(position(&rig, 1), (20, 19));
        assert_eq!(facing(&rig, 1), Direction::South);
        assert_eq!(sequence_index(&rig, 1), 1, "the outside-range turn is kept");
    }
}

#[test]
fn a_patrol_blocked_by_an_object_walks_in_place_without_reversing() {
    let events = vec![
        mover(1, ORIGIN, MovementType::WalkUpAndDown, (2, 2)),
        mover(2, (20, 19), MovementType::FaceDown, (0, 0)),
    ];
    let mut rig = Rig::new(events, FAR);
    rig.tick(17);
    assert_eq!(position(&rig, 1), ORIGIN);
    assert_eq!(step(&rig, 1), Some((StepKind::WalkInPlace, 1)));
    assert_eq!(facing(&rig, 1), Direction::North);
    assert_eq!(sequence_index(&rig, 1), 0);
}

#[test]
fn an_earlier_sprite_slot_releases_its_old_tile_to_a_later_one_in_the_same_frame() {
    // A patrols west from (21, 20); its walks finish on frames 16, 32, ... Frame 32 releases
    // (20, 20), which B, in the next sprite slot, then walks onto in its own callback.
    let events = vec![
        mover(1, (21, 20), MovementType::WalkLeftAndRight, (2, 2)),
        mover(2, (20, 21), MovementType::WanderAround, (2, 2)),
    ];
    let mut rig = Rig::new(events, (25, 25));
    rig.rng = ScriptedRng::new([0, 1]);

    rig.tick(31);
    assert_eq!(position(&rig, 2), (20, 21));
    assert_eq!(
        previous(&rig, 1),
        (20, 20),
        "A still holds the tile it left"
    );

    rig.tick(1);
    assert_eq!(previous(&rig, 1), (19, 20));
    assert_eq!(position(&rig, 2), (20, 20));
}

#[test]
fn two_wanderers_draw_in_sprite_slot_order_and_see_each_others_moves() {
    let events = vec![
        mover(1, (20, 20), MovementType::WanderAround, (1, 1)),
        mover(2, (22, 20), MovementType::WanderAround, (1, 1)),
    ];
    let mut rig = Rig::new(events, FAR);
    rig.rng = ScriptedRng::new([0, 0, 3, 2, 1, 2]);

    rig.tick(1);
    assert_eq!(rig.rng.draws(), 2);

    // Frame 32: A draws East and commits (21, 20); B then draws West, finds A there, and
    // redraws its delay (draw 1: 64).
    rig.tick(31);
    assert_eq!(rig.rng.draws(), 5);
    assert_eq!(position(&rig, 1), (21, 20));
    assert_eq!(position(&rig, 2), (22, 20));
    assert_eq!(facing(&rig, 2), Direction::West);
    assert_eq!((phase(&rig, 1), phase(&rig, 2)), (6, 3));
    assert_eq!(delay(&rig, 2), 63);

    // Frame 48: A's delay (draw 2: 96); B still counts down from 63.
    rig.tick(16);
    assert_eq!(rig.rng.draws(), 6);
    assert_eq!(delay(&rig, 1), 95);
    assert_eq!(delay(&rig, 2), 47);
}

#[test]
fn callbacks_draw_in_sprite_slot_order_not_template_order() {
    // Template 1 starts outside the view, so template 2 takes the first sprite slot.
    let events = vec![
        mover(1, (20, 30), MovementType::WanderAround, (2, 2)),
        mover(2, ORIGIN, MovementType::WanderAround, (2, 2)),
    ];
    let mut rig = Rig::new(events, (20, 20));
    rig.terrain.set_wall((20, 19));
    rig.rng = ScriptedRng::new([0, 1, 2, 3]);

    rig.tick(30);
    rig.player = (20, 21);
    rig.tick(1);
    assert_eq!(rig.rng.draws(), 1);
    assert!(rig.collection.get(1).unwrap().is_active());

    // Frame 32: B (template 2) draws its direction and blocked redraw before A's first delay.
    rig.tick(1);
    assert_eq!(rig.rng.draws(), 4);
    assert_eq!(facing(&rig, 2), Direction::North);
    assert_eq!(delay(&rig, 2), 95);
    assert_eq!(delay(&rig, 1), 127);
}

#[test]
fn a_frozen_frame_neither_draws_nor_advances_a_walk() {
    let mut rig = rig_of(MovementType::WanderAround, (2, 2), &[0, 3]);
    rig.tick(40);
    let before = (position(&rig, 1), step(&rig, 1), rig.rng.draws());

    rig.frozen = true;
    rig.tick(30);

    assert_eq!((position(&rig, 1), step(&rig, 1), rig.rng.draws()), before);
}

#[test]
fn a_walk_updates_elevation_when_it_begins() {
    let mut rig = rig_of(MovementType::WalkUpAndDown, (2, 2), &[]);
    rig.terrain.set_elevation((20, 19), 0);

    rig.tick(1);

    let state = rig.collection.get(1).unwrap().state();
    assert_eq!(state.position(), (20, 19));
    assert_eq!(state.elevation(), 0);
    assert_eq!(
        state.previous_elevation(),
        3,
        "a transition cell keeps the drawn level"
    );
}

#[test]
fn a_changed_movement_type_restarts_the_patrol_and_its_direction_index() {
    let mut rig = rig_of(MovementType::WalkUpAndDown, (1, 1), &[]);
    rig.tick(17);
    assert_eq!(sequence_index(&rig, 1), 1);

    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .set_movement_type(MovementType::WalkDownAndUp);
    rig.tick(1);

    assert_eq!(sequence_index(&rig, 1), 0);
}

fn real_template(map: &'static str, local_id: u8) -> ObjectEvent {
    let events = assets::MapEventsTable::new()
        .resolve(MapId(map))
        .expect("a bundled map must resolve in the generated table");
    events
        .object_events
        .iter()
        .find(|event| event.local_id == local_id)
        .copied()
        .expect("the checkpoint object is on the map")
}

#[test]
fn route_103s_boy_wanders_inside_his_authored_range() {
    let boy = real_template("MAP_ROUTE103", 10);
    assert_eq!(
        (boy.x, boy.y, boy.movement_type),
        (20, 10, MovementType::WanderAround)
    );
    assert_eq!((boy.movement_range_x, boy.movement_range_y), (1, 2));

    let mut rig = Rig::new(vec![boy], (20, 15));
    rig.rng = ScriptedRng::new([0, 3, 0, 3, 0]);
    rig.tick(47);
    assert_eq!(position(&rig, 10), (21, 10));
    rig.tick(32);
    assert_eq!(rig.rng.draws(), 5, "East again is outside range x = 1");
    assert_eq!(position(&rig, 10), (21, 10));
    assert_eq!(facing(&rig, 10), Direction::East);
}

#[test]
fn route_103s_fisherman_patrols_from_a_zero_x_range() {
    let fisherman = real_template("MAP_ROUTE103", 6);
    assert_eq!(
        (fisherman.x, fisherman.y, fisherman.movement_type),
        (50, 8, MovementType::WalkDownAndUp)
    );
    assert_eq!(
        (fisherman.movement_range_x, fisherman.movement_range_y),
        (0, 1)
    );

    let mut rig = Rig::new(vec![fisherman], (50, 13));
    let mut ran = 0;
    for (frame, tile) in [(1, (50, 9)), (17, (50, 8)), (33, (50, 9))] {
        tick_to(&mut rig, &mut ran, frame);
        assert_eq!(position(&rig, 6), tile, "frame {frame}");
    }
}
