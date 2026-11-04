//! Admission and move-shape gating for [`resolve_hit`]: which moves
//! [`ensure_resolvable`] accepts (ordinary hits, Struggle, the poison and
//! confuse secondary-chance effects) and which it rejects, and in what order,
//! before any RNG draw.

use super::test_fixtures::{
    mon, BULBASAUR, GROWL, POISON_STING, PSYBEAM, SLASH, SQUIRTLE, SWIFT, TACKLE, UNKNOWN_MOVE,
};
use super::{ensure_resolvable, is_ordinary_hit_effect, resolve_hit};
use crate::damage::STRUGGLE;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::script_rng::SequenceRng;
use assets::MoveId;

const DOUBLE_SLAP: MoveId = MoveId::DOUBLE_SLAP;

const HORN_DRILL: MoveId = MoveId::HORN_DRILL;

const SONIC_BOOM: MoveId = MoveId::SONIC_BOOM;

const COUNTER: MoveId = MoveId::COUNTER;

const SEISMIC_TOSS: MoveId = MoveId::SEISMIC_TOSS;

const ABSORB: MoveId = MoveId::ABSORB;

const QUICK_ATTACK: MoveId = MoveId::QUICK_ATTACK;

const CURSE: MoveId = MoveId::CURSE;

const FALSE_SWIPE: MoveId = MoveId::FALSE_SWIPE;

const PURSUIT: MoveId = MoveId::PURSUIT;

/// `MOVE_SMOG`: `EFFECT_POISON_HIT`.
const SMOG: MoveId = MoveId::SMOG;

/// `MOVE_SLUDGE`: `EFFECT_POISON_HIT`.
const SLUDGE: MoveId = MoveId::SLUDGE;

/// `MOVE_SLUDGE_BOMB`: `EFFECT_POISON_HIT`.
const SLUDGE_BOMB: MoveId = MoveId::SLUDGE_BOMB;

/// `MOVE_POISON_TAIL`: `EFFECT_POISON_TAIL`, an unported trampoline.
const POISON_TAIL: MoveId = MoveId::POISON_TAIL;

#[test]
fn zero_power_moves_are_reported_as_non_damaging() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![GROWL]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);

    assert_eq!(
        resolve_hit(&dex, GROWL, &attacker, &defender, false, &mut rng),
        Err(BattleError::NonDamagingMove(GROWL))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn powered_moves_with_other_execution_rules_are_rejected_before_rng() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let unsupported_moves = [
        SONIC_BOOM,
        DOUBLE_SLAP,
        HORN_DRILL,
        COUNTER,
        SEISMIC_TOSS,
        ABSORB,
        FALSE_SWIPE,
        PURSUIT,
    ];

    for move_id in unsupported_moves {
        assert!(dex.move_data(move_id).unwrap().power > 0, "{move_id:?}");
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            resolve_hit(&dex, move_id, &attacker, &defender, false, &mut rng),
            Err(BattleError::UnsupportedMoveEffect(move_id)),
            "{move_id:?}"
        );
        assert_eq!(rng.draws(), 0, "{move_id:?}");
    }
}

#[test]
fn ordinary_hit_shaped_moves_and_struggle_are_accepted() {
    let dex = Dex::new();
    for move_id in [TACKLE, SLASH, SWIFT, QUICK_ATTACK] {
        let effect = dex.move_data(move_id).unwrap().effect;
        assert!(is_ordinary_hit_effect(effect), "{move_id:?}");
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }

    assert!(!is_ordinary_hit_effect(
        dex.move_data(STRUGGLE).unwrap().effect
    ));
    assert_eq!(ensure_resolvable(&dex, STRUGGLE), Ok(()));
}

#[test]
fn every_effect_poison_hit_move_is_admitted_though_not_ordinary() {
    let dex = Dex::new();
    for move_id in [POISON_STING, SMOG, SLUDGE, SLUDGE_BOMB] {
        let effect = dex.move_data(move_id).unwrap().effect;
        assert!(
            !is_ordinary_hit_effect(effect),
            "{move_id:?} needs the poison trampoline, not the plain hit script"
        );
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()), "{move_id:?}");
    }
}

#[test]
fn poison_tail_is_still_rejected_like_every_other_unported_trampoline() {
    let dex = Dex::new();
    assert_eq!(
        ensure_resolvable(&dex, POISON_TAIL),
        Err(BattleError::UnsupportedMoveEffect(POISON_TAIL))
    );
}

#[test]
fn effect_confuse_hit_is_admitted_though_not_ordinary() {
    let dex = Dex::new();
    let effect = dex.move_data(PSYBEAM).unwrap().effect;
    assert!(
        !is_ordinary_hit_effect(effect),
        "Psybeam needs the confuse trampoline, not the plain hit script"
    );
    assert_eq!(ensure_resolvable(&dex, PSYBEAM), Ok(()));
}

#[test]
fn admission_reports_unknown_and_non_damaging_moves_in_order() {
    let dex = Dex::new();
    assert_eq!(
        ensure_resolvable(&dex, UNKNOWN_MOVE),
        Err(BattleError::UnknownMove(UNKNOWN_MOVE))
    );
    assert_eq!(
        ensure_resolvable(&dex, GROWL),
        Err(BattleError::NonDamagingMove(GROWL))
    );
    assert_eq!(
        ensure_resolvable(&dex, CURSE),
        Err(BattleError::NonDamagingMove(CURSE))
    );
}
