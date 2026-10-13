//! Tests for [`super::first_battle_trigger`]: the Route 101 rescue coord-event
//! trigger, its `VAR_ROUTE101_STATE` bookkeeping, and its precedence over the
//! wild encounter roll.

use super::test_support::*;
use super::{ActiveBattle, OverworldPhase};
use crate::new_game;
use assets::{MapId, MoveId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs, MAX_IV};
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_EAST_ARROW_WARP, MB_NORMAL, MB_TALL_GRASS,
};
use engine::overworld::{
    Direction, MapRuntime, PlayerState, WALK_FRAMES_PER_TILE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// First Route 101 rescue coord event (`Route101/map.json`, elevation 3).
const ROUTE_101_TRIGGER_TILE: (i32, i32) = (10, 19);

const ROUTE_101_TRIGGER_ELEVATION: u8 = 3;

/// Treecko's `MOVE_PURSUIT`: `EFFECT_PURSUIT` has no resolver, so `validate_player_move`
/// refuses it before any draw (`crates/battle/src/battle.rs:415`).
const UNEXECUTABLE_MOVE: MoveId = MoveId::PURSUIT;

/// Open 25x25 synthetic grid at elevation 3 with `MAP_ROUTE101`'s real generated events.
fn route_101_trigger_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(25, 25),
        MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

fn route_101_trigger_phase_with_special_tiles(
    player: PlayerState,
    specials: &[((u16, u16), u8)],
) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles(25, 25, specials),
        MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

fn trigger_tile_cell() -> (u16, u16) {
    let (x, y) = ROUTE_101_TRIGGER_TILE;
    (
        u16::try_from(x).expect("the trigger tile is inside the fixture"),
        u16::try_from(y).expect("the trigger tile is inside the fixture"),
    )
}

fn walk_east(phase: &mut OverworldPhase, tiles: usize) {
    for _ in 0..tiles {
        walk_one_tile_east(phase);
    }
}

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
            crate::overworld::test_support::synthetic_scene(25, 25),
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
        phase.party_lead.is_none(),
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

/// `cross_connection` runs the entry sync, so crossing from Littleroot sets the var to 1 on arrival,
/// before the trigger is tested. Needs the pack: `cross_connection` bails out without it.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_crossing_into_route_101_primes_the_rescue_var_on_arrival() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    // Two steps from the crossing along the walkable `x = 10` column (see `connections_tests`).
    let player = PlayerState::new((10, 1), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, littleroot, player, None);
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(0),
        "setup: a phase built on Littleroot leaves the fresh-save value alone"
    );

    phase.step(held(Buttons::UP));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.player.position(), (10, 0));
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(0),
        "still on Littleroot -- the bump is map-scoped"
    );

    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, route101, "the crossing must have landed");
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "cross_connection runs Route101_OnFrame's own guard on arrival, so the rescue \
         trigger's gate is already open when this step's landing drains"
    );
    assert!(
        !phase.is_first_battle_active(),
        "the crossing step is still mid-animation -- the coord event is only tested once \
         its landing drains"
    );
}

/// Crossing from Littleroot lands on `(10, 19)` (see `connections_tests`), starts the battle, and
/// plays it through `OverworldPhase::step` alone.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let player = PlayerState::new((10, 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, littleroot, player, None);
    // Stands in for the un-ported Birch-bag handout, as `load_default` assigns it.
    phase.party_lead = Some(new_game::provisional_starter());

    for _ in 0..3 {
        walk_one_tile(&mut phase, Buttons::UP);
    }

    assert_eq!(
        phase.map_id, route101,
        "the third step must have crossed into Route 101"
    );
    assert_eq!(
        phase.player.position(),
        (10, 19),
        "Route 101's real south-edge landing tile is the real rescue trigger tile"
    );
    assert!(!phase.is_wild_battle_active());
    let Some(ActiveBattle::First(battle)) = phase.active_battle.as_ref() else {
        panic!("landing on the real trigger tile must start the scripted first battle");
    };
    assert_eq!(battle.enemy().species(), assets::SpeciesId(288));
    assert_eq!(battle.enemy().level(), 2);

    let mut frames = 0;
    while phase.is_first_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(
            frames < 500,
            "the headless first-battle driver must terminate"
        );
    }
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the battle writes the lead mon back once it ends");
    assert_eq!(lead.species(), new_game::PROVISIONAL_STARTER_SPECIES);
    assert!(
        !lead.is_fainted(),
        "issue #251's conclusion heals the lead the instant the battle ends -- see \
         first_battle_conclusion_tests for its own dedicated pins"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(3),
        "the real trigger tile must be consumed once the battle ends, and issue #251's own \
         conclusion advances the var a second time, past this trigger's own \
         TRIGGER_CONSUMED_STATE, to its terminal value"
    );
    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        "the conclusion's own warp MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5 must resolve \
         against the real pack too, reached this time through the real trigger rather than a \
         synthetic Route 101 room (first_battle_conclusion_tests' own real-pack test)"
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
        phase.party_lead.is_some(),
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

