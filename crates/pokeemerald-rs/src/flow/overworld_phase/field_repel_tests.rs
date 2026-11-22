//! The Repel step counter (`UpdateRepelCounter`, `pokeemerald/src/wild_encounter.c:850-872`)
//! driven through the production step frame: the per-step countdown, the expiry message and the
//! step it claims. The gate that reads the counter is pinned in
//! `crate::flow::wild_encounter::repel_tests`.

use assets::{MapId, MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Ivs, Status1};
use engine::event_data::VAR_REPEL_STEP_COUNT;
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_CAVE};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::rng::Rng;
use engine::save::{GameStat, Pokemon};
use platform::{ButtonState, Buttons};

use crate::flow::save_continue_support::save_from_the_start_menu;
use crate::flow::tests::TempSave;

use super::test_support::*;
use super::{OverworldPhase, SyntheticStartMenu};

/// `VAR_FRIENDSHIP_STEP_COUNTER`, `VAR_POISON_STEP_COUNTER`
/// (`pokeemerald/include/constants/vars.h:60-61`).
const VAR_FRIENDSHIP_STEP_COUNTER: u16 = 0x402A;
const VAR_POISON_STEP_COUNTER: u16 = 0x402B;
const START_TILE: (i32, i32) = (3, 3);

fn phase_with_repel(steps: u16) -> OverworldPhase {
    let mut phase = synthetic_phase(PlayerState::new(START_TILE, 3, Direction::East), None);
    phase.synthetic_start_menu = SyntheticStartMenu::Builds;
    set_repel(&mut phase, steps);
    phase
}

fn set_repel(phase: &mut OverworldPhase, steps: u16) {
    phase
        .save1
        .event_data
        .var_set(VAR_REPEL_STEP_COUNT, steps)
        .unwrap();
}

fn repel(phase: &OverworldPhase) -> u16 {
    phase
        .save1
        .event_data
        .var_get(VAR_REPEL_STEP_COUNT)
        .unwrap()
}

fn var(phase: &OverworldPhase, id: u16) -> u16 {
    phase.save1.event_data.var_get(id).unwrap()
}

/// One completed step: the player is put back on [`START_TILE`] facing east and walks a tile, the
/// neutral call after it observing the landing.
fn complete_step(phase: &mut OverworldPhase) {
    phase.player = PlayerState::new(START_TILE, 3, Direction::East);
    walk_one_tile_east(phase);
}

/// Closes the open field message the way a player does: A until the box is gone.
fn dismiss_message(phase: &mut OverworldPhase) {
    for _ in 0..600 {
        if !phase.field_message_open_for_test() {
            return;
        }
        phase.step(pressed(Buttons::A));
    }
    panic!("the message box never closed");
}

#[test]
fn a_counted_step_takes_one_off_a_nonzero_counter() {
    let mut phase = phase_with_repel(5);
    let rng = phase.rng.state();

    for expected in [4, 3, 2] {
        complete_step(&mut phase);
        assert_eq!(repel(&phase), expected);
        assert!(!phase.field_message_open_for_test());
    }
    assert_eq!(phase.rng.state(), rng, "the countdown draws nothing");
}

#[test]
fn a_zero_counter_stays_zero_and_the_step_is_otherwise_unchanged() {
    let mut idle = phase_with_repel(0);
    let mut reference = synthetic_phase(PlayerState::new(START_TILE, 3, Direction::East), None);

    complete_step(&mut idle);
    complete_step(&mut reference);

    assert_eq!(repel(&idle), 0);
    assert!(!idle.field_message_open_for_test());
    assert_eq!(idle.player.position(), reference.player.position());
    assert_eq!(idle.rng.state(), reference.rng.state());
    assert_eq!(
        idle.save1.game_stats.get(GameStat::Steps),
        reference.save1.game_stats.get(GameStat::Steps)
    );
}

#[test]
fn only_a_completed_step_counts() {
    let mut phase = phase_with_repel(5);
    for _ in 0..4 {
        phase.step(ButtonState::new());
    }
    assert_eq!(repel(&phase), 5, "standing still counts nothing");

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
        assert_eq!(repel(&phase), 5, "the step is still draining");
    }
    phase.step(ButtonState::new());
    assert_eq!(repel(&phase), 4);
    for _ in 0..4 {
        phase.step(ButtonState::new());
    }
    assert_eq!(repel(&phase), 4, "a landing counts once");
}

