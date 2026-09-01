//! Tests for [`super::first_battle_trigger`]: the Route 101 scripted
//! first-battle coord-event trigger (issue #231), its `VAR_ROUTE101_STATE`
//! on-frame bookkeeping, and its interaction with the ordinary wild
//! encounter roll.

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use assets::MapId;
use battle::BattleOutcome;
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_EAST_ARROW_WARP, MB_NORMAL, MB_TALL_GRASS,
};
use engine::overworld::{
    Direction, MapRuntime, PlayerState, WALK_FRAMES_PER_TILE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

// ---------------------------------------------------------------------------
// The Route 101 scripted first-battle trigger (issue #231).
// ---------------------------------------------------------------------------

/// One of the real Route 101 rescue coord-event trigger tiles
/// (`pokeemerald/data/maps/Route101/map.json`'s `coord_events`, elevation 3)
/// -- `first_battle_trigger`'s own module docs have the full citation.
const ROUTE_101_TRIGGER_TILE: (i32, i32) = (10, 19);

/// `ROUTE_101_TRIGGER_TILE`'s own elevation.
const ROUTE_101_TRIGGER_ELEVATION: u8 = 3;

/// A synthetic, fully open (no collision, elevation 3 throughout) room named
/// `MAP_ROUTE101`: the layout grid is fabricated, but `map_id` still resolves
/// to Route 101's *real* generated [`assets::MapEventsTable`] entry -- the
/// same "real events over a synthetic grid" split
/// `crate::flow::wild_encounter::tests::route_101_phase` uses for the wild
/// table, here for the coord-event trigger instead. 25x25 is large enough to
/// place the player next to [`ROUTE_101_TRIGGER_TILE`].
fn route_101_trigger_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(25, 25),
        MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

/// [`route_101_trigger_phase`] with metatile behaviors painted onto named
/// cells -- the precedence tests below need the trigger tile to *also* be
/// something (tall grass, a door, an arrow), which is the only way this
/// port's own precedence between the coord event and the rest of
/// `ProcessPlayerFieldInput` becomes observable at all.
fn route_101_trigger_phase_with_special_tiles(
    player: PlayerState,
    specials: &[((u16, u16), u8)],
) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles(25, 25, specials),
        MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

/// [`ROUTE_101_TRIGGER_TILE`] as the `(u16, u16)` the synthetic-scene
/// fixtures index cells by.
fn trigger_tile_cell() -> (u16, u16) {
    let (x, y) = ROUTE_101_TRIGGER_TILE;
    (
        u16::try_from(x).expect("the trigger tile is inside the fixture"),
        u16::try_from(y).expect("the trigger tile is inside the fixture"),
    )
}

/// Walk `tiles` whole tiles east, each with its own drain frame
/// ([`walk_one_tile_east`]).
fn walk_east(phase: &mut OverworldPhase, tiles: usize) {
    for _ in 0..tiles {
        walk_one_tile_east(phase);
    }
}

/// Step 1 of the upstream chain (`first_battle_trigger`'s module docs):
/// `Route101_OnFrame`'s guard bumps a fresh save's `VAR_ROUTE101_STATE` from
/// `0` to `1` the moment the player is on the map, or this slice's trigger
/// (gated on that same value) could never gate open in real play.
#[test]
fn entering_route_101_bumps_the_fresh_save_rescue_var_to_one() {
    let phase = route_101_trigger_phase(PlayerState::new((0, 0), 3, Direction::East));
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "a fresh save's VAR_ROUTE101_STATE must read 1 on Route 101, not its 0 default"
    );
}

/// Entering any *other* map must never touch `VAR_ROUTE101_STATE` -- the
/// on-frame bump is gated on the map, matching upstream's own map-scoped
/// `MAP_SCRIPT_ON_FRAME_TABLE`.
#[test]
fn entering_a_different_map_leaves_the_rescue_var_at_its_fresh_save_zero() {
    let phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((0, 0), 3, Direction::East),
        None,
    );
    assert_eq!(phase.save1.event_data.var_get(VAR_ROUTE101_STATE), Ok(0));
}