/// A fired coord event suppresses the wild roll on its tile: upstream's
/// `TryStartStepBasedScript` returns before `CheckStandardWildEncounter`
/// (`src/field_control_avatar.c:155-162,485-486`).
///
/// The stream position and `prev_metatile_behavior` pin the suppression. The immunity counter
/// cannot, because the battle restart (`src/battle_setup.c:941`) zeroes it either way. The control
/// run proves the grass tile would roll if the trigger were spent.
#[test]
fn the_route_101_trigger_suppresses_the_wild_encounter_roll_on_its_own_tile() {
    // The first rolled grass step encounters with this seed (wild_encounter `ENCOUNTER_SEED`).
    const SEED: u32 = 17;
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let grass = [(trigger_tile_cell(), MB_TALL_GRASS)];

    let mut phase = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 5, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &grass,
    );
    phase.rng = Rng::new(SEED);
    phase.party_lead = Some(new_game::provisional_starter());
    // Four steps spend the immunity window, so the fifth onto the tile would roll.
    walk_east(&mut phase, 4);
    assert_eq!(
        phase.wild.immunity_steps(),
        WILD_ENCOUNTER_IMMUNITY_STEPS,
        "setup: the immunity window must be spent before the trigger step"
    );
    assert_eq!(
        phase.rng.state(),
        Rng::new(SEED).state(),
        "setup: immune steps draw nothing, so the stream is still untouched"
    );

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.is_first_battle_active(),
        "the coord event still fires on a tile that is also grass"
    );
    assert!(
        !phase.is_wild_battle_active(),
        "and the grass roll it outranks must never have happened"
    );
    let mut reference = Rng::new(SEED);
    let player_trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
    crate::flow::first_battle::start_first_battle(
        new_game::provisional_starter(),
        player_trainer_id,
        &mut reference,
    )
    .expect("the same construction must succeed off a reference stream");
    assert_eq!(
        phase.rng.state(),
        reference.state(),
        "the frame's only draws are start_first_battle's -- an encounter roll would add its \
         own AllowWildCheckOnNewMetatile draw ahead of them"
    );
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "CB2_StartFirstBattle's RestartWildEncounterImmunitySteps (battle_setup.c:941) \
         must reset the spent window on the way into the fight (this value cannot \
         distinguish suppression -- the stream assertion above owns that)"
    );
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_NORMAL,
        "sPrevMetatileBehavior must still read the ordinary ground of the step before"
    );

    // Control: with the trigger spent (var 2) the same walk really rolls.
    let mut control = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 5, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &grass,
    );
    control
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    control.rng = Rng::new(SEED);
    control.party_lead = Some(new_game::provisional_starter());
    walk_east(&mut control, 5);
    assert!(
        !control.is_first_battle_active(),
        "control: a spent trigger must not fire"
    );
    assert!(
        control.is_wild_battle_active(),
        "control: the same grass tile really does roll an encounter when nothing outranks it"
    );
    assert_eq!(
        control.wild.prev_metatile_behavior(),
        MB_TALL_GRASS,
        "control: and CheckStandardWildEncounter really does record the tile it saw"
    );
}