/// The step that takes the counter from one to zero claims the step: the message opens, the
/// frame's other field input is dropped, and movement is held.
#[test]
fn the_last_step_shows_the_expiry_message_and_claims_the_step() {
    let mut phase = phase_with_repel(2);
    complete_step(&mut phase);
    assert_eq!(repel(&phase), 1);
    assert!(!phase.field_message_open_for_test());

    complete_step(&mut phase);

    assert_eq!(repel(&phase), 0);
    assert!(phase.field_message_open_for_test(), "the expiry message");
    // While the message is up the held direction moves nothing.
    let at = phase.player.position();
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), at);
    assert!(phase.field_message_open_for_test());

    dismiss_message(&mut phase);
    assert_eq!(repel(&phase), 0, "it does not count again");
    let before = phase.player.position();
    walk_one_tile_east(&mut phase);
    assert_ne!(phase.player.position(), before, "the player walks on");
    assert!(!phase.field_message_open_for_test());
}

/// The claim is the landing call's own: a direction still held on it starts no further step,
/// where the same call with the Repel already spent walks straight on (`PlayerStep` never runs
/// once `ProcessPlayerFieldInput` returns TRUE, `overworld.c:1444-1455`).
#[test]
fn the_expiry_step_starts_no_further_step_from_a_held_direction() {
    fn walk_with_the_direction_held_through_the_landing(repel_steps: u16) -> OverworldPhase {
        let mut phase = phase_with_repel(repel_steps);
        for _ in 0..=WALK_FRAMES_PER_TILE {
            phase.step(held(Buttons::RIGHT));
        }
        phase
    }

    let expired = walk_with_the_direction_held_through_the_landing(1);
    assert_eq!(repel(&expired), 0);
    assert!(!expired.player.in_transit(), "no second step began");
    assert_eq!(expired.player.position(), (START_TILE.0 + 1, START_TILE.1));

    let control = walk_with_the_direction_held_through_the_landing(0);
    assert!(control.player.in_transit(), "the control walks straight on");
}

/// On the expiry step the consumers ahead of `UpdateRepelCounter` in `TryStartStepBasedScript`
/// still run (`field_control_avatar.c:483-496`) and everything after it does not: the step stat,
/// the friendship and poison counters advance, and the frame's same-frame input is discarded.
#[test]
fn the_expiry_step_runs_the_consumers_ahead_of_it_and_none_behind_it() {
    let mut phase = phase_with_repel(1);
    let steps = phase.save1.game_stats.get(GameStat::Steps);

    // The landing call carries an A press and a START press: both are same-frame field input
    // `ProcessPlayerFieldInput` never reaches once the step script claims the frame.
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    let mut landing = ButtonState::new();
    landing.update(Buttons::A | Buttons::START);
    phase.step(landing);

    assert_eq!(phase.save1.game_stats.get(GameStat::Steps), steps + 1);
    assert_eq!(var(&phase, VAR_FRIENDSHIP_STEP_COUNTER), 1);
    assert_eq!(var(&phase, VAR_POISON_STEP_COUNTER), 1);
    assert_eq!(repel(&phase), 0);
    assert!(phase.field_message_open_for_test());
    assert!(
        phase.start_menu().is_none(),
        "the claimed frame opens no menu"
    );
}

fn poisoned_lead(hp: u16) -> Pokemon {
    let dex = Dex::new();
    let ivs = Ivs {
        hp: 15,
        attack: 15,
        defense: 15,
        speed: 15,
        sp_attack: 15,
        sp_defense: 15,
    };
    let mut mon = BattlePokemon::new(&dex, SpeciesId(277), 5, ivs, 1, vec![MoveId::SLASH]).unwrap();
    mon.set_status1(Status1::Poisoned);
    let mut record = crate::party::to_save_pokemon(&dex, &mon);
    record.hp = hp;
    record
}

fn phase_with_poisoned_lead(hp: u16, repel_steps: u16) -> OverworldPhase {
    let mut phase = phase_with_repel(repel_steps);
    phase.save1.player_party[0] = poisoned_lead(hp);
    phase.save1.player_party[1] = poisoned_lead(hp);
    phase.save1.player_party[1].hp = 20;
    phase.save1.player_party_count = 2;
    phase.load_lead_from_save();
    phase
        .save1
        .event_data
        .var_set(VAR_POISON_STEP_COUNTER, 3)
        .unwrap();
    phase
}

