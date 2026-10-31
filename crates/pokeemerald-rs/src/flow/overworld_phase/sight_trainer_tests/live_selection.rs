//! Sight-trainer selection from the phase's live object-event collection:
//! the cone follows the live entry's tile and facing, blockers are live
//! entries, and the approach keeps the selected entry's local ID.

use engine::overworld::{Direction, PlayerState};
use platform::ButtonState;

use super::super::OverworldPhase;
use super::support::*;

/// Berry tree `(58, 5)` on Route 103, local ID 7: an ordinary visible,
/// elevation-3 object event used as a movable blocker.
const BLOCKER_LOCAL_ID: u8 = 7;

/// A Route 103 phase for a player at `player`, with a constructible party so
/// a selected cone starts an approach, and `edit` applied to the live entry
/// with local ID `local_id` before the frame runs.
fn phase_after(
    player: (i32, i32),
    local_id: u8,
    edit: impl FnOnce(&mut engine::overworld::ObjectEventState),
) -> OverworldPhase {
    let mut phase = route_103_phase(PlayerState::new(player, 3, Direction::North));
    phase.set_fresh_lead_for_test(overwhelming_lead());
    phase.synthetic_sight_trainer = Some(assets::trainers::TrainerId(STAND_IN_TRAINER));
    edit(
        phase
            .object_events
            .get_mut(local_id)
            .expect("the Route 103 entry exists")
            .state_mut(),
    );
    phase.step(ButtonState::new());
    phase
}

fn rhett_id() -> u8 {
    rhetts_object_event().local_id
}

/// Untouched live state selects exactly what the template geometry did, and
/// the approach is for that entry, walking from its tile.
#[test]
fn initial_live_state_selects_rhett_and_the_approach_keeps_his_local_id() {
    let (rx, ry) = RHETT_TILE;
    let phase = phase_after((rx, ry + 1), rhett_id(), |_| {});
    let approach = phase.sight_approach.as_ref().expect("Rhett's cone reaches");
    assert_eq!(approach.local_id(), rhett_id());
    assert_eq!(approach.trainer().position(), (67, 5));
}

/// Rhett walked three tiles east: his cone is now `(70, 6..=7)`, and the old
/// `(67, 6)` is no longer in any cone.
#[test]
fn a_live_trainer_tile_moves_the_cone_with_it() {
    let walked = |s: &mut engine::overworld::ObjectEventState| {
        for _ in 0..3 {
            s.walk(Direction::East);
        }
        s.face(Direction::South);
    };
    let old = phase_after((67, 6), rhett_id(), walked);
    assert!(old.sight_approach.is_none(), "(67, 6) left the live cone");

    let new = phase_after((70, 7), rhett_id(), walked);
    let approach = new.sight_approach.as_ref().expect("(70, 7) is in the cone");
    assert_eq!(approach.local_id(), rhett_id());
    assert_eq!(approach.trainer().position(), (70, 5));
}

/// Rhett turned east: `(69, 5)` is now two tiles ahead of him, `(67, 6)` is
/// beside his live line of sight and no longer seen.
#[test]
fn a_live_trainer_facing_turns_the_cone() {
    let turned = |s: &mut engine::overworld::ObjectEventState| s.face(Direction::East);
    let old = phase_after((67, 6), rhett_id(), turned);
    assert!(old.sight_approach.is_none());

    let new = phase_after((69, 5), rhett_id(), turned);
    let approach = new.sight_approach.as_ref().expect("(69, 5) is in the cone");
    assert_eq!(approach.local_id(), rhett_id());
}

/// A live blocker walked onto `(67, 6)` hides a player at `(67, 7)` from
/// Rhett; left at its authored `(58, 5)` it does not. A second trainer at
/// `(67, 9)` facing north also covers `(67, 7)`, so the approach that remains
/// belongs to that other entry.
#[test]
fn a_live_blocker_on_the_line_stops_the_trainer_seeing_the_player() {
    let clear = phase_after((67, 7), BLOCKER_LOCAL_ID, |_| {});
    let approach = clear.sight_approach.as_ref().expect("setup: Rhett sees");
    assert_eq!(approach.local_id(), rhett_id());

    let blocked = phase_after((67, 7), BLOCKER_LOCAL_ID, |s| {
        for _ in 0..9 {
            s.walk(Direction::East);
        }
        s.walk(Direction::South);
    });
    let approach = blocked
        .sight_approach
        .as_ref()
        .expect("the other cone holds");
    assert_ne!(approach.local_id(), rhett_id(), "Rhett's line is blocked");
    assert_eq!(approach.trainer().position(), (67, 9));
}