/// A forced-movement landing skips the coord-event lookup
/// (`src/field_control_avatar.c:116-122,155-161`).
#[test]
fn a_forced_movement_landing_on_the_trigger_tile_must_not_fire_the_coord_event() {
    use engine::overworld::metatile_behavior::MB_MUDDY_SLOPE;

    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &[(trigger_tile_cell(), MB_MUDDY_SLOPE)],
    );
    phase.party_lead = Some(new_game::provisional_starter());

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        !phase.is_first_battle_active(),
        "a forced-movement landing must not run the coord-event lookup at all"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "the trigger's own setvar never ran, so the rescue var must still read the \
         on-frame guard's 1, not the coord event's 2"
    );
}

/// Door and arrow warps cannot race the coord event (`src/field_control_avatar.c:155-168`) on this map
/// because Route 101 has no warp events. That premise is asserted so a future warp forces a real
/// precedence test; door and arrow behaviors painted on the tile still leave the trigger firing.
#[test]
fn route_101_has_no_warp_events_so_the_trigger_can_never_race_one() {
    let route101 = MapId("MAP_ROUTE101");
    let events = assets::MapEventsTable::new()
        .resolve(route101)
        .expect("Route 101 resolves in the generated map-events table");
    assert!(
        events.warp_events.is_empty(),
        "Route 101 has no warp events, so the door/arrow arms of the trigger's precedence \
         are unreachable over bundled data -- if that ever changes, pin them for real"
    );

    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    for behavior in [MB_ANIMATED_DOOR, MB_EAST_ARROW_WARP] {
        let mut phase = route_101_trigger_phase_with_special_tiles(
            PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
            &[(trigger_tile_cell(), behavior)],
        );
        phase.rng = Rng::new(4242);
        phase.party_lead = Some(new_game::provisional_starter());

        walk_one_tile_east(&mut phase);

        assert_eq!(
            phase.map_id, route101,
            "behavior {behavior:#04x}: no warp may fire, so the map must not change"
        );
        assert_eq!(
            phase.player.position(),
            (tx, ty),
            "behavior {behavior:#04x}: the player stays on the trigger tile"
        );
        assert!(
            phase.is_first_battle_active(),
            "behavior {behavior:#04x}: the coord event fires regardless of the tile's behavior"
        );
    }
}

/// The `PreventExitSouth` coord event at `(10, 18)` must not start a battle. It also triggers on
/// `VAR_ROUTE101_STATE` at elevation 3 with `var_value` 2, so only the `TRIGGER_SCRIPT` name check
/// in `first_battle_trigger_at` stops it.
#[test]
fn the_prevent_exit_coord_events_never_start_a_battle() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    // Starts on the rescue tile: with the var past the ready state no walk onto it is needed.
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::North,
    ));
    // `TRIGGER_CONSUMED_STATE`, set directly: the later conclusion advance to 3 is not this test's subject.
    phase
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    phase.party_lead = Some(new_game::provisional_starter());

    let rng_before = phase.rng.state();
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.player.position(),
        (tx, ty - 1),
        "setup: the step must land on the PreventExitSouth coord event"
    );
    phase.step(ButtonState::new());
    assert!(
        !phase.mid_step(),
        "setup: the landing call must have consumed the completed step"
    );
    assert!(
        !phase.is_first_battle_active(),
        "a PreventExit* coord event is not the rescue trigger, whatever its var says"
    );
    assert!(!phase.is_wild_battle_active());
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "and nothing was constructed off the shared stream either -- start_first_battle \
         draws before it can fail"
    );
}

/// No Route 101 object event neighbors a rescue tile, so the trigger's interaction-discard arm
/// (`src/field_control_avatar.c:172`) is unreachable; a future neighbor forces a real test.
#[test]
fn no_route_101_object_event_stands_beside_the_rescue_trigger_tiles() {
    let events = assets::MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .expect("Route 101 resolves in the generated map-events table");
    for (tx, ty) in [ROUTE_101_TRIGGER_TILE, (11, 19)] {
        for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
            let (nx, ny) = (tx + dx, ty + dy);
            assert!(
                !events
                    .object_events
                    .iter()
                    .any(|o| i32::from(o.x) == nx && i32::from(o.y) == ny),
                "an object event at ({nx}, {ny}) would make the trigger's interaction-discard \
                 arm reachable -- pin it for real instead of relying on this assertion"
            );
        }
    }
}