/// `UpdatePoisonStepCounter` runs ahead of `UpdateRepelCounter` and a faint claims the step
/// (`field_control_avatar.c:547-553`), so the Repel is not counted on that step.
#[test]
fn a_poison_faint_claims_the_step_before_the_repel_counts() {
    let mut phase = phase_with_poisoned_lead(1, 1);

    complete_step(&mut phase);

    assert_eq!(
        phase.save1.player_party[0].hp, 0,
        "the drain fainted the lead"
    );
    assert_eq!(repel(&phase), 1, "the claimed step never reaches the Repel");
    assert!(!phase.field_message_open_for_test());
}

/// A poison tick that faints nobody claims nothing, so the Repel still counts and still ends.
#[test]
fn a_poison_tick_without_a_faint_leaves_the_repel_to_count() {
    let mut phase = phase_with_poisoned_lead(10, 1);

    complete_step(&mut phase);

    assert_eq!(phase.save1.player_party[0].hp, 9, "the drain ticked");
    assert_eq!(var(&phase, VAR_POISON_STEP_COUNTER), 0);
    assert_eq!(repel(&phase), 0);
    assert!(phase.field_message_open_for_test());
}

/// `InUnionRoom` ends `UpdateRepelCounter` before it reads the counter (`wild_encounter.c:856-857`).
#[test]
fn the_union_room_does_not_count() {
    let mut phase = phase_with_repel(2);
    phase.map_id = MapId("MAP_UNION_ROOM");

    complete_step(&mut phase);

    assert_eq!(repel(&phase), 2);
    assert!(!phase.field_message_open_for_test());
}

/// A landing that fires a door warp is claimed ahead of the step scripts, so it counts no Repel
/// step, though `GAME_STAT_STEPS` still counts it (`field_control_avatar.c:155-163,483-490`).
#[test]
fn a_door_warp_landing_does_not_count() {
    const CAVE: MapId = MapId("MAP_GRANITE_CAVE_B1F");
    const FLOOR: (u16, u16) = (7, 5);
    const DOOR: (u16, u16) = (8, 5);

    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[(FLOOR, MB_CAVE, 3), (DOOR, MB_ANIMATED_DOOR, 0)],
        ),
        CAVE,
        PlayerState::new((6, 5), 3, Direction::East),
        None,
    );
    phase.wild_table_screen = Some((CAVE, false));
    set_repel(&mut phase, 5);
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(held(Buttons::RIGHT));
    assert_eq!(repel(&phase), 4, "the floor landing counts");
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.save1.game_stats.get(GameStat::Steps), 2);
    assert_eq!(repel(&phase), 4, "the door landing reaches no step script");
}

/// The Repel ignores the forced-movement flag that holds poison off: a slide exit counts
/// (`field_control_avatar.c:495`, no `FORCED_MOVE` test in `UpdateRepelCounter`).
#[test]
fn a_slide_exit_landing_counts() {
    let mut phase = phase_on_a_perpendicular_slides_landing_frame();
    set_repel(&mut phase, 3);

    phase.step(ButtonState::new());

    assert_eq!(repel(&phase), 2);
}

/// The counter is the saved variable: a save taken mid-countdown continues counting from the
/// saved value, and the step that reaches zero shows the message in the continued session.
#[test]
fn the_counter_survives_save_and_continue_mid_countdown() {
    let temp = TempSave::new("field-repel-cycle");
    let mut slot = temp.slot();
    let mut phase = phase_with_repel(3);
    complete_step(&mut phase);
    assert_eq!(repel(&phase), 2);
    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    assert_eq!(
        saved.block1.event_data.var_get(VAR_REPEL_STEP_COUNT),
        Ok(2),
        "the mid-countdown value reaches the disk"
    );

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(ONE_F),
        ONE_F,
        saved.block1,
        saved.block2,
    );
    resumed.synthetic_start_menu = SyntheticStartMenu::Builds;
    assert_eq!(repel(&resumed), 2);

    complete_step(&mut resumed);
    assert_eq!(repel(&resumed), 1);
    assert!(!resumed.field_message_open_for_test());
    complete_step(&mut resumed);
    assert_eq!(repel(&resumed), 0);
    assert!(resumed.field_message_open_for_test());
}

/// A walk with a Repel left draws nothing from the generator, from the first step to the last.
#[test]
fn the_whole_countdown_draws_nothing() {
    let mut phase = phase_with_repel(4);
    phase.rng = Rng::new(7);
    let start = phase.rng.state();
    for _ in 0..4 {
        complete_step(&mut phase);
        dismiss_message(&mut phase);
    }
    assert_eq!(repel(&phase), 0);
    assert_eq!(phase.rng.state(), start);
}
