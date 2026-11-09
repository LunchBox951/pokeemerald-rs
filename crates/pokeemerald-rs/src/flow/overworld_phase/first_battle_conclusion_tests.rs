//! Tests for [`super::first_battle_conclusion`]: heal, flag and var writes,
//! and the warp to Birch's lab, on both terminal outcomes and on an abort.

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use assets::{MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs, MAX_IV};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::Buttons;

/// A Route 101 rescue coord-event trigger tile `(x, y)`.
const ROUTE_101_TRIGGER_TILE: (i32, i32) = (10, 19);

const ROUTE_101_TRIGGER_ELEVATION: u8 = 3;

/// Seed under which the provisional starter beats the Zigzagoon.
const WINNING_SEED: u32 = 4242;

/// Seed under which [`fragile_treecko_lead`] loses to the Zigzagoon.
const LOSING_SEED: u32 = 1;

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`). The constants in
/// this file are independent transcriptions of upstream, not production
/// imports.
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// `VAR_ROUTE101_STATE` after the trigger consumes the scene, before the
/// conclusion.
const ROUTE101_STATE_TRIGGER_CONSUMED: u16 = 2;

/// `setvar VAR_ROUTE101_STATE, 3` in `Route101_EventScript_BirchsBag`.
const ROUTE101_STATE_CONCLUDED: u16 = 3;

/// `VAR_BIRCH_LAB_STATE` (`include/constants/vars.h:152`).
const VAR_BIRCH_LAB_STATE: u16 = 0x4084;

/// `setvar VAR_BIRCH_LAB_STATE, 2` in `Route101_EventScript_BirchsBag`.
const BIRCH_LAB_STATE_STARTER_GIVEN: u16 = 2;

/// `VAR_STARTER_MON` (`include/constants/vars.h:53`).
const VAR_STARTER_MON: u16 = 0x4023;

/// `VAR_STARTER_MON` encoding of Treecko, the provisional starter.
const STARTER_MON_TREECKO: u16 = 0;

/// `VAR_STARTER_MON` encoding of Mudkip.
const STARTER_MON_MUDKIP: u16 = 2;

/// Every var starts at 0 in a fresh `EventData`.
const VAR_DEFAULT: u16 = 0;

const TREECKO_SPECIES: SpeciesId = SpeciesId(277);

/// Treecko's level-16 learnset move. `EFFECT_PURSUIT` has no resolver, so
/// `validate_player_move` refuses every slot holding it before any draw
/// (`crates/battle/src/battle.rs:415`), and its full PP keeps the all-spent
/// Struggle diversion (`crates/battle/src/battle.rs:491`) from turning the
/// abort into a played turn.
const UNEXECUTABLE_MOVE: MoveId = MoveId::PURSUIT;

/// `FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE`
/// (`include/constants/flags.h:769`), the hide flag of the rescue-battle
/// Birch template.
const FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE: u16 = 0x2D0;

/// `FLAG_HIDE_ROUTE_101_ZIGZAGOON` (`include/constants/flags.h:801`), the
/// hide flag of the rescue Zigzagoon template.
const FLAG_HIDE_ROUTE_101_ZIGZAGOON: u16 = 0x2EE;

/// `FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH`
/// (`include/constants/flags.h:770`).
const FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH: u16 = 0x2D1;

/// `FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG`
/// (`include/constants/flags.h:749`).
const FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG: u16 = 0x2BC;

const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");

const BIRCHS_LAB: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB");

/// The `warp` coordinates in `Route101_EventScript_BirchsBag`
/// (`scripts.inc:242`).
const BIRCHS_LAB_ARRIVAL: (i32, i32) = (6, 5);

/// Elevation of [`BIRCHS_LAB_ARRIVAL`] in the real layout.
const BIRCHS_LAB_ARRIVAL_ELEVATION: u8 = 3;

/// The player's money in the loss tests; odd so that halving would change it.
const STARTING_MONEY: u32 = 2001;

/// A synthetic, fully open `MAP_ROUTE101` phase with the given player.
fn route_101_trigger_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(25, 25),
        ROUTE_101,
        player,
        None,
    )
    .with_private_object_event_rng()
}

/// [`route_101_trigger_phase`] that retains `source`, so the conclusion's
/// deferred Birch's Lab warp reads the pack that source names.
fn route_101_trigger_phase_from_source(
    player: PlayerState,
    source: crate::pack_source::PackSource,
) -> OverworldPhase {
    OverworldPhase::for_test_with_source(
        crate::overworld::test_support::synthetic_scene(25, 25),
        ROUTE_101,
        player,
        None,
        source,
    )
    .with_private_object_event_rng()
}

/// [`route_101_trigger_phase`] that retains the checkout pack, so the
/// conclusion's deferred Birch's Lab warp reads the repository's own extract.
fn repo_route_101_trigger_phase(player: PlayerState) -> OverworldPhase {
    route_101_trigger_phase_from_source(player, crate::pack_source::PackSource::Repo)
}

/// [`route_101_trigger_phase`] whose pack source carries a synthetic Birch's
/// Lab, so the conclusion's warp lands without an extracted pack. The
/// returned guard owns the scratch pack file.
fn lab_route_101_trigger_phase(
    player: PlayerState,
) -> (
    OverworldPhase,
    crate::overworld::test_support::BirchsLabPack,
) {
    let lab = crate::overworld::test_support::BirchsLabPack::new();
    let phase = route_101_trigger_phase_from_source(player, lab.source());
    (phase, lab)
}

/// A pack source naming a file that does not exist, so no room can load.
fn unloadable_pack_source() -> crate::pack_source::PackSource {
    crate::pack_source::PackSource::Explicit(
        std::path::Path::new("/nonexistent/pokeemerald-rs-3240/missing.pack").into(),
    )
}

fn play_first_battle_to_conclusion(phase: &mut OverworldPhase) {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    assert_eq!(
        phase.player.position(),
        (tx - 1, ty),
        "setup: the phase must start one tile west of the trigger"
    );
    walk_one_tile_east(phase);
    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.is_first_battle_active(),
        "setup: the trigger must fire"
    );

    let mut frames = 0;
    while phase.is_first_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 20, "the fixture battles must resolve quickly");
    }
}

/// A level-1 Treecko with zero Defense IV and only Pound. A level-2
/// Zigzagoon's Tackle can overkill it in one hit from above the
/// `AI_FirstBattle` flee threshold, so [`LOSING_SEED`] yields
/// [`BattleOutcome::PlayerLost`]. The derivation is in
/// `crate::flow::wild_encounter::first_battle_recovery_tests::a_lost_route_101_first_battle_heals_the_lead_instead_of_leaving_it_fainted`.
fn fragile_treecko_lead() -> BattlePokemon {
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: 0,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), TREECKO_SPECIES, 1, ivs, 0, vec![MoveId::POUND])
        .expect("Treecko/Pound must be in the dex")
}

/// A win heals the lead. [`WINNING_SEED`] has the provisional starter take
/// damage and spend PP on the way to victory, so a missing heal is visible.
#[test]
fn conclude_first_battle_heals_the_lead_on_a_win() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(WINNING_SEED);
    phase.set_fresh_lead_for_test(new_game::provisional_starter());
    let max_hp = phase.lead_for_test().unwrap().stats().max_hp;
    let base_pp = phase.lead_for_test().unwrap().moves()[0].pp;

    play_first_battle_to_conclusion(&mut phase);

    assert_eq!(
        phase.first_battle_outcome(),
        Some(BattleOutcome::PlayerWon),
        "setup: this seed/lead combination must really win"
    );
    let lead = phase.lead_for_test().expect("the lead is still present");
    assert!(!lead.is_fainted());
    assert_eq!(
        lead.current_hp(),
        max_hp,
        "HealPlayerParty restores full HP even on a win"
    );
    assert_eq!(
        lead.moves()[0].pp,
        base_pp,
        "HealPlayerParty restores PP to its base value even on a win"
    );
}

/// The three var writes happen on both a win and a loss.
#[test]
fn conclude_first_battle_writes_all_three_vars_on_both_outcomes() {
    for (seed, lead, expected_outcome) in [
        (
            WINNING_SEED,
            new_game::provisional_starter(),
            BattleOutcome::PlayerWon,
        ),
        (
            LOSING_SEED,
            fragile_treecko_lead(),
            BattleOutcome::PlayerLost,
        ),
    ] {
        let (tx, ty) = ROUTE_101_TRIGGER_TILE;
        let (mut phase, _lab) = lab_route_101_trigger_phase(PlayerState::new(
            (tx - 1, ty),
            ROUTE_101_TRIGGER_ELEVATION,
            Direction::East,
        ));
        phase.rng = Rng::new(seed);
        phase.set_fresh_lead_for_test(lead);
        // Treecko's encoding equals the fresh default, so a non-default start
        // is needed to show the write ran.
        phase
            .save1
            .event_data
            .var_set(VAR_STARTER_MON, STARTER_MON_MUDKIP)
            .expect("VAR_STARTER_MON is an ordinary var");

        play_first_battle_to_conclusion(&mut phase);

        assert_eq!(
            phase.first_battle_outcome(),
            Some(expected_outcome),
            "setup: seed {seed} must produce {expected_outcome:?}"
        );
        assert_eq!(
            phase.map_id, BIRCHS_LAB,
            "setup: the warp to the lab must land, or the writes are rolled back"
        );
        assert_eq!(
            phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
            Ok(ROUTE101_STATE_CONCLUDED),
            "outcome {expected_outcome:?}: setvar VAR_ROUTE101_STATE, 3"
        );
        assert_eq!(
            phase.save1.event_data.var_get(VAR_BIRCH_LAB_STATE),
            Ok(BIRCH_LAB_STATE_STARTER_GIVEN),
            "outcome {expected_outcome:?}: setvar VAR_BIRCH_LAB_STATE, 2"
        );
        assert_eq!(
            phase.save1.event_data.var_get(VAR_STARTER_MON),
            Ok(STARTER_MON_TREECKO),
            "outcome {expected_outcome:?}: the conclusion overwrites the preset Mudkip encoding with Treecko's"
        );
    }
}

/// The four hide-flag writes happen on both a win and a loss. The lab-Birch
/// flag starts set, as in a new save, and ends cleared. The three Route 101
/// flags start clear and end set.
#[test]
fn conclude_first_battle_writes_the_object_event_flags_on_both_outcomes() {
    for (seed, lead, expected_outcome) in [
        (
            WINNING_SEED,
            new_game::provisional_starter(),
            BattleOutcome::PlayerWon,
        ),
        (
            LOSING_SEED,
            fragile_treecko_lead(),
            BattleOutcome::PlayerLost,
        ),
    ] {
        let (tx, ty) = ROUTE_101_TRIGGER_TILE;
        let (mut phase, _lab) = lab_route_101_trigger_phase(PlayerState::new(
            (tx - 1, ty),
            ROUTE_101_TRIGGER_ELEVATION,
            Direction::East,
        ));
        phase.rng = Rng::new(seed);
        phase.set_fresh_lead_for_test(lead);
        // This fixture bypasses `new_game::init_save_blocks`, so seed the
        // flag that a new save starts with.
        phase
            .save1
            .event_data
            .flag_set(FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH)
            .expect("the lab-Birch hide flag is an ordinary flag");

        play_first_battle_to_conclusion(&mut phase);

        assert_eq!(
            phase.first_battle_outcome(),
            Some(expected_outcome),
            "setup: seed {seed} must produce {expected_outcome:?}"
        );
        assert_eq!(
            phase.map_id, BIRCHS_LAB,
            "setup: the warp to the lab must land, or the writes are rolled back"
        );
        assert_eq!(
            phase
                .save1
                .event_data
                .flag_get(FLAG_HIDE_ROUTE_101_ZIGZAGOON),
            Ok(true),
            "outcome {expected_outcome:?}: removeobject LOCALID_ROUTE101_ZIGZAGOON persists as \
             setting the Zigzagoon's own hide flag"
        );
        assert_eq!(
            phase
                .save1
                .event_data
                .flag_get(FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE),
            Ok(true),
            "outcome {expected_outcome:?}: setflag FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE"
        );
        assert_eq!(
            phase
                .save1
                .event_data
                .flag_get(FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH),
            Ok(false),
            "outcome {expected_outcome:?}: clearflag FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH"
        );
        assert_eq!(
            phase
                .save1
                .event_data
                .flag_get(FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG),
            Ok(true),
            "outcome {expected_outcome:?}: setflag FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG"
        );
    }
}

/// The conclusion leaves money unchanged on a loss, unlike a white-out,
/// which halves it.
#[test]
fn conclude_first_battle_never_halves_money_even_on_a_loss() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(LOSING_SEED);
    phase.set_fresh_lead_for_test(fragile_treecko_lead());
    phase.save1.money = STARTING_MONEY;

    play_first_battle_to_conclusion(&mut phase);

    assert_eq!(
        phase.first_battle_outcome(),
        Some(BattleOutcome::PlayerLost),
        "setup: this seed/lead combination must really lose"
    );
    assert_eq!(
        phase.save1.money, STARTING_MONEY,
        "the first-battle heal must not halve money"
    );
}

/// A Treecko whose only move is [`UNEXECUTABLE_MOVE`], so
/// `crate::flow::first_battle::advance_first_battle` aborts: its fallback
/// scan finds no slot `validate_player_move` accepts.
fn unplayable_treecko_lead() -> BattlePokemon {
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: MAX_IV,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    BattlePokemon::new(
        &Dex::new(),
        TREECKO_SPECIES,
        5,
        ivs,
        0,
        vec![UNEXECUTABLE_MOVE],
    )
    .expect("Treecko/Pursuit must be in the dex")
}

/// An aborted battle (no reported outcome) does not conclude: no heal, no
/// var writes, no warp. The fixture matches
/// `first_battle_trigger_tests::trigger::an_aborted_first_battle_still_consumes_the_route_101_trigger`.
#[test]
fn an_aborted_first_battle_does_not_run_the_conclusion() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(WINNING_SEED);
    phase.set_fresh_lead_for_test(unplayable_treecko_lead());

    walk_one_tile_east(&mut phase);
    assert!(
        phase.is_first_battle_active(),
        "setup: the trigger must fire"
    );

    // The turn fails before any draw, so one frame ends the battle with no
    // outcome.
    phase.step(held(Buttons::RIGHT));
    assert!(
        !phase.is_first_battle_active(),
        "setup: the aborted battle emptied the slot"
    );
    assert_eq!(
        phase.first_battle_outcome(),
        None,
        "setup: an abort reports no outcome"
    );

    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(ROUTE101_STATE_TRIGGER_CONSUMED),
        "the trigger's consumed state, not the conclusion's terminal value"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_BIRCH_LAB_STATE),
        Ok(VAR_DEFAULT),
        "no conclusion ran, so this var must still read its default"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_STARTER_MON),
        Ok(STARTER_MON_TREECKO),
        "unwritten; Treecko's encoding equals the default, so this is consistent with no write \
         but does not prove it"
    );
    assert_eq!(
        phase.map_id, ROUTE_101,
        "no conclusion ran, so no warp fired"
    );
}

/// With the real pack, a lost first battle heals the lead and lands the
/// player in Birch's lab on the `warp` tile, so the warp resolves a real room.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_a_lost_first_battle_still_heals_and_reaches_the_lab() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = repo_route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(LOSING_SEED);
    phase.set_fresh_lead_for_test(fragile_treecko_lead());

    play_first_battle_to_conclusion(&mut phase);

    assert_eq!(
        phase.first_battle_outcome(),
        Some(BattleOutcome::PlayerLost),
        "setup: this seed/lead combination must really lose"
    );
    let lead = phase.lead_for_test().expect("healed lead still present");
    assert!(
        !lead.is_fainted(),
        "healed even against the real pack's own dex data"
    );
    assert_eq!(
        phase.map_id, BIRCHS_LAB,
        "warp MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5 must actually resolve the lab"
    );
    assert_eq!(
        phase.player.position(),
        BIRCHS_LAB_ARRIVAL,
        "the warp's own explicit coordinates"
    );
}

/// With the real pack, the lab arrival adopts the destination tile's real
/// elevation as soon as the conclusion returns, rather than the
/// `ELEVATION_TRANSITION` wildcard. Both current and previous elevation
/// match, as for any freshly placed [`engine::overworld::PlayerState`].
/// The same relocation is checked for white-out in
/// `white_out::tests::real_pack_white_out_relocation_adopts_the_heal_locations_elevation_before_any_movement`.
///
/// The map and position assertions come first because the Route 101 trigger
/// tile has the same elevation, so the elevation assertions alone would pass
/// if the warp never fired.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_first_battle_conclusion_relocation_adopts_the_labs_elevation_before_any_movement() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = repo_route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(WINNING_SEED);
    phase.set_fresh_lead_for_test(new_game::provisional_starter());

    play_first_battle_to_conclusion(&mut phase);

    assert_eq!(phase.map_id, BIRCHS_LAB, "setup: the warp must have fired");
    assert_eq!(
        phase.player.position(),
        BIRCHS_LAB_ARRIVAL,
        "setup: the lab arrival tile whose elevation is under test"
    );
    assert_eq!(
        phase.player.elevation(),
        BIRCHS_LAB_ARRIVAL_ELEVATION,
        "the lab arrival tile's own real elevation, not the transition wildcard"
    );
    assert_eq!(
        phase.player.previous_elevation(),
        BIRCHS_LAB_ARRIVAL_ELEVATION,
        "previous elevation equals the current one for a freshly placed player"
    );
}

/// An unrelated var the conclusion must never disturb, rolled back or not.
const VAR_UNRELATED_SENTINEL: u16 = 0x4010;

const UNRELATED_SENTINEL_VALUE: u16 = 9;

/// A `VAR_BIRCH_LAB_STATE` the rollback must restore, distinct from both the
/// default and the concluded value.
const BIRCH_LAB_STATE_PRESET: u16 = 7;

/// `VAR_ROUTE101_STATE` while the rescue coord event is armed
/// (`Route101/scripts.inc:14-17`): the value the conclusion's rollback
/// leaves so the trigger fires again.
const ROUTE101_STATE_TRIGGER_READY: u16 = 1;

/// The state a conclusion whose lab load failed must leave behind.
fn assert_conclusion_rolled_back(phase: &OverworldPhase, outcome: BattleOutcome, max_hp: u32) {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    assert_eq!(
        phase.map_id, ROUTE_101,
        "setup: the lab warp must have failed"
    );
    assert_eq!(phase.player.position(), (tx, ty), "the player stays put");
    let data = &phase.save1.event_data;
    assert_eq!(
        data.var_get(VAR_ROUTE101_STATE),
        Ok(ROUTE101_STATE_TRIGGER_READY),
        "{outcome:?}: neither concluded (3) nor consumed (2): the rescue is re-armed"
    );
    assert_eq!(
        data.var_get(VAR_BIRCH_LAB_STATE),
        Ok(BIRCH_LAB_STATE_PRESET)
    );
    assert_eq!(data.var_get(VAR_STARTER_MON), Ok(STARTER_MON_MUDKIP));
    assert_eq!(
        data.var_get(VAR_UNRELATED_SENTINEL),
        Ok(UNRELATED_SENTINEL_VALUE)
    );
    for (flag, expected, name) in [
        (FLAG_HIDE_ROUTE_101_ZIGZAGOON, false, "Zigzagoon"),
        (
            FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE,
            false,
            "Route 101 Birch",
        ),
        (
            FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH,
            true,
            "lab Birch",
        ),
        (
            FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG,
            false,
            "starters bag",
        ),
    ] {
        assert_eq!(
            data.flag_get(flag),
            Ok(expected),
            "{outcome:?}: the {name} hide flag is back where it was"
        );
    }
    assert_eq!(phase.save1.money, STARTING_MONEY, "no money change");
    let lead = phase.lead_for_test().expect("the lead is still present");
    assert!(!lead.is_fainted());
    assert_eq!(lead.current_hp(), max_hp, "the heal stays committed");
}

/// The state the retried rescue's conclusion leaves once the lab loads.
fn assert_retried_handoff_landed(phase: &OverworldPhase) {
    assert_eq!(
        phase.map_id, BIRCHS_LAB,
        "the retried handoff lands in the lab"
    );
    assert_eq!(phase.player.position(), BIRCHS_LAB_ARRIVAL);
    let data = &phase.save1.event_data;
    assert_eq!(
        data.var_get(VAR_ROUTE101_STATE),
        Ok(ROUTE101_STATE_CONCLUDED)
    );
    assert_eq!(
        data.var_get(VAR_BIRCH_LAB_STATE),
        Ok(BIRCH_LAB_STATE_STARTER_GIVEN)
    );
    assert_eq!(data.var_get(VAR_STARTER_MON), Ok(STARTER_MON_TREECKO));
    assert_eq!(
        data.flag_get(FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH),
        Ok(false)
    );
}

/// Upstream's `warp` cannot fail (`src/scrcmd.c:740-751`), so a lab that will
/// not load must not leave the conclusion half applied: the player stays on
/// Route 101, none of the story vars or hide flags the script writes before
/// its `warp` survives, and the rescue is re-armed so a later landing on the
/// trigger tile retries it. The heal is kept (`HealPlayerParty` is idempotent
/// and no fainted lead may reach the overworld).
#[test]
fn a_failed_lab_load_rolls_the_conclusion_back_and_the_rescue_retries() {
    for (seed, lead, expected_outcome) in [
        (
            WINNING_SEED,
            new_game::provisional_starter(),
            BattleOutcome::PlayerWon,
        ),
        (
            LOSING_SEED,
            fragile_treecko_lead(),
            BattleOutcome::PlayerLost,
        ),
    ] {
        let (tx, ty) = ROUTE_101_TRIGGER_TILE;
        let lab = crate::overworld::test_support::BirchsLabPack::new();
        let mut phase = route_101_trigger_phase_from_source(
            PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
            unloadable_pack_source(),
        );
        phase.rng = Rng::new(seed);
        phase.set_fresh_lead_for_test(lead);
        phase.save1.money = STARTING_MONEY;
        let event_data = &mut phase.save1.event_data;
        event_data
            .var_set(VAR_STARTER_MON, STARTER_MON_MUDKIP)
            .unwrap();
        event_data
            .var_set(VAR_BIRCH_LAB_STATE, BIRCH_LAB_STATE_PRESET)
            .unwrap();
        event_data
            .var_set(VAR_UNRELATED_SENTINEL, UNRELATED_SENTINEL_VALUE)
            .unwrap();
        event_data
            .flag_set(FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH)
            .unwrap();
        let max_hp = phase.lead_for_test().unwrap().stats().max_hp;

        play_first_battle_to_conclusion(&mut phase);

        assert_eq!(
            phase.first_battle_outcome(),
            Some(expected_outcome),
            "setup: seed {seed} must produce {expected_outcome:?}"
        );
        assert_conclusion_rolled_back(&phase, expected_outcome, max_hp);

        // Standing on the trigger tile does not restart the fight.
        for _ in 0..10 {
            phase.step(platform::ButtonState::default());
            assert!(
                !phase.is_first_battle_active(),
                "no restart without a landing"
            );
        }

        // The asset source comes back; stepping off and landing on the trigger
        // tile again runs the rescue, and this time the handoff completes.
        phase.pack_source = lab.source();
        turn_and_walk_one_tile(&mut phase, Buttons::LEFT);
        turn_and_walk_one_tile(&mut phase, Buttons::RIGHT);
        assert!(
            phase.is_first_battle_active(),
            "{expected_outcome:?}: the re-armed rescue fires again"
        );
        let mut frames = 0;
        while phase.is_first_battle_active() {
            phase.step(held(Buttons::RIGHT));
            frames += 1;
            assert!(frames < 20, "the fixture battles must resolve quickly");
        }
        assert_retried_handoff_landed(&phase);
    }
}

/// With a pack source that cannot load the lab, the rescue must not read
/// concluded while the player is still on Route 101.
#[test]
fn a_failed_lab_load_leaves_the_rescue_unconcluded() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase_from_source(
        PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        unloadable_pack_source(),
    );
    phase.rng = Rng::new(WINNING_SEED);
    phase.set_fresh_lead_for_test(new_game::provisional_starter());
    phase
        .save1
        .event_data
        .flag_set(FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH)
        .unwrap();

    play_first_battle_to_conclusion(&mut phase);

    assert_eq!(phase.first_battle_outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(
        phase.map_id, ROUTE_101,
        "setup: the lab warp must have failed"
    );
    assert_ne!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(ROUTE101_STATE_CONCLUDED),
        "rescue marked concluded although the player never left Route 101"
    );
    assert_eq!(
        phase
            .save1
            .event_data
            .flag_get(FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE),
        Ok(false),
        "Route 101 Birch hidden although the player never left Route 101"
    );
}