/// The entry bump's fresh-save behavior *and* its documented idempotency,
/// pinned through the real continue constructor: `OverworldPhase::from_saved`
/// must run `sync_route_101_state_on_entry` exactly as `Self::new`,
/// `Self::warp_to`, and `Self::cross_connection` already do, so a fresh save
/// (var `0`) resumed on Route 101 still gets bumped to `1`, and every later
/// story state (`1`, `2`, `3`) resumes unchanged -- without that guard,
/// re-entering the map on a post-rescue save would re-open the trigger and
/// re-fight the scripted Zigzagoon (issue #231 review).
#[test]
fn continuing_on_route_101_only_advances_the_fresh_rescue_state() {
    for (saved_state, expected_state) in [(0, 1), (1, 1), (2, 2), (3, 3)] {
        let (mut block1, block2) = new_game::init_save_blocks_for_new_game();
        block1
            .event_data
            .var_set(VAR_ROUTE101_STATE, saved_state)
            .unwrap();
        let resumed = OverworldPhase::from_saved(
            crate::overworld::tests::synthetic_scene(25, 25),
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

/// The acceptance path this issue exists for: stepping onto the real Route
/// 101 rescue trigger tile, with the var in its pre-rescue state, starts the
/// scripted first battle through the real [`OverworldPhase::step`] path --
/// not [`crate::flow::first_battle::start_first_battle`] called directly.
/// Pins (a) from the issue's test list.
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
        phase.wild_battle.is_none(),
        "this trigger must never route through the ordinary wild-encounter path"
    );
    let battle = phase
        .first_battle
        .as_ref()
        .expect("the rescue trigger must start the scripted first battle");
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

/// Route 101's second rescue coord event must trigger independently of the
/// first one: approach `(11, 19)` from the east and exercise the complete
/// [`OverworldPhase::step`] path through the landing drain frame.
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
        phase.first_battle.is_some(),
        "the second rescue coord event must start the scripted first battle"
    );
}

/// The no-party-lead arm of `super::first_battle_trigger`'s
/// `begin_first_battle`, which its own doc comment claims outright ("The
/// trigger is still consumed in that case, exactly as it is upstream -- the
/// coord event fires, the cutscene it stands in for runs, and the tile is
/// spent whether or not this port could build a battle out of it") and which
/// `connections_tests::walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`'s
/// own doc comment relies on to explain why that pack-only walk stays quiet.
/// Nothing else pins it: every other trigger test assigns a lead, so moving
/// the `var_set` below `begin_first_battle`'s `let Some(lead) = ... else`
/// early return survives the whole suite without this.
#[test]
fn the_trigger_is_consumed_even_when_there_is_no_party_lead_to_fight_with() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    // Deliberately no `party_lead`: `OverworldPhase::for_test` leaves it
    // `None`, which is the bare-test-phase case that arm exists for.
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.first_battle.is_none(),
        "there was no lead mon to build a battle out of"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "the coord event still fired, so the trigger is spent -- upstream's own \
         setvar VAR_ROUTE101_STATE, 2 runs mid-cutscene, long before any battle"
    );
}

/// A step that lands elsewhere on Route 101 -- one tile short of the trigger
/// -- must not start anything: the coord-event lookup is exact-position, not
/// a nearby-tile fuzzy match.
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
    assert!(phase.first_battle.is_none());
    assert!(phase.wild_battle.is_none());
}

/// (b) from the issue's test list: the battle the trigger starts plays to a
/// real terminal outcome through the per-frame driver
/// ([`OverworldPhase::step`] -> [`crate::flow::first_battle::advance_first_battle`]),
/// freezing the overworld exactly as an ordinary wild battle does, and hands
/// the player's mon back on the frame it ends.
///
/// The "frozen" claim is checked only on frames that leave [`OverworldPhase::first_battle`]
/// still `Some` -- the one frame that empties it (issue #251) also runs
/// [`OverworldPhase::conclude_first_battle`], which may move the player via
/// its own warp to Birch's lab (pack-dependent:
/// [`OverworldPhase::warp_to_position`]'s "leaves the player exactly where
/// they stood" fallback when no pack is extracted). That warp is this
/// module's own concern, not this test's -- see
/// `super::first_battle_conclusion_tests` for its pins.
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
    assert!(phase.first_battle.is_some(), "setup: the trigger must fire");

    let frozen_at = phase.player.position();
    let mut frames = 0;
    while phase.first_battle.is_some() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(
            frames < 500,
            "the headless first-battle driver must terminate"
        );
        if phase.first_battle.is_some() {
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
}

