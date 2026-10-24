//! The Route 101 rescue trigger: `VAR_ROUTE101_STATE` bookkeeping, firing, the scripted
//! battle, and the aborted battle.

use super::super::test_support::*;
use super::super::{ActiveBattle, OverworldPhase};
use super::support::*;
use crate::new_game;
use assets::MapId;
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs, MAX_IV};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// `Route101_OnFrame` sets a fresh save's `VAR_ROUTE101_STATE` from 0 to 1 (`data/maps/Route101/scripts.inc:10-17`).
#[test]
fn entering_route_101_bumps_the_fresh_save_rescue_var_to_one() {
    let phase = route_101_trigger_phase(PlayerState::new((0, 0), 3, Direction::East));
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "a fresh save's VAR_ROUTE101_STATE must read 1 on Route 101, not its 0 default"
    );
}

#[test]
fn entering_a_different_map_leaves_the_rescue_var_at_its_fresh_save_zero() {
    let phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(10, 10),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((0, 0), 3, Direction::East),
        None,
    );
    assert_eq!(phase.save1.event_data.var_get(VAR_ROUTE101_STATE), Ok(0));
}

#[test]
fn continuing_on_route_101_only_advances_the_fresh_rescue_state() {
    for (saved_state, expected_state) in [(0, 1), (1, 1), (2, 2), (3, 3)] {
        let (mut block1, block2) = new_game::init_save_blocks_for_new_game();
        block1
            .event_data
            .var_set(VAR_ROUTE101_STATE, saved_state)
            .unwrap();
        let resumed = OverworldPhase::from_saved(
            crate::overworld::test_support::synthetic_scene(25, 25)
                .in_room_for_test(MapId("MAP_ROUTE101")),
            MapId("MAP_ROUTE101"),
            block1,
            block2,
        );
        assert_eq!(
            resumed.save1.event_data.var_get(VAR_ROUTE101_STATE),
            Ok(expected_state),
            "a saved VAR_ROUTE101_STATE of {saved_state} must resume as {expected_state}"
        );
    }
}

/// Stepping onto the rescue tile through `OverworldPhase::step` starts the scripted Zigzagoon battle.
#[test]
fn stepping_onto_the_route_101_trigger_tile_starts_the_scripted_first_battle() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "entering the map already primed the pre-rescue state"
    );

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        !phase.is_wild_battle_active(),
        "this trigger must never route through the ordinary wild-encounter path"
    );
    let Some(ActiveBattle::First(battle)) = phase.active_battle.as_ref() else {
        panic!("the rescue trigger must start the scripted first battle");
    };
    assert_eq!(
        battle.enemy().species(),
        assets::SpeciesId(288),
        "SPECIES_ZIGZAGOON"
    );
    assert_eq!(battle.enemy().level(), 2);
    assert!(
        phase.lead_for_test().is_none(),
        "the lead mon moved into the battle"
    );
}

/// A transition-elevation cell (`0`) wildcards: the trigger fires at the retained elevation
/// (see `MapRuntime::coord_events_at`).
#[test]
fn the_rescue_trigger_fires_at_the_retained_elevation_not_the_transition_cell() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_cell_elevation(
            25,
            25,
            trigger_tile_cell(),
            engine::overworld::collision::ELEVATION_TRANSITION,
        ),
        MapId("MAP_ROUTE101"),
        PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        None,
    );
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert_eq!(
        phase.player.elevation(),
        engine::overworld::collision::ELEVATION_TRANSITION,
        "setup: the landed cell's collision elevation must be the transition value"
    );
    assert_eq!(
        phase.player.previous_elevation(),
        ROUTE_101_TRIGGER_ELEVATION,
        "setup: the retained elevation must still be the ordinary one the player walked in with"
    );
    assert!(
        phase.is_first_battle_active(),
        "the rescue trigger must fire at the retained elevation even though the tile's own \
         collision elevation is the transition value"
    );
}

/// The second rescue coord event at `(11, 19)` fires on the landing call, not the draining one.
#[test]
fn stepping_west_onto_the_second_route_101_trigger_tile_starts_the_scripted_first_battle() {
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (12, 19),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::West,
    ));
    phase.party_lead = Some(new_game::provisional_starter());

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::LEFT));
    }
    assert_eq!(phase.player.position(), (11, 19));
    assert!(
        !phase.is_first_battle_active(),
        "the call that drains the walk animation is upstream's last CB2 animation \
         frame -- nothing has looked at the completed step yet (issue #1039)"
    );

    phase.step(ButtonState::new());
    assert!(
        phase.is_first_battle_active(),
        "the second rescue coord event must start the scripted first battle"
    );
}