/// Route 101's coord events never stack, so real data never gives `first_battle_trigger_at` more
/// than one candidate; the stacked-scan tests below fabricate stacks.
#[test]
fn route_101_coord_events_all_sit_at_distinct_positions() {
    let events = assets::MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .expect("Route 101 always has a static events entry");
    let mut positions: Vec<(i16, i16, u8)> = events
        .coord_events
        .iter()
        .map(|e| (e.x, e.y, e.elevation))
        .collect();
    let total = positions.len();
    positions.sort_unstable();
    positions.dedup();
    assert_eq!(
        positions.len(),
        total,
        "two coord events now share a tile -- review this module's \
         single-candidate reachability arguments, which assumed they never would"
    );
}

// Stacked coord-event scan: fabricated stacks driven through `first_battle_trigger_ready`,
// the entry point `OverworldPhase::step` calls.

fn candidate_on_the_trigger_tile(kind: assets::CoordEventKind) -> assets::CoordEvent {
    assets::CoordEvent {
        x: i16::try_from(ROUTE_101_TRIGGER_TILE.0).expect("a map coordinate fits an i16"),
        y: i16::try_from(ROUTE_101_TRIGGER_TILE.1).expect("a map coordinate fits an i16"),
        elevation: ROUTE_101_TRIGGER_ELEVATION,
        kind,
    }
}

/// The real rescue coord event from the generated events table, so map-data changes break the stacks.
fn rescue_trigger_candidate() -> assets::CoordEvent {
    let events = assets::MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .expect("Route 101 resolves in the generated map-events table");
    let x = i16::try_from(ROUTE_101_TRIGGER_TILE.0).expect("a map coordinate fits an i16");
    let y = i16::try_from(ROUTE_101_TRIGGER_TILE.1).expect("a map coordinate fits an i16");
    let rescue = *events
        .coord_events
        .iter()
        .find(|c| c.x == x && c.y == y)
        .expect("Route 101 declares its rescue coord event on the trigger tile");
    assert!(
        matches!(
            rescue.kind,
            assets::CoordEventKind::Trigger {
                var: "VAR_ROUTE101_STATE",
                var_value: 1,
                script: "Route101_EventScript_StartBirchRescue",
            }
        ),
        "fixture precondition: the rescue tile's real coord event is the \
         VAR_ROUTE101_STATE == 1 Birch-rescue trigger"
    );
    assert_eq!(
        rescue.elevation, ROUTE_101_TRIGGER_ELEVATION,
        "fixture precondition: the stacks below share the rescue event's own elevation"
    );
    rescue
}

/// Leaks the event list so the runtime can borrow `'static` events in a test binary.
fn route_101_runtime_with_stack(
    phase: &OverworldPhase,
    stack: Vec<assets::CoordEvent>,
) -> MapRuntime<'_> {
    let coord_events: &'static [assets::CoordEvent] = Box::leak(stack.into_boxed_slice());
    let header = assets::MapHeaderTable::new()
        .header(MapId("MAP_ROUTE101"))
        .expect("Route 101 resolves in the generated map-header table");
    let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
        id: MapId("MAP_ROUTE101"),
        shared_events_map: None,
        object_events: &[],
        warp_events: &[],
        coord_events,
        bg_events: &[],
    }));
    phase.scene.runtime(MapId("MAP_ROUTE101"), header, events)
}

fn phase_on_the_trigger_tile() -> OverworldPhase {
    let phase = route_101_trigger_phase(PlayerState::new(
        ROUTE_101_TRIGGER_TILE,
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::South,
    ));
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "setup: entering Route 101 must leave the live var at the rescue trigger's var_value"
    );
    phase
}

