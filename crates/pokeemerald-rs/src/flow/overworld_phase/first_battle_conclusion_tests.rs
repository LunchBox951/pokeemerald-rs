//! Tests for [`super::first_battle_conclusion`] (issue #251):
//! `Route101_EventScript_BirchsBag`'s post-battle heal, its three var
//! writes, and the warp to Birch's lab -- pinned per-area, the same split
//! `first_battle_trigger_tests`/`route103_rival_tests` already use.

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use assets::{MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs, MAX_IV};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::Buttons;

/// One of the real Route 101 rescue coord-event trigger tiles -- see
/// `super::first_battle_trigger_tests`'s own identical constant for the
/// full citation; independently transcribed here too, the same "each test
/// file names its own fixture" convention this crate's test suite already
/// keeps throughout.
const ROUTE_101_TRIGGER_TILE: (i32, i32) = (10, 19);

/// [`ROUTE_101_TRIGGER_TILE`]'s own elevation.
const ROUTE_101_TRIGGER_ELEVATION: u8 = 3;

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`) -- independently
/// transcribed, matching this crate's own "own copy per module" convention.
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// `VAR_BIRCH_LAB_STATE` (`include/constants/vars.h:152`).
const VAR_BIRCH_LAB_STATE: u16 = 0x4084;

/// `VAR_STARTER_MON` (`include/constants/vars.h:53`).
const VAR_STARTER_MON: u16 = 0x4023;

/// A synthetic, fully open room named `MAP_ROUTE101` -- [`super::first_battle_trigger_tests`]'s
/// own `route_101_trigger_phase` fixture, reconstructed locally (this
/// module cannot import a sibling test file's private items).
fn route_101_trigger_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(25, 25),
        assets::MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

/// Walk onto [`ROUTE_101_TRIGGER_TILE`] from one tile west and play the
/// scripted first battle out to a terminal outcome, `held(Buttons::RIGHT)`
/// every frame (mirroring `first_battle_trigger_tests`' own pattern) --
/// the shared setup every test below needs before it can inspect what
/// [`super::OverworldPhase::conclude_first_battle`] left behind.
fn play_first_battle_to_conclusion(phase: &mut OverworldPhase) {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    assert_eq!(
        phase.player.position(),
        (tx - 1, ty),
        "setup: the phase must start one tile west of the trigger"
    );
    walk_one_tile_east(phase);
    assert_eq!(phase.player.position(), (tx, ty));
    assert!(phase.first_battle.is_some(), "setup: the trigger must fire");

    let mut frames = 0;
    while phase.first_battle.is_some() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 20, "the fixture battles must resolve quickly");
    }
}

/// A deliberately fragile level-1 Treecko (zero Defense IV, Pound only)
/// whose max HP a level-2 Zigzagoon's Tackle can overkill in one hit from
/// above the `AI_FirstBattle` flee threshold -- see
/// `crate::flow::wild_encounter::tests::a_lost_route_101_first_battle_heals_the_lead_instead_of_leaving_it_fainted`'s
/// own doc comment for the full derivation of why seed `2` reliably
/// produces [`BattleOutcome::PlayerLost`] with this exact lead.
fn fragile_treecko_lead() -> BattlePokemon {
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: 0,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), SpeciesId(277), 1, ivs, 0, vec![MoveId(1)])
        .expect("Treecko/Pound must be in the dex")
}

/// The heal runs on a **win** too, and is observable: seed `4242` (the same
/// seed `first_battle_trigger_tests` already uses for its own passing
/// scenario) has the provisional starter take real damage and spend real PP
/// on the way to victory, so a heal that silently no-op'd would be
/// indistinguishable from one that never ran without this.
#[test]
fn conclude_first_battle_heals_the_lead_on_a_win() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());
    let max_hp = phase.party_lead.as_ref().unwrap().stats().max_hp;
    let base_pp = phase.party_lead.as_ref().unwrap().moves()[0].pp;

    play_first_battle_to_conclusion(&mut phase);

    assert_eq!(
        phase.first_battle_outcome(),
        Some(BattleOutcome::PlayerWon),
        "setup: this seed/lead combination must really win"
    );
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the lead is still present");
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

/// The three var writes run on **either** terminal outcome (module docs,
/// "The upstream chain": `CB2_EndFirstBattle` never branches on the
/// outcome) -- proven side by side rather than duplicated per outcome.
#[test]
fn conclude_first_battle_writes_all_three_vars_on_both_outcomes() {
    for (seed, lead, expected_outcome) in [
        (
            4242,
            new_game::provisional_starter(),
            BattleOutcome::PlayerWon,
        ),
        (2, fragile_treecko_lead(), BattleOutcome::PlayerLost),
    ] {
        let (tx, ty) = ROUTE_101_TRIGGER_TILE;
        let mut phase = route_101_trigger_phase(PlayerState::new(
            (tx - 1, ty),
            ROUTE_101_TRIGGER_ELEVATION,
            Direction::East,
        ));
        phase.rng = Rng::new(seed);
        phase.party_lead = Some(lead);

        play_first_battle_to_conclusion(&mut phase);

        assert_eq!(
            phase.first_battle_outcome(),
            Some(expected_outcome),
            "setup: seed {seed} must produce {expected_outcome:?}"
        );
        assert_eq!(
            phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
            Ok(3),
            "outcome {expected_outcome:?}: setvar VAR_ROUTE101_STATE, 3"
        );
        assert_eq!(
            phase.save1.event_data.var_get(VAR_BIRCH_LAB_STATE),
            Ok(2),
            "outcome {expected_outcome:?}: setvar VAR_BIRCH_LAB_STATE, 2"
        );
        assert_eq!(
            phase.save1.event_data.var_get(VAR_STARTER_MON),
            Ok(0),
            "outcome {expected_outcome:?}: VAR_STARTER_MON reads Treecko's own encoding"
        );
    }
}

/// `Route101_EventScript_BirchsBag`'s own `HealPlayerParty` has no
/// accompanying `SetMoney(.../2)` -- unlike [`super::white_out`], this
/// conclusion must never halve the player's money, on either outcome
/// (module docs, "What's modelled, narrowly": halving money for a
/// story-mandated fight would be a fidelity bug, not a shortcut).
#[test]
fn conclude_first_battle_never_halves_money_even_on_a_loss() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(2);
    phase.party_lead = Some(fragile_treecko_lead());
    phase.save1.money = 2001;

    play_first_battle_to_conclusion(&mut phase);

    assert_eq!(
        phase.first_battle_outcome(),
        Some(BattleOutcome::PlayerLost),
        "setup: this seed/lead combination must really lose"
    );
    assert_eq!(
        phase.save1.money, 2001,
        "the Birch's-bag heal must never halve money, unlike white_out's own DoWhiteOut"
    );
}

/// An **aborted** first battle (module docs, "The upstream chain": no real
/// outcome, no upstream counterpart) must not run the conclusion at all --
/// no heal, no var writes, no warp. Mirrors
/// `first_battle_trigger_tests::an_aborted_first_battle_still_consumes_the_route_101_trigger`'s
/// own PP-drain setup, since that is the one reachable way
/// `crate::flow::first_battle::advance_first_battle` reports `None` instead
/// of a real outcome.
#[test]
fn an_aborted_first_battle_does_not_run_the_conclusion() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    let mut lead = new_game::provisional_starter();
    let starting_pp = lead.moves()[0].pp;
    for _ in 0..starting_pp {
        lead.deduct_pp(0)
            .expect("draining a slot that still has PP");
    }
    phase.party_lead = Some(lead);

    walk_one_tile_east(&mut phase);
    assert!(phase.first_battle.is_some(), "setup: the trigger must fire");

    // One driver frame: the turn fails pre-draw (no PP), so the battle ends
    // with no outcome at all -- and no conclusion may run off it.
    phase.step(held(Buttons::RIGHT));
    assert!(
        phase.first_battle.is_none(),
        "setup: the aborted battle emptied the slot"
    );
    assert_eq!(
        phase.first_battle_outcome(),
        None,
        "setup: an abort reports no outcome"
    );

    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "the trigger's own TRIGGER_CONSUMED_STATE, not the conclusion's terminal 3"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_BIRCH_LAB_STATE),
        Ok(0),
        "no conclusion ran, so this var must still read its fresh-save default"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_STARTER_MON),
        Ok(0),
        "unwritten, so this reads the same 0 default VAR_STARTER_MON always starts at -- not \
         proof either way, but consistent with no write having happened"
    );
    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_ROUTE101"),
        "no conclusion ran, so no warp fired -- the player stays exactly where the aborted \
         battle left them"
    );
}

/// The real-pack acceptance path: a lost Route 101 first battle really does
/// land the player inside Birch's lab, on the exact tile the `warp`
/// command names (`scripts.inc:245`) -- the standing proof the lab's own
/// bundled layout/tileset (module docs, "What's modelled, narrowly") are
/// enough for [`super::OverworldPhase::warp_to_position`] to resolve a real
/// room, not merely an id that happens to parse.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_a_lost_first_battle_still_heals_and_reaches_the_lab() {
    let lab = assets::MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB");
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(2);
    phase.party_lead = Some(fragile_treecko_lead());

    play_first_battle_to_conclusion(&mut phase);

    assert_eq!(
        phase.first_battle_outcome(),
        Some(BattleOutcome::PlayerLost),
        "setup: this seed/lead combination must really lose"
    );
    let lead = phase
        .party_lead
        .as_ref()
        .expect("healed lead still present");
    assert!(
        !lead.is_fainted(),
        "healed even against the real pack's own dex data"
    );
    assert_eq!(
        phase.map_id, lab,
        "warp MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5 must actually resolve the lab"
    );
    assert_eq!(
        phase.player.position(),
        (6, 5),
        "the warp's own explicit coordinates"
    );
}