/// With no party lead no battle is built, but the trigger still sets the var to 2 (`scripts.inc:40`).
#[test]
fn the_trigger_is_consumed_even_when_there_is_no_party_lead_to_fight_with() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        !phase.is_first_battle_active(),
        "there was no lead mon to build a battle out of"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "the coord event still fired, so the trigger is spent -- upstream's own \
         setvar VAR_ROUTE101_STATE, 2 runs mid-cutscene, long before any battle"
    );
}

#[test]
fn a_step_that_does_not_land_on_the_trigger_tile_starts_nothing() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 2, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx - 1, ty));
    assert!(!phase.is_first_battle_active());
    assert!(!phase.is_wild_battle_active());
}

/// The triggered battle reaches a terminal outcome through `OverworldPhase::step`, hands the lead back,
/// and restarts the animation tick.
///
/// The overworld is checked frozen only while the slot is still full: the emptying frame also
/// concludes the battle, which may warp (pack-dependent; see `first_battle_conclusion_tests`).
#[test]
fn the_scripted_first_battle_plays_to_a_terminal_outcome_and_hands_the_lead_back() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());
    walk_one_tile_east(&mut phase);
    assert!(
        phase.is_first_battle_active(),
        "setup: the trigger must fire"
    );

    phase.tick = 40;
    let frozen_at = phase.player.position();
    let mut frames = 0;
    while phase.is_first_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(
            frames < 500,
            "the headless first-battle driver must terminate"
        );
        if phase.is_first_battle_active() {
            assert_eq!(
                phase.player.position(),
                frozen_at,
                "the overworld is frozen while the scripted first battle runs"
            );
        }
    }
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the battle writes the lead mon back on the frame it ends");
    assert_eq!(lead.species(), new_game::PROVISIONAL_STARTER_SPECIES);
    assert_eq!(
        phase.first_battle_outcome,
        Some(BattleOutcome::PlayerWon),
        "an emptied battle slot must retain the real terminal outcome"
    );
    assert_eq!(
        phase.tick, 0,
        "a completed first battle must restart the tileset animation tick"
    );
    let _ = phase.compose_frame();
    assert_eq!(
        phase.tick, 0,
        "the first composed field frame sees the initial tick"
    );
}

/// After the battle the var no longer matches the trigger, so re-entering the tile starts no battle,
/// rolls no wild encounter and draws nothing.
///
/// The concluded var value is carried to a fresh phase because the conclusion's warp depends on
/// whether a pack is extracted, and this test must pass either way.
#[test]
fn after_the_battle_ends_the_trigger_tile_cannot_refire() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut concluded = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    concluded.rng = Rng::new(4242);
    concluded.party_lead = Some(new_game::provisional_starter());
    walk_one_tile_east(&mut concluded);
    let mut frames = 0;
    while concluded.is_first_battle_active() {
        concluded.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 500, "setup: the driver must terminate");
    }
    let concluded_state = concluded
        .save1
        .event_data
        .var_get(VAR_ROUTE101_STATE)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    assert_eq!(
        concluded_state, 3,
        "setup: the conclusion must have advanced the var to its terminal value"
    );

    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, concluded_state)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    let rng_before = phase.rng.state();
    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        !phase.is_first_battle_active(),
        "the trigger must not refire once VAR_ROUTE101_STATE has advanced"
    );
    assert!(
        !phase.is_wild_battle_active(),
        "the tile must not fall through to an ordinary wild-encounter roll either"
    );
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "ordinary ground (this synthetic room's own default) draws nothing either way"
    );
}

/// The trigger draws from the phase's shared RNG exactly as a direct
/// `start_first_battle` call on an identically seeded generator would.
#[test]
fn the_trigger_draws_off_the_phases_single_shared_stream() {
    const SEED: u32 = 4242;
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(SEED);
    let lead = new_game::provisional_starter();
    phase.party_lead = Some(lead.clone());

    walk_one_tile_east(&mut phase);
    let Some(ActiveBattle::First(battle)) = phase.active_battle.as_ref() else {
        panic!("the trigger must fire");
    };

    // Reference: the same construction off an independent generator with the same seed.
    let mut reference = Rng::new(SEED);
    let player_trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
    let expected =
        crate::flow::first_battle::start_first_battle(lead, player_trainer_id, &mut reference)
            .expect("the same construction must succeed off a reference stream");
    assert_eq!(battle.enemy().personality(), expected.enemy().personality());
    assert_eq!(battle.enemy().ivs(), expected.enemy().ivs());
    assert_eq!(
        phase.rng.state(),
        reference.state(),
        "the trigger must draw exactly as many values as start_first_battle documents -- no \
         more, no fewer"
    );
}

