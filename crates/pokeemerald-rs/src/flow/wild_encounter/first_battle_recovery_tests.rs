//! First-battle recovery: a lost Route 101 first battle heals the lead the same frame it
//! ends, so a fainted lead never reaches a later roll.

use super::test_support::{held, route_101_phase_with_grass, walk_east_and_land};
use assets::{MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs, MAX_IV};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::Buttons;

/// **Test-ratchet replacement.** Before issue #251, this test
/// (`a_fainted_lead_from_a_lost_first_battle_draws_no_later_roll`) drove a
/// *directly written* fainted lead through grass and proved every step drew
/// nothing -- pinning `super::lead_can_fight`, the fail-closed guard that
/// existed only because a lost Route 101 first battle left a fainted lead
/// standing in the overworld with nothing to heal it (`CB2_EndFirstBattle`
/// has no `IsPlayerDefeated` branch, so no white-out ever ran).
///
/// `overworld_phase::first_battle_conclusion::OverworldPhase::conclude_first_battle`
/// (issue #251) now heals that lead in the very same frame the battle ends,
/// on every outcome -- so the guard's own precondition can no longer occur
/// at all, and it has been removed (`super`'s own module docs, "Lead-health
/// eligibility"). This test proves the *strictly stronger*
/// claim the old one could not: not "a roll is suppressed while the lead is
/// fainted," but "the lead is never fainted in the first place, past the
/// frame the battle ends" -- a claim that makes the deleted guard's own
/// precondition provably unreachable rather than merely refused.
///
/// Drives a real loss through the real trigger and driver -- not a directly
/// written fainted lead -- so the heal under test is
/// `conclude_first_battle`'s own, not asserted in isolation. The lead is a
/// deliberately crafted level-1 Treecko (zero Defense IV, Pound only) whose
/// 12 max HP a level-2 Zigzagoon's Tackle can overkill from `25%` (`3/12`,
/// above `AI_FirstBattle`'s `<=20%` flee threshold) straight to `0` in one
/// hit -- seed `1` was chosen by enumeration over the LCG (the same
/// technique `ENCOUNTER_SEED`'s own doc comment names) to produce exactly
/// that within a bounded number of turns; `crate::flow::first_battle::advance_first_battle`
/// always picks move slot 0, so the sequence is fully deterministic. (Picked
/// against the shared stream's current draw count -- `start_first_battle`'s
/// discarded `SetWildMonHeldItem` draw included -- so a future change to
/// that count would need to re-enumerate a working seed here too.)
#[test]
fn a_lost_route_101_first_battle_heals_the_lead_instead_of_leaving_it_fainted() {
    const TRIGGER_TILE: (i32, i32) = (10, 19);
    const TRIGGER_ELEVATION: u8 = 3;
    const VAR_ROUTE101_STATE: u16 = 0x4060;
    const VAR_BIRCH_LAB_STATE: u16 = 0x4084;
    const VAR_STARTER_MON: u16 = 0x4023;

    let (tx, ty) = TRIGGER_TILE;
    let mut phase = route_101_phase_with_grass(
        25,
        25,
        PlayerState::new((tx - 1, ty), TRIGGER_ELEVATION, Direction::East),
        &[],
    );
    phase.rng = Rng::new(1);
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: 0,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    let fragile_treecko =
        BattlePokemon::new(&Dex::new(), SpeciesId(277), 1, ivs, 0, vec![MoveId::POUND])
            .expect("Treecko/Pound must be in the dex");
    let max_hp = fragile_treecko.stats().max_hp;
    let base_pp = fragile_treecko.moves()[0].pp;
    phase.party_lead = Some(fragile_treecko);

    walk_east_and_land(&mut phase, 1);
    assert_eq!(
        phase.player.position(),
        (tx, ty),
        "setup: landed on the rescue trigger"
    );
    assert!(
        phase.is_first_battle_active(),
        "setup: the trigger must fire"
    );

    let mut frames = 0;
    while phase.is_first_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 20, "the crafted loss must resolve quickly");
    }
    assert_eq!(
        phase.first_battle_outcome(),
        Some(BattleOutcome::PlayerLost),
        "setup: the seed/lead combination must really lose"
    );

    let lead = phase
        .party_lead
        .as_ref()
        .expect("conclude_first_battle heals in place -- it does not clear the lead");
    assert!(
        !lead.is_fainted(),
        "a lost first battle must not leave a fainted lead standing in the overworld any more"
    );
    assert_eq!(
        lead.current_hp(),
        max_hp,
        "HealPlayerParty restores full HP"
    );
    assert_eq!(
        lead.moves()[0].pp,
        base_pp,
        "HealPlayerParty restores PP to its base value too"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(3),
        "Route101_EventScript_BirchsBag's own setvar VAR_ROUTE101_STATE, 3 -- even on a loss"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_BIRCH_LAB_STATE),
        Ok(2),
        "setvar VAR_BIRCH_LAB_STATE, 2 -- even on a loss"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_STARTER_MON),
        Ok(0),
        "VAR_STARTER_MON reads Treecko's own encoding (0 is also the var's fresh default -- \
         the overwrite itself is pinned by first_battle_conclusion_tests' pre-poisoned run; \
         the two non-default vars above prove the conclusion ran here)"
    );
    // Deliberately not asserting on `phase.map_id`/`phase.player.position()`
    // here: `conclude_first_battle`'s own warp to the lab depends on a local
    // asset pack being extracted (`OverworldPhase::warp_to_position`'s own
    // "leaves the player exactly where they stood" failure contract when one
    // isn't) -- this test must pass identically either way, so the warp
    // itself is `first_battle_conclusion_tests`' `#[ignore]`d real-pack
    // proof, not this one's.
}
