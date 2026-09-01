use super::{
    ensure_resolvable, is_flag_move_effect, resolve_flag_move, FlagMoveOutcome, EFFECT_CHARGE,
    EFFECT_FOCUS_ENERGY, EFFECT_SPLASH,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::volatile::Volatiles;
use assets::{MoveId, SpeciesId};

const SPLASH: MoveId = MoveId(150);
const FOCUS_ENERGY: MoveId = MoveId(116);
const CHARGE: MoveId = MoveId(268);
const DEFENSE_CURL: MoveId = MoveId(111);
const TACKLE: MoveId = MoveId(33);
const TEST_SPECIES: SpeciesId = SpeciesId(1);
const TEST_LEVEL: u8 = 5;
const TEST_PERSONALITY: u32 = 0;

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

fn mon(dex: &Dex, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        TEST_SPECIES,
        TEST_LEVEL,
        MAX_IVS,
        TEST_PERSONALITY,
        moves,
    )
    .unwrap()
}

#[test]
fn the_three_flag_move_effects_are_non_damaging_and_resolvable() {
    let dex = Dex::new();
    for (move_id, effect) in [
        (SPLASH, EFFECT_SPLASH),
        (FOCUS_ENERGY, EFFECT_FOCUS_ENERGY),
        (CHARGE, EFFECT_CHARGE),
    ] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, effect);
        assert!(is_flag_move_effect(effect));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
        assert_eq!(dex.move_data(move_id).unwrap().power, 0);
    }

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

#[test]
fn splash_always_reports_that_nothing_happened() {
    let dex = Dex::new();
    let attacker = mon(&dex, vec![SPLASH]);
    assert_eq!(
        resolve_flag_move(&dex, SPLASH, &attacker).unwrap(),
        FlagMoveOutcome::NothingHappened
    );
    let mut attacker_with_other_flags_active = attacker.clone();
    attacker_with_other_flags_active
        .volatiles_mut()
        .set_focus_energy();
    attacker_with_other_flags_active
        .volatiles_mut()
        .set_charge();
    assert_eq!(
        resolve_flag_move(&dex, SPLASH, &attacker_with_other_flags_active).unwrap(),
        FlagMoveOutcome::NothingHappened
    );
}

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
        "Charge remains successful after its existing timer advances"
    );
}

#[test]
fn no_flag_move_can_draw_because_none_is_given_a_stream() {
    let dex = Dex::new();
    let mut pumped = mon(&dex, vec![SPLASH, FOCUS_ENERGY, CHARGE]);
    pumped.volatiles_mut().set_focus_energy();
    for move_id in [SPLASH, FOCUS_ENERGY, CHARGE] {
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