/// (c) from the issue's test list: by the time the battle ends,
/// `VAR_ROUTE101_STATE` has advanced past the trigger's own gate (upstream's
/// own `setvar VAR_ROUTE101_STATE, 2`, written at trigger time --
/// `first_battle_trigger`'s "When the var advances" section), so stepping
/// onto the same tile again starts nothing -- no second battle, and the roll
/// doesn't silently fall through to an ordinary wild encounter either.
///
/// Issue #251's own conclusion advances the var a second time, to its
/// terminal `3` (`super::first_battle_conclusion`'s own module docs) --
/// this test's subject is the trigger's refusal, not that further advance,
/// so it drives the battle to conclusion on its own throwaway phase purely
/// to obtain the real post-conclusion var value, then re-approaches the
/// tile on a **fresh** phase with that value pre-set. A single phase
/// carried straight through would conflate two different pack-dependent
/// facts -- whether `conclude_first_battle`'s own warp fires depends on
/// whether a local pack is extracted (`OverworldPhase::warp_to_position`'s
/// "leaves the player exactly where they stood" fallback), and this test
/// must pass identically either way.
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
    while concluded.first_battle.is_some() {
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

    // A fresh phase, carrying only the concluded var value forward -- the
    // same kind of completed step that fired the trigger the first time.
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
        phase.first_battle.is_none(),
        "the trigger must not refire once VAR_ROUTE101_STATE has advanced"
    );
    assert!(
        phase.wild_battle.is_none(),
        "the tile must not fall through to an ordinary wild-encounter roll either"
    );
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "ordinary ground (this synthetic room's own default) draws nothing either way"
    );
}

/// (e) from the issue's test list: the trigger draws off the phase's one
/// shared stream, in [`crate::flow::first_battle::start_first_battle`]'s own
/// documented order -- stepping onto the tile must produce *exactly* the
/// battle a direct call to that function, off an identically-seeded
/// reference generator, would.
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
    let battle = phase.first_battle.as_ref().expect("the trigger must fire");

    // Replay the identical construction off an independent, identically
    // seeded generator: no overworld step precedes it, so if the phase drew
    // anything of its own before handing off, the enemy's rolled identity
    // (and the stream's final position) would disagree.
    let mut reference = Rng::new(SEED);
    let expected = crate::flow::first_battle::start_first_battle(lead, &mut reference)
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

/// The `cross_connection` half of [`OverworldPhase`]'s map-entry bump
/// (`super::first_battle_trigger::sync_route_101_state_on_entry`'s own
/// "called at every one of `OverworldPhase`'s map-entry points" claim):
/// walking into Route 101 over its map-edge connection primes
/// `VAR_ROUTE101_STATE` from a fresh save's `0` to `1` **on arrival**, before
/// the crossing step's own walk animation has drained and therefore before
/// the rescue trigger this slice gates on that value is ever tested.
///
/// Real-pack, and unavoidably so: `OverworldPhase::cross_connection` loads
/// the entered map's room through `crate::overworld::load_room` and bails out
/// before any arrival effect when no pack is extracted, so no synthetic
/// fixture in this file can reach the call site at all.
///
/// Deleting `cross_connection`'s `sync_route_101_state_on_entry` call fails
/// here directly (the var stays `0`), and fails
/// `real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle`
/// below as a consequence -- the trigger's gate never opens, so no battle
/// starts. This test is the direct one: it isolates the arrival effect from
/// the trigger that consumes it.
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

    // One tile south of Littleroot's own last interior row (the walkable
    // `x = 10` column `connections_tests::walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`
    // documents), so exactly two steps reach the crossing.
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

    // The step that walks off the north edge: the crossing itself.
    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, route101, "the crossing must have landed");
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "cross_connection runs Route101_OnFrame's own guard on arrival, so the rescue \
         trigger's gate is already open when this step's landing drains"
    );
    assert!(
        phase.first_battle.is_none(),
        "the crossing step is still mid-animation -- the coord event is only tested once \
         its landing drains"
    );
}