/// Weather and failed-var candidates return `NULL` upstream, so `GetCoordEventScriptAtPosition`
/// keeps scanning and reaches the rescue trigger (`src/field_control_avatar.c:881-884,891,909-911`).
/// The failed-var candidate is `PreventExitSouth` (`== 2`) while the var reads 1.
#[test]
fn a_weather_and_failed_var_candidate_stacked_ahead_do_not_hide_the_rescue_trigger() {
    let phase = phase_on_the_trigger_tile();
    let runtime = route_101_runtime_with_stack(
        &phase,
        vec![
            candidate_on_the_trigger_tile(assets::CoordEventKind::Weather(
                assets::CoordWeather::Rain,
            )),
            candidate_on_the_trigger_tile(assets::CoordEventKind::Trigger {
                var: "VAR_ROUTE101_STATE",
                var_value: 2,
                script: "Route101_EventScript_PreventExitSouth",
            }),
            rescue_trigger_candidate(),
        ],
    );

    assert!(
        phase.first_battle_trigger_ready(&runtime, Some(ROUTE_101_TRIGGER_TILE)),
        "neither a weather candidate nor a failed var check yields a script upstream, so \
         the scan must keep going and reach the rescue trigger stacked behind them"
    );
}

/// A `TRIGGER_RUN_IMMEDIATELY` candidate runs its script and still returns `NULL`, so the scan
/// continues (`src/field_control_avatar.c:886-889`). No bundled Route 101 event uses it.
#[test]
fn a_run_immediately_candidate_stacked_ahead_does_not_hide_the_rescue_trigger() {
    let phase = phase_on_the_trigger_tile();
    let runtime = route_101_runtime_with_stack(
        &phase,
        vec![
            candidate_on_the_trigger_tile(assets::CoordEventKind::Trigger {
                var: "TRIGGER_RUN_IMMEDIATELY",
                var_value: 0,
                script: "LavaridgeTown_EventScript_HotSpringsTrigger",
            }),
            rescue_trigger_candidate(),
        ],
    );

    assert!(
        phase.first_battle_trigger_ready(&runtime, Some(ROUTE_101_TRIGGER_TILE)),
        "TRIGGER_RUN_IMMEDIATELY returns NULL after running its script, so it must not end \
         the scan short of the rescue trigger behind it"
    );
}

/// A foreign trigger whose guard passes (`VAR_ROUTE101_STATE == 1`) ends the scan with its own
/// script, so the rescue behind it must not fire
/// (`src/field_control_avatar.c:891-892,909-911`).
#[test]
fn a_foreign_trigger_whose_guard_passes_wins_the_tile_from_the_rescue_trigger() {
    let phase = phase_on_the_trigger_tile();
    let runtime = route_101_runtime_with_stack(
        &phase,
        vec![
            candidate_on_the_trigger_tile(assets::CoordEventKind::Trigger {
                var: "VAR_ROUTE101_STATE",
                var_value: 1,
                script: "Route101_EventScript_SomeOtherTrigger",
            }),
            rescue_trigger_candidate(),
        ],
    );

    assert!(
        !phase.first_battle_trigger_ready(&runtime, Some(ROUTE_101_TRIGGER_TILE)),
        "a yielding candidate ends upstream's scan and its own script runs -- the rescue \
         trigger stacked behind it must not fire"
    );
}

/// A trigger on a var this port cannot read is treated as yielding, so the scan fails closed
/// instead of guessing past it (see `first_battle_trigger_at`).
#[test]
fn a_trigger_on_an_unevaluable_var_stacked_ahead_fails_closed() {
    let phase = phase_on_the_trigger_tile();
    let runtime = route_101_runtime_with_stack(
        &phase,
        vec![
            candidate_on_the_trigger_tile(assets::CoordEventKind::Trigger {
                var: "VAR_LITTLEROOT_TOWN_STATE",
                var_value: 1,
                script: "LittlerootTown_EventScript_GoSaveBirchTrigger",
            }),
            rescue_trigger_candidate(),
        ],
    );

    assert!(
        !phase.first_battle_trigger_ready(&runtime, Some(ROUTE_101_TRIGGER_TILE)),
        "an un-evaluable guard must end the scan, not be assumed to fail"
    );
}
