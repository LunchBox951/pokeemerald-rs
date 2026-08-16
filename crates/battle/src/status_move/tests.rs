//! [`crate::status_move`]'s own unit tests: the four scripts' outcomes, the
//! ordering quirk Defense Curl depends on, and the fact that none of them
//! draws.

use super::{
    ensure_resolvable, is_status_move_effect, resolve_status_move, StatusMoveOutcome,
    DEFENSE_CURL_STAT_CHANGE, EFFECT_CHARGE, EFFECT_DEFENSE_CURL, EFFECT_FOCUS_ENERGY,
    EFFECT_SPLASH,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::stat_change::{ChangedStat, StatChangeDirection};
use crate::stat_stage::StatStage;
use assets::{MoveEffect, MoveId, MoveTarget, SpeciesId};

const SPLASH: MoveId = MoveId(150);
const FOCUS_ENERGY: MoveId = MoveId(116);
const DEFENSE_CURL: MoveId = MoveId(111);
const CHARGE: MoveId = MoveId(268);
const TACKLE: MoveId = MoveId(33);
const GROWTH: MoveId = MoveId(74);

fn mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), level, Ivs::default(), 0, moves).unwrap()
}

/// The four effect ids are the ones the real move table carries, and each is
/// `MOVE_TARGET_USER` with `0` base power -- the shared shape that makes a
/// target parameter unnecessary.
#[test]
fn the_four_effect_ids_match_the_real_move_table() {
    let dex = Dex::new();
    for (move_id, effect) in [
        (SPLASH, EFFECT_SPLASH),
        (FOCUS_ENERGY, EFFECT_FOCUS_ENERGY),
        (DEFENSE_CURL, EFFECT_DEFENSE_CURL),
        (CHARGE, EFFECT_CHARGE),
    ] {
        let mv = dex.move_data(move_id).unwrap();
        assert_eq!(mv.effect, effect, "move {}", move_id.0);
        assert_eq!(mv.power, 0, "move {}", move_id.0);
        assert_eq!(mv.target, MoveTarget::USER, "move {}", move_id.0);
        assert!(is_status_move_effect(mv.effect));
    }
    // Charge's `accuracy = 100` byte is inert -- its script has no
    // `accuracycheck` command to read it.
    assert_eq!(dex.move_data(CHARGE).unwrap().accuracy, 100);
    for move_id in [SPLASH, FOCUS_ENERGY, DEFENSE_CURL] {
        assert_eq!(dex.move_data(move_id).unwrap().accuracy, 0);
    }
}

#[test]
fn ensure_resolvable_rejects_everything_else() {
    let dex = Dex::new();
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    // Growth also raises a stat on the user, but through the *shared*
    // `BattleScript_EffectStatUp` tail -- `crate::stat_change` owns it.
    assert_eq!(
        ensure_resolvable(&dex, GROWTH),
        Err(BattleError::UnsupportedMoveEffect(GROWTH))
    );
    assert!(!is_status_move_effect(MoveEffect(13))); // EFFECT_SPECIAL_ATTACK_UP
    let bad = MoveId(60_000);
    assert_eq!(
        ensure_resolvable(&dex, bad),
        Err(BattleError::UnknownMove(bad))
    );
}

#[test]
fn splash_reports_nothing_happened_and_changes_no_state() {
    let dex = Dex::new();
    let user = mon(&dex, 129, 5, vec![SPLASH]); // Magikarp
    let before = user.clone();
    assert_eq!(
        resolve_status_move(&dex, SPLASH, &user).unwrap(),
        StatusMoveOutcome::NothingHappened
    );
    assert_eq!(user, before, "resolving must not mutate the user");
}

#[test]
fn focus_energy_sets_the_flag_once_and_then_fails() {
    let dex = Dex::new();
    let mut user = mon(&dex, 335, 15, vec![FOCUS_ENERGY]); // Makuhita
    assert_eq!(
        resolve_status_move(&dex, FOCUS_ENERGY, &user).unwrap(),
        StatusMoveOutcome::FocusEnergy
    );
    // The caller applies it; a second use then takes the script's
    // `jumpifstatus2 ... BattleScript_ButItFailed` branch.
    user.volatiles_mut().set_focus_energy();
    assert_eq!(
        resolve_status_move(&dex, FOCUS_ENERGY, &user).unwrap(),
        StatusMoveOutcome::Failed,
        "a second Focus Energy is `But it failed!`"
    );
}

/// Charge has no failure branch: using it while already charged restarts the
/// timer rather than failing (unlike Focus Energy).
#[test]
fn charge_always_succeeds_even_while_already_charged() {
    let dex = Dex::new();
    let mut user = mon(&dex, 100, 15, vec![CHARGE]); // Voltorb
    assert_eq!(
        resolve_status_move(&dex, CHARGE, &user).unwrap(),
        StatusMoveOutcome::Charge
    );
    user.volatiles_mut().set_charge();
    assert_eq!(
        resolve_status_move(&dex, CHARGE, &user).unwrap(),
        StatusMoveOutcome::Charge,
        "BattleScript_EffectCharge has no `But it failed!` path"
    );
}

#[test]
fn defense_curl_raises_the_users_own_defense_by_one() {
    let dex = Dex::new();
    let user = mon(&dex, 183, 15, vec![DEFENSE_CURL]); // Marill
    assert_eq!(
        resolve_status_move(&dex, DEFENSE_CURL, &user).unwrap(),
        StatusMoveOutcome::DefenseCurl {
            new_stage: StatStage::new(1).unwrap(),
            capped: false,
        }
    );
    assert_eq!(DEFENSE_CURL_STAT_CHANGE.stat, ChangedStat::Defense);
    assert_eq!(
        DEFENSE_CURL_STAT_CHANGE.direction,
        StatChangeDirection::Raise
    );
    assert_eq!(DEFENSE_CURL_STAT_CHANGE.delta(), 1);
    assert!(DEFENSE_CURL_STAT_CHANGE.affects_user());
}

/// The ordering quirk: `setdefensecurlbit` runs **before** the stat change
/// (`data/battle_scripts_1.s:2018`-`:2020`), so a user already at `+6`
/// Defense still gets the flag while its raise reports "won't go any
/// higher!". Here that is the `capped: true` outcome -- the caller is
/// required to set the bit on that path too, which
/// `crate::battle`'s own tests pin end to end.
#[test]
fn a_capped_defense_curl_still_reports_the_move_connecting() {
    let dex = Dex::new();
    let mut user = mon(&dex, 183, 15, vec![DEFENSE_CURL]);
    user.stages_mut().defense = StatStage::MAX;
    assert_eq!(
        resolve_status_move(&dex, DEFENSE_CURL, &user).unwrap(),
        StatusMoveOutcome::DefenseCurl {
            new_stage: StatStage::MAX,
            capped: true,
        }
    );
}

/// The whole point of the missing `rng` parameter: there is no way for any
/// of these four to touch the shared stream, so the caller's draw budget for
/// them is provably zero rather than merely documented as zero. (Enforced by
/// the signature; asserted here so the intent is visible in the test suite
/// too.)
#[test]
fn none_of_the_four_takes_an_rng_at_all() {
    let dex = Dex::new();
    let user = mon(&dex, 100, 15, vec![CHARGE, SPLASH]);
    // Each call type-checks without an `rng` argument -- if any script
    // gained a draw, this signature would have to change and every caller
    // with it.
    for move_id in [SPLASH, FOCUS_ENERGY, DEFENSE_CURL, CHARGE] {
        assert!(
            resolve_status_move(&dex, move_id, &user).is_ok(),
            "{move_id:?}"
        );
    }
}