/// The end-to-end acceptance path against the real extracted pack: walking
/// off Littleroot Town's north edge crosses into Route 101 and lands *exactly*
/// on the real rescue coord-event trigger tile
/// (`connections_tests::walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`
/// pins the landing itself at `(10, 19)`), which must start the
/// scripted first battle and play it to a real outcome through
/// [`OverworldPhase::step`] alone -- no direct call into
/// [`crate::flow::first_battle`] anywhere in this test.
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
    // The production stand-in for the un-ported Birch-bag handout
    // (`crate::new_game::provisional_starter`'s own docs) -- without it
    // there would be nothing to fight with, exactly as `OverworldPhase::load_default`
    // assigns it before the player can ever reach the overworld for real.
    phase.party_lead = Some(new_game::provisional_starter());

    // Two ordinary steps north to the edge, then the crossing step itself,
    // then its own walk animation drains -- the same setup
    // `connections_tests::walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`
    // uses, continued one step further into the rescue trigger.
    for _ in 0..3 {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
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
    assert!(phase.wild_battle.is_none());
    let battle = phase
        .first_battle
        .as_ref()
        .expect("landing on the real trigger tile must start the scripted first battle");
    assert_eq!(battle.enemy().species(), assets::SpeciesId(288));
    assert_eq!(battle.enemy().level(), 2);

    // Play it out through the real per-frame driver.
    let mut frames = 0;
    while phase.first_battle.is_some() {
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

/// Review regression (#231, finding 1): an **aborted** first battle must
/// consume the trigger just the same.
///
/// `crate::flow::first_battle::advance_first_battle`'s own doc comment spells
/// the abort contract out — a turn the engine cannot play (here: a lead whose
/// slot 0 has no PP left, `battle::BattleError::NoPpRemaining(0)` out of
/// `Battle::take_turn`'s pre-draw validation) empties the slot, writes the
/// lead back, and returns **`None`**, never an outcome. An earlier revision
/// of this slice advanced `VAR_ROUTE101_STATE` only on `Some(outcome)`, which
/// left the var at `1` on exactly this path and the coord-event tile live, so
/// the next step onto it started the whole thing over. The var now moves at
/// trigger time, upstream's own ordering (`scripts.inc:40`, mid-cutscene) —
/// see `super::first_battle_trigger`'s "When the var advances" section.
#[test]
fn an_aborted_first_battle_still_consumes_the_route_101_trigger() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    // Prove beginning this attempt clears stale terminal state rather than
    // letting its later abort masquerade as a completed battle.
    phase.first_battle_outcome = Some(BattleOutcome::PlayerWon);
    // Drain slot 0 through the same accessor the turn engine spends PP with,
    // rather than reaching into the struct -- `crate::flow::first_battle`'s
    // own abort test does it this way too.
    let mut lead = new_game::provisional_starter();
    let starting_pp = lead.moves()[0].pp;
    assert!(starting_pp > 0, "a freshly built starter starts with PP");
    for _ in 0..starting_pp {
        lead.deduct_pp(0)
            .expect("draining a slot that still has PP");
    }
    phase.party_lead = Some(lead);

    walk_one_tile_east(&mut phase);
    assert!(
        phase.first_battle.is_some(),
        "setup: the trigger must fire and build a battle -- the abort happens a frame later"
    );
    assert_eq!(
        phase.first_battle_outcome, None,
        "starting a new fight clears the previous terminal outcome"
    );

    // One driver frame: the turn fails pre-draw, so the battle ends with no
    // outcome at all.
    phase.step(held(Buttons::RIGHT));
    assert!(
        phase.first_battle.is_none(),
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

    // Walk off the tile and back onto it -- the same fresh completed step
    // that fired the trigger the first time.
    phase.player = PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East);
    phase.pending_landing = None;
    let rng_before = phase.rng.state();
    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.first_battle.is_none(),
        "an aborted battle must not leave the rescue trigger live"
    );
    assert!(phase.wild_battle.is_none());
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "a spent trigger draws nothing -- not even start_first_battle's construction draws"
    );
}