/// An aborted battle (the engine cannot play the lead's only move) writes the lead back with no
/// outcome and still leaves the trigger consumed, matching upstream's trigger-time
/// `setvar VAR_ROUTE101_STATE, 2` (`scripts.inc:40`).
///
/// The lead has full PP, because spent PP would fall back or divert into Struggle
/// (`crates/battle/src/battle.rs:491`) instead of aborting.
#[test]
fn an_aborted_first_battle_still_consumes_the_route_101_trigger() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    // A stale outcome must be cleared by starting this attempt.
    phase.first_battle_outcome = Some(BattleOutcome::PlayerWon);
    let lead = BattlePokemon::new(
        &Dex::new(),
        new_game::PROVISIONAL_STARTER_SPECIES,
        5,
        Ivs {
            hp: MAX_IV,
            attack: MAX_IV,
            defense: MAX_IV,
            speed: MAX_IV,
            sp_attack: MAX_IV,
            sp_defense: MAX_IV,
        },
        0,
        vec![UNEXECUTABLE_MOVE],
    )
    .expect("Treecko/Pursuit must be in the dex");
    assert!(
        lead.moves().iter().all(|slot| slot.pp > 0),
        "setup: an abort here must come from the effect gate, not from spent PP"
    );
    phase.party_lead = Some(lead);

    walk_one_tile_east(&mut phase);
    assert!(
        phase.is_first_battle_active(),
        "setup: the trigger must fire and build a battle -- the abort happens a frame later"
    );
    assert_eq!(
        phase.first_battle_outcome, None,
        "starting a new fight clears the previous terminal outcome"
    );

    phase.tick = 40;
    phase.step(held(Buttons::RIGHT));
    assert!(
        !phase.is_first_battle_active(),
        "setup: the aborted battle must have emptied the slot"
    );
    assert!(
        phase.lead_for_test().is_some(),
        "setup: the abort writes the lead back all the same"
    );
    assert_eq!(
        phase.first_battle_outcome, None,
        "an abort must not manufacture or retain a terminal outcome"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "an abort produces no outcome, and the trigger must be consumed anyway"
    );
    assert_eq!(
        phase.tick, 0,
        "an aborted first battle must restart the tileset animation tick"
    );
    let _ = phase.compose_frame();
    assert_eq!(
        phase.tick, 0,
        "the first composed field frame sees the initial tick"
    );

    phase.player = PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East);
    phase.pending_landing = None;
    let rng_before = phase.rng.state();
    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        !phase.is_first_battle_active(),
        "an aborted battle must not leave the rescue trigger live"
    );
    assert!(!phase.is_wild_battle_active());
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "a spent trigger draws nothing -- not even start_first_battle's construction draws"
    );
}

/// A Truant lead (`battle_util.c:2086-2097`) builds no Zigzagoon and leaves the
/// rescue armed, so `CreateMon`'s draws are not spent on a refused battle.
#[test]
fn an_inadmissible_lead_leaves_the_rescue_armed_without_drawing() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    let slakoth = BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId::SLAKOTH,
        5,
        Ivs {
            hp: MAX_IV,
            attack: MAX_IV,
            defense: MAX_IV,
            speed: MAX_IV,
            sp_attack: MAX_IV,
            sp_defense: MAX_IV,
        },
        0,
        vec![assets::MoveId::SCRATCH],
    )
    .expect("Slakoth/Scratch must be in the dex");
    assert_eq!(
        battle::ensure_participant_admissible(&slakoth),
        Err(battle::BattleError::UnportedAbilityInteraction(
            assets::AbilityId::TRUANT
        )),
    );
    phase.party_lead = Some(slakoth);

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(!phase.is_first_battle_active(), "no battle may start");
    assert_eq!(
        phase.rng.state(),
        Rng::new(4242).state(),
        "an inadmissible lead must not draw at all"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "the rescue stays armed for a lead the engine can fight with"
    );
    assert!(phase.lead_for_test().is_some(), "the lead stays home");
    phase.assert_lead_at_slot_for_test(0);
}

/// A successful first-battle start leaves the lead lent, so a second lend is refused.
#[test]
#[should_panic(expected = "lent out to a battle")]
fn a_started_first_battle_holds_the_lead_on_loan() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.set_fresh_lead_for_test(new_game::provisional_starter());
    walk_one_tile_east(&mut phase);
    assert!(
        matches!(phase.active_battle, Some(ActiveBattle::First(_))),
        "setup: the battle started"
    );
    let _ = phase.take_lead_battler();
}
