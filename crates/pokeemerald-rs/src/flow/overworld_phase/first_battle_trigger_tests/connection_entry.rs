//! Real-pack connection entry into Route 101: the arrival bookkeeping and landing on the rescue trigger.

use super::super::test_support::*;
use super::super::{ActiveBattle, OverworldPhase};
use crate::new_game;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// `cross_connection` runs the entry sync, so crossing from Littleroot sets the var to 1 on arrival,
/// before the trigger is tested. Needs the pack: `cross_connection` bails out without it.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_crossing_into_route_101_primes_the_rescue_var_on_arrival() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let source = crate::pack_source::PackSource::Repo;
    let scene = crate::overworld::load_room_from_source(
        &source,
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    // Two steps from the crossing along the walkable `x = 10` column (see `connections_tests`).
    let player = PlayerState::new((10, 1), 3, Direction::North);
    let mut phase = OverworldPhase::for_test_with_source(scene, littleroot, player, None, source);
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
    let source = crate::pack_source::PackSource::Repo;
    let scene = crate::overworld::load_room_from_source(
        &source,
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let player = PlayerState::new((10, 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test_with_source(scene, littleroot, player, None, source);
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