/// Review regression (#231, finding 2a): the trigger **suppresses the
/// wild-encounter roll** on the frame it fires.
///
/// Upstream reaches `CheckStandardWildEncounter` (`field_control_avatar.c:162`)
/// only by falling through `TryStartStepBasedScript` (`:155-161`), which
/// returns `TRUE` the moment `TryStartCoordEventScript` (`:485-486`) fires —
/// so a coord-event frame never rolls. This is the one arm of that precedence
/// bundled Route 101 data can actually express: the rescue tile is paintable
/// as tall grass, and Route 101's real land table is fightable, so absent the
/// suppression the very same step would draw.
///
/// Pinned two ways, because a suppressed roll is the absence of an effect:
/// the stream must sit exactly where `start_first_battle` alone left it, and
/// `CheckStandardWildEncounter`'s `sPrevMetatileBehavior` side effect
/// (`:668-686`) must be untouched. The immunity counter cannot serve as a
/// third pin -- a fired roll zeroes it (saturation) and `begin_first_battle`'s
/// `battle_setup.c:941` restart zeroes it too, so its `0` here pins the
/// *restart*, not the suppression. The control run at the end proves the
/// fixture really would have rolled.
#[test]
fn the_route_101_trigger_suppresses_the_wild_encounter_roll_on_its_own_tile() {
    // `crate::flow::wild_encounter::tests`' own `ENCOUNTER_SEED`: its first
    // draws make the first *rolled* grass step produce a real encounter, so
    // the control below is unambiguous.
    const SEED: u32 = 17;
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let grass = [(trigger_tile_cell(), MB_TALL_GRASS)];

    let mut phase = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 5, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &grass,
    );
    phase.rng = Rng::new(SEED);
    phase.party_lead = Some(new_game::provisional_starter());
    // Four ordinary steps burn the whole post-transition immunity window
    // (`WILD_ENCOUNTER_IMMUNITY_STEPS`) without drawing, so the fifth -- the
    // one onto the trigger tile -- is a step the roll would really see.
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
        phase.first_battle.is_some(),
        "the coord event still fires on a tile that is also grass"
    );
    assert!(
        phase.wild_battle.is_none(),
        "and the grass roll it outranks must never have happened"
    );
    let mut reference = Rng::new(SEED);
    crate::flow::first_battle::start_first_battle(new_game::provisional_starter(), &mut reference)
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

    // Control: the identical walk with the trigger already spent
    // (`VAR_ROUTE101_STATE == 2`) really does roll on that same tile, so the
    // assertions above are about a suppressed roll rather than a fixture that
    // could never have rolled.
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
        control.first_battle.is_none(),
        "control: a spent trigger must not fire"
    );
    assert!(
        control.wild_battle.is_some(),
        "control: the same grass tile really does roll an encounter when nothing outranks it"
    );
    assert_eq!(
        control.wild.prev_metatile_behavior(),
        MB_TALL_GRASS,
        "control: and CheckStandardWildEncounter really does record the tile it saw"
    );
}

/// Review regression (#231, finding 2b): the trigger also outranks the
/// door-warp check (`field_control_avatar.c:155-161`, after
/// `TryStartCoordEventScript` inside the same `TryStartStepBasedScript`) and
/// the arrow poll (`:164-168`) — and *why that half cannot be driven to a
/// real warp on this map*.
///
/// Both warp paths need a `warp_events` entry at the tile
/// (`engine::overworld::trigger_door_warp`/`trigger_arrow_warp` each look one
/// up after the behavior check), and **Route 101 declares none at all** — a
/// route is entered by walking, not through a door. So no test over bundled
/// data can make a warp and this coord event contend for the same frame; the
/// precedence is encoded and unit-pinned rather than driven end to end,
/// exactly as
/// `crate::flow::wild_encounter::tests::a_fired_encounter_closes_the_arrow_warp_poll`
/// pins `arrow_poll_open`'s own equally-unreachable encounter arm.
///
/// What this test *does* pin is that reachability argument itself — the
/// emptiness of `warp_events` is an assertion, so a future Route 101 warp
/// fails here and forces the real precedence test — plus the behavior half
/// the fixture can express: a door-shaped and an arrow-shaped metatile
/// behavior painted onto the trigger tile change nothing about the frame.
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
            phase.first_battle.is_some(),
            "behavior {behavior:#04x}: the coord event fires regardless of the tile's behavior"
        );
    }
}

