use super::{
    ensure_resolvable, is_flag_move_effect, resolve_flag_move, FlagMoveOutcome, EFFECT_CHARGE,
    EFFECT_FOCUS_ENERGY, EFFECT_SPLASH,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::volatile::Volatiles;
use assets::{MoveId, SpeciesId};

/// `MOVE_SPLASH`.
const SPLASH: MoveId = MoveId(150);
/// `MOVE_FOCUS_ENERGY`.
const FOCUS_ENERGY: MoveId = MoveId(116);
/// `MOVE_CHARGE`.
const CHARGE: MoveId = MoveId(268);
/// `MOVE_DEFENSE_CURL` — deliberately *not* in this pipeline (module docs).
const DEFENSE_CURL: MoveId = MoveId(111);
/// `MOVE_TACKLE`, the plain-hit control.
const TACKLE: MoveId = MoveId(33);

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

fn mon(dex: &Dex, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(1), 5, MAX_IVS, 0, moves).unwrap()
}

#[test]
fn exactly_three_effects_are_flag_only() {
    let dex = Dex::new();
    for (move_id, effect) in [
        (SPLASH, EFFECT_SPLASH),
        (FOCUS_ENERGY, EFFECT_FOCUS_ENERGY),
        (CHARGE, EFFECT_CHARGE),
    ] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, effect);
        assert!(is_flag_move_effect(effect));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
        // All three are `0` base power, which is why the hit pipeline
        // rejects them with `NonDamagingMove` before its own effect check
        // and `ensure_executable` has to fall through to this one.
        assert_eq!(dex.move_data(move_id).unwrap().power, 0);
    }

    // Defense Curl is the documented boundary: it also raises a stat, so it
    // belongs to the stat-change family (issue #322), not here.
    assert!(!is_flag_move_effect(
        dex.move_data(DEFENSE_CURL).unwrap().effect
    ));
    assert_eq!(
        ensure_resolvable(&dex, DEFENSE_CURL),
        Err(BattleError::UnsupportedMoveEffect(DEFENSE_CURL))
    );
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

/// Splash prints and does nothing — no accuracy check, no failure branch.
#[test]
fn splash_always_reports_that_nothing_happened() {
    let dex = Dex::new();
    let attacker = mon(&dex, vec![SPLASH]);
    assert_eq!(
        resolve_flag_move(&dex, SPLASH, &attacker).unwrap(),
        FlagMoveOutcome::NothingHappened
    );
    // Even from a battler that is somehow already pumped and charged: Splash
    // reads no state at all.
    let mut odd = attacker.clone();
    odd.volatiles_mut().set_focus_energy();
    odd.volatiles_mut().set_charge();
    assert_eq!(
        resolve_flag_move(&dex, SPLASH, &odd).unwrap(),
        FlagMoveOutcome::NothingHappened
    );
}

/// Focus Energy's failure branch is the **script's** `jumpifstatus2`
/// (`data/battle_scripts_1.s:889`), so a second use fails rather than
/// re-setting the bit.
#[test]
fn focus_energy_fails_on_an_already_pumped_user() {
    let dex = Dex::new();
    let fresh = mon(&dex, vec![FOCUS_ENERGY]);
    assert_eq!(fresh.volatiles(), Volatiles::default());
    assert_eq!(
        resolve_flag_move(&dex, FOCUS_ENERGY, &fresh).unwrap(),
        FlagMoveOutcome::GettingPumped
    );

    let mut pumped = fresh.clone();
    pumped.volatiles_mut().set_focus_energy();
    assert_eq!(
        resolve_flag_move(&dex, FOCUS_ENERGY, &pumped).unwrap(),
        FlagMoveOutcome::Failed
    );
}

/// Charge has **no** failure branch: using it while already charged simply
/// restarts the timer, and the script prints the same string.
#[test]
fn charge_never_fails_even_when_already_charged() {
    let dex = Dex::new();
    let fresh = mon(&dex, vec![CHARGE]);
    assert_eq!(
        resolve_flag_move(&dex, CHARGE, &fresh).unwrap(),
        FlagMoveOutcome::ChargingPower
    );

    let mut charged = fresh.clone();
    charged.volatiles_mut().set_charge();
    charged.volatiles_mut().tick_charge();
    assert_eq!(
        resolve_flag_move(&dex, CHARGE, &charged).unwrap(),
        FlagMoveOutcome::ChargingPower,
        "Charge has no BattleScript_ButItFailed branch"
    );
}

/// The headline claim, made unforgeable by the signature: [`resolve_flag_move`]
/// takes no `rng`, so none of the three scripts can spend a draw on any
/// path — including the failure path. This test is the *readable* statement
/// of what the type system already enforces.
#[test]
fn no_flag_move_can_draw_because_none_is_given_a_stream() {
    let dex = Dex::new();
    let mut pumped = mon(&dex, vec![SPLASH, FOCUS_ENERGY, CHARGE]);
    pumped.volatiles_mut().set_focus_energy();
    for move_id in [SPLASH, FOCUS_ENERGY, CHARGE] {
        // Compiles only because the call needs no `BattleRng` at all.
        assert!(resolve_flag_move(&dex, move_id, &pumped).is_ok());
    }
}

#[test]
fn an_unknown_move_is_reported_as_such() {
    let dex = Dex::new();
    let attacker = mon(&dex, vec![SPLASH]);
    let unknown = MoveId(60_000);
    assert_eq!(
        resolve_flag_move(&dex, unknown, &attacker),
        Err(BattleError::UnknownMove(unknown))
    );
    assert_eq!(
        ensure_resolvable(&dex, unknown),
        Err(BattleError::UnknownMove(unknown))
    );
}