/// Review regression (#231, finding 3): `Route101_EventScript_PreventExitSouth`'s
/// own coord event at `(10, 18)` must never start a battle — and the only
/// thing standing between it and one is the **script-name** check in
/// `super::first_battle_trigger::OverworldPhase::first_battle_trigger_at`.
///
/// Everything else about that event matches: it is a `Trigger` on
/// `"VAR_ROUTE101_STATE"` at elevation 3, its `var_value` is `2` — the exact
/// value this cut leaves behind once the rescue trigger is consumed — and it
/// sits one tile north of the rescue tile, so the player really does stand on
/// it, with the var really reading `2`, the moment they walk back south.
/// Dropping the `script != TRIGGER_SCRIPT` half of that check restarts the
/// Zigzagoon fight there.
#[test]
fn the_prevent_exit_coord_events_never_start_a_battle() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    // Starts directly on the rescue tile itself, not one step west of it:
    // with the var already past `PRE_RESCUE_STATE` (below), the rescue
    // trigger's own gate no longer matches there, so no walk onto it is
    // needed to reach the ordinary-ground state this test wants before
    // stepping north.
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::North,
    ));
    // The value `Route101_EventScript_StartBirchRescue`'s own trigger-time
    // write leaves behind (`super::first_battle_trigger::TRIGGER_CONSUMED_STATE`)
    // -- set directly rather than reached by driving a battle to conclusion,
    // since issue #251's own conclusion now advances the var a second time,
    // to `3` (`super::first_battle_conclusion`'s module docs), and this
    // test's subject is the script-name discriminator `PreventExitSouth`'s
    // *own* `var_value == 2` happens to share, not that later advance.
    phase
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    phase.party_lead = Some(new_game::provisional_starter());

    // North onto `(10, 18)` -- `Route101_EventScript_PreventExitSouth`'s own
    // coord-event tile.
    let rng_before = phase.rng.state();
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.player.position(),
        (tx, ty - 1),
        "setup: the step must land on the PreventExitSouth coord event"
    );
    assert!(
        phase.first_battle.is_none(),
        "a PreventExit* coord event is not the rescue trigger, whatever its var says"
    );
    assert!(phase.wild_battle.is_none());
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "and nothing was constructed off the shared stream either -- start_first_battle \
         draws before it can fail"
    );
}

/// The other half of the reachability argument
/// `route_101_has_no_warp_events_so_the_trigger_can_never_race_one` starts:
/// the trigger's "discards a same-frame interaction" arm
/// (`crate::flow::wild_encounter::field_input_consumed`, upstream `:172`)
/// cannot be driven either, because no Route 101 object event stands next to
/// a rescue tile for an A press to find.
///
/// Route 101's six object events sit at `(16, 8)`, `(9, 13)`, `(7, 14)`,
/// `(10, 13)`, `(5, 11)` and `(2, 13)` -- the nearest is five rows north of
/// the trigger row. Asserted rather than asserted-in-a-comment so a future
/// object-event addition next to the rescue tiles fails here and forces the
/// real precedence test to be written.
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

/// Route 101's own coord events never stack: all nine sit at distinct
/// `(x, y, elevation)` positions, so no *real* Route 101 tile ever hands
/// `first_battle_trigger_at`'s scan (issue #343) more than one candidate.
/// That is the whole claim here -- the scan's own multi-candidate branches
/// are pinned separately, over fabricated stacks, by the four tests below.
///
/// Asserted like the slice's other data premises (no warp events, no
/// adjacent NPCs), so a map change that stacks two coord events on one
/// Route 101 tile fails this test and forces a look at what upstream's scan
/// would do with the pair -- in particular whether the newcomer's guard is
/// one this port can evaluate at all, since a trigger on any var other than
/// `VAR_ROUTE101_STATE` makes the scan fail closed
/// (`super::first_battle_trigger::OverworldPhase::first_battle_trigger_at`'s
/// "A guard this port cannot evaluate" section).
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

// ---------------------------------------------------------------------------
// The stacked coord-event scan (issue #343).
//
// Route 101's real coord events never share a tile (the test above), so the
// only way to drive `first_battle_trigger_at`'s scan past a first candidate
// is to fabricate the stack: real header, real player, real save state,
// synthetic `coord_events`. These go through `first_battle_trigger_ready`
// -- the one entry point `OverworldPhase::step` calls -- rather than the
// private `first_battle_trigger_at`, so the production caller's own skip
// behaviour is what is pinned.
// ---------------------------------------------------------------------------

/// One fabricated coord event sitting on [`ROUTE_101_TRIGGER_TILE`], at that
/// tile's own elevation.
fn candidate_on_the_trigger_tile(kind: assets::CoordEventKind) -> assets::CoordEvent {
    assets::CoordEvent {
        x: i16::try_from(ROUTE_101_TRIGGER_TILE.0).expect("a map coordinate fits an i16"),
        y: i16::try_from(ROUTE_101_TRIGGER_TILE.1).expect("a map coordinate fits an i16"),
        elevation: ROUTE_101_TRIGGER_ELEVATION,
        kind,
    }
}

/// The **real** rescue coord event, read straight out of the generated
/// map-events table rather than restated -- so every stack below ends in
/// exactly the candidate production data puts on that tile, and a map-data
/// change to it breaks these tests rather than sliding past them.
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

/// A [`MapRuntime`] over `phase`'s own synthetic Route 101 grid and real
/// header, but a fabricated `coord_events` list. Leaks its event list
/// `'static` the same way `engine`'s own `map_runtime` fixtures leak theirs
/// -- a handful of small allocations for the process's lifetime, in a test
/// binary.
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

/// A phase standing on [`ROUTE_101_TRIGGER_TILE`] with `VAR_ROUTE101_STATE`
/// at the rescue trigger's own `1`, asserted rather than assumed.
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

/// Two candidates that upstream's `TryRunCoordEventScript` answers with
/// `NULL` -- a weather event (`field_control_avatar.c:881-884`) and a
/// trigger whose var check fails (`:891`) -- stacked *ahead* of the rescue
/// trigger must not hide it: `GetCoordEventScriptAtPosition` only stops on a
/// candidate that yields a script (`:909-911`), so the scan reaches the
/// third entry and fires.
///
/// The failed-var candidate is Route 101's own `PreventExitSouth` script on
/// `VAR_ROUTE101_STATE == 2` while the live var reads `1` -- the exact
/// mismatch the map's real post-rescue triggers produce, moved onto the
/// rescue tile.
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

/// A `TRIGGER_RUN_IMMEDIATELY` candidate (`include/constants/vars.h:312`)
/// stacked ahead must not hide the rescue trigger either: upstream runs its
/// script on the spot and *still* returns `NULL`
/// (`field_control_avatar.c:886-889`), so the scan continues. Thirty-six
/// bundled coord events across five maps carry that `var` -- none on Route
/// 101, which is why this stack has to be fabricated.
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

/// The other side of the same rule, and the one the first cut of this scan
/// got wrong: a **foreign trigger whose var check passes** stacked ahead of
/// the rescue trigger ends the scan. Upstream's `TryRunCoordEventScript`
/// returns that candidate's script (`field_control_avatar.c:891-892`) and
/// `GetCoordEventScriptAtPosition` returns it straight out of the loop
/// (`:909-911`), so the foreign script -- not the rescue -- is what runs on
/// this tile, and the rescue trigger behind it is never reached.
///
/// The candidate here gates on `VAR_ROUTE101_STATE == 1`, the value the live
/// var actually holds, so its guard genuinely passes; only its script name
/// differs. A scan that skipped it for that reason alone would fire a battle
/// upstream never starts.
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

/// A trigger gated on a variable this port cannot read at all (`var` is an
/// open reference -- `assets::CoordEventKind::Trigger`) stacked ahead of the
/// rescue trigger **fails closed**: with no way to know whether upstream's
/// `VarGet` would pass, the scan treats it as yielding and stops, rather
/// than optimistically stepping over it into a battle upstream might never
/// have started (`first_battle_trigger_at`'s "A guard this port cannot
/// evaluate" section, which records the divergence).
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
