use super::{
    drain_amount, ensure_resolvable, is_drain_effect, resolve_drain, resolve_drain_move,
    EFFECT_ABSORB,
};
use crate::ability::{suppresses_critical_hits, LIQUID_OOZE, OVERGROW};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::HitOutcome;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use assets::species::AbilityId;
use assets::{MoveId, SpeciesId};

/// `MOVE_ABSORB`.
const ABSORB: MoveId = MoveId(71);
/// `MOVE_MEGA_DRAIN`.
const MEGA_DRAIN: MoveId = MoveId(72);
/// `MOVE_GIGA_DRAIN`.
const GIGA_DRAIN: MoveId = MoveId(202);
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

fn mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), level, MAX_IVS, 0, moves).unwrap()
}

/// The reference scenario every damage pin below is derived from, hand
/// computed from upstream's arithmetic rather than from this crate's output:
///
/// Bulbasaur (species 1) level 5, max IVs, Hardy, using Absorb (move 71,
/// power 20, accuracy 100, Grass/**special**) against Squirtle (species 7)
/// level 5, max IVs, Hardy.
///
/// - Bulbasaur Sp. Atk = `CALC_STAT(base 65, iv 31, lvl 5)` =
///   `(2*65+31)*5/100 + 5` = `805/100 = 8`, `+5` = **13**.
/// - Squirtle Sp. Def = `(2*64+31)*5/100 + 5` = `795/100 = 7`, `+5` = **12**.
/// - `CalculateBaseDamage`: `13 * 20 = 260`; `* (2*5/5 + 2 = 4)` = 1040;
///   `/ 12` = 86; `/ 50` = 1; `+ 2` = **3**.
/// - STAB: Bulbasaur is Grass/Poison and Absorb is Grass, `3*15/10` = **4**.
/// - Type: Grass into pure Water is `x2` = **8**.
/// - Best damage roll (100%) leaves it at **8**.
const PINNED_ABSORB_DAMAGE: u32 = 8;

#[test]
fn only_effect_absorb_is_a_drain_effect() {
    let dex = Dex::new();
    for move_id in [ABSORB, MEGA_DRAIN, GIGA_DRAIN] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, EFFECT_ABSORB);
        assert!(is_drain_effect(dex.move_data(move_id).unwrap().effect));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }
    assert!(!is_drain_effect(dex.move_data(TACKLE).unwrap().effect));
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    let unknown = MoveId(60_000);
    assert_eq!(
        ensure_resolvable(&dex, unknown),
        Err(BattleError::UnknownMove(unknown))
    );
}

/// `Cmd_negativedamage`: halve, **then** floor at 1. The floor after the
/// halving is what makes a 1-damage hit still heal 1 rather than 0.
#[test]
fn drain_amount_halves_then_floors() {
    assert_eq!(
        drain_amount(0),
        0,
        "a move that dealt nothing heals nothing"
    );
    assert_eq!(drain_amount(1), 1, "the floor fires after the halving");
    assert_eq!(drain_amount(2), 1);
    assert_eq!(drain_amount(3), 1, "truncating division, not rounding");
    assert_eq!(drain_amount(8), 4);
    assert_eq!(drain_amount(999), 499);
}

/// The Liquid Ooze branch flips the sign and nothing else — same magnitude,
/// same `-1` floor, applied to the attacker instead.
#[test]
fn liquid_ooze_inverts_the_same_magnitude() {
    assert_eq!(resolve_drain(0, LIQUID_OOZE), None, "no effect, no branch");
    let healed = resolve_drain(9, OVERGROW).unwrap();
    let hurt = resolve_drain(9, LIQUID_OOZE).unwrap();
    assert_eq!(healed.amount, 4);
    assert!(!healed.inverted);
    assert_eq!(hurt.amount, healed.amount, "magnitude is untouched");
    assert!(hurt.inverted);
    // The floor survives the flip: a 1-damage drain costs the attacker 1.
    assert_eq!(resolve_drain(1, LIQUID_OOZE).unwrap().amount, 1);
    // An unrelated ability heals normally.
    assert!(!resolve_drain(9, AbilityId(1)).unwrap().inverted);
}

/// A landed drain move costs **3** draws, one fewer than a landed ordinary
/// move: `BattleScript_EffectAbsorb` ends with `goto BattleScript_MoveEnd`
/// and never reaches `seteffectwithchance`. The sequence is exactly 3 long,
/// so a stray fourth draw would panic.
#[test]
fn a_landed_drain_move_draws_exactly_three_times() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![ABSORB]);
    let defender = mon(&dex, 7, 5, vec![TACKLE]);
    // draw0: accuracy (Absorb is 100-accuracy, so any value hits).
    // draw1: crit roll 1 -> 1 % 16 != 0 -> no crit.
    // draw2: damage roll 0 -> 100%.
    let mut rng = SequenceRng::new([0, 1, 0]);
    let outcome = resolve_drain_move(&dex, ABSORB, &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: PINNED_ABSORB_DAMAGE,
            is_critical: false,
        }
    );
    assert_eq!(
        rng.draws(),
        3,
        "no seteffectwithchance draw for a drain move"
    );
}

/// A miss ends the script at the `accuracycheck`, for **1** draw.
#[test]
fn a_missed_drain_move_draws_once() {
    let dex = Dex::new();
    // Mega Drain is also 100-accuracy, so use an accuracy stage to force a
    // miss instead: -6 accuracy on the attacker makes the roll
    // `100 * 33/100 = 33`, which a draw of 40 (-> roll 41) exceeds.
    let mut attacker = mon(&dex, 1, 5, vec![MEGA_DRAIN]);
    attacker.stages_mut().accuracy = crate::stat_stage::StatStage::MIN;
    let defender = mon(&dex, 7, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([40]);
    let outcome =
        resolve_drain_move(&dex, MEGA_DRAIN, &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(outcome, HitOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

/// `suppress_crit` (`BATTLE_TYPE_FIRST_BATTLE`) drops the landed row to 2,
/// exactly as it drops every row in `crate::hit`'s table.
#[test]
fn suppressing_the_crit_drops_a_landed_drain_to_two_draws() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![ABSORB]);
    let defender = mon(&dex, 7, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0]);
    let outcome = resolve_drain_move(&dex, ABSORB, &attacker, &defender, true, &mut rng).unwrap();
    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: PINNED_ABSORB_DAMAGE,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), 2);
}

/// A Battle Armor defender drops a landed drain to **2** draws (accuracy +
/// damage roll, no crit) instead of the plain 3, exactly as `suppress_crit`
/// does above -- but needing no caller flag (issue #391).
///
/// Bulbasaur (Sp. Atk 13, the reference scenario's own figure) using Absorb
/// against Anorith (species 390, Battle Armor; Sp. Def base 50):
/// - Anorith Sp. Def = `(2*50+31)*5/100+5` = `655/100=6`, `+5` = **11**.
/// - `13*20=260`; `*4=1040`; `/11=94`; `/50=1`; `+2=3`.
/// - STAB (Grass on a Grass/Poison attacker): `3*15/10=4`.
/// - Type: Grass into Rock is `x2` (`4*20/10=8`), then into Bug is `x0.5`
///   (`8*5/10=4`) -- the two type legs are applied one at a time, not
///   combined first, so the intermediate 8 matters even though the net
///   multiplier is `x1`.
/// - Best roll (100%) leaves it at **4**.
#[test]
fn a_battle_armor_defender_drops_a_landed_drain_to_two_draws() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![ABSORB]);
    let defender = mon(&dex, 390, 5, vec![TACKLE]); // Anorith, Battle Armor
    assert!(suppresses_critical_hits(defender.ability()));

    let mut rng = SequenceRng::new([0, 0]);
    let outcome = resolve_drain_move(&dex, ABSORB, &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: 4,
            is_critical: false,
        }
    );
    assert_eq!(
        rng.draws(),
        2,
        "Battle Armor must skip the crit draw on a drain move too"
    );
}

/// Overgrow, the ability this pipeline exposes: at `hp <= maxHP / 3` a Grass
/// move's **power** goes from 20 to 30, and the hand computation follows it
/// all the way through — `13*30 = 390`, `*4 = 1560`, `/12 = 130`, `/50 = 2`,
/// `+2 = 4`; STAB `4*15/10 = 6`; `x2` for Grass into Water = **12**.
///
/// Bulbasaur's level-5 max HP is `CALC_HP(base 45, iv 31, lvl 5)` =
/// `(2*45+31)*5/100 + level + 10` = `605/100 = 6`, `+15` = **21**, so the
/// gate is `21/3 = 7` and the boost starts at 7 HP.
#[test]
fn overgrow_boosts_a_low_hp_grass_drain_in_the_damage_path() {
    let dex = Dex::new();
    let defender = mon(&dex, 7, 5, vec![TACKLE]);

    let healthy = mon(&dex, 1, 5, vec![ABSORB]);
    assert_eq!(healthy.ability(), OVERGROW, "Bulbasaur's slot-0 ability");
    assert_eq!(healthy.stats().max_hp, 21);

    let mut pinched = healthy.clone();
    pinched.apply_damage(21 - 7); // exactly maxHP / 3
    assert_eq!(pinched.current_hp(), 7);

    let mut rng = SequenceRng::new([0, 1, 0]);
    let plain = resolve_drain_move(&dex, ABSORB, &healthy, &defender, false, &mut rng).unwrap();
    let mut rng = SequenceRng::new([0, 1, 0]);
    let boosted = resolve_drain_move(&dex, ABSORB, &pinched, &defender, false, &mut rng).unwrap();
    assert_eq!(
        plain,
        HitOutcome::Hit {
            damage: PINNED_ABSORB_DAMAGE,
            is_critical: false
        }
    );
    assert_eq!(
        boosted,
        HitOutcome::Hit {
            damage: 12,
            is_critical: false
        }
    );

    // One HP above the gate is not boosted -- the `<=` is on `maxHP / 3`.
    let mut just_above = healthy.clone();
    just_above.apply_damage(21 - 8);
    let mut rng = SequenceRng::new([0, 1, 0]);
    assert_eq!(
        resolve_drain_move(&dex, ABSORB, &just_above, &defender, false, &mut rng).unwrap(),
        plain
    );
}

/// A resisted matchup (a real net multiplier `< 1`, not two type legs that
/// cancel back to neutral) still lands and still costs the same 3 draws.
/// No Grass move has a *type immunity* to reach — Grass is `x0` into
/// nothing — so the immune row of this pipeline's table is unreachable in
/// practice, and the immunity shape is pinned in `crate::fixed_damage`'s
/// tests (Normal into Ghost) instead of being faked here.
#[test]
fn a_resisted_drain_still_lands_for_the_same_three_draws() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![ABSORB]);
    // Caterpie (10) is pure Bug, so Grass is a clean `x0.5` with no second
    // type to cancel it back out (unlike a Water/Poison target, where
    // Grass's `x2` into Water and `x0.5` into Poison multiply back to
    // neutral). Its Sp. Def is 20 base: `(2*20+31)*5/100 + 5` = `355/100 =
    // 3`, `+5` = **8**.
    // `13*20 = 260`, `*4 = 1040`, `/8 = 130`, `/50 = 2`, `+2 = 4`;
    // STAB `4*15/10 = 6`; `x0.5` leaves **3**.
    let resisted = mon(&dex, 10, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 1, 0]);
    let outcome = resolve_drain_move(&dex, ABSORB, &attacker, &resisted, false, &mut rng).unwrap();
    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: 3,
            is_critical: false
        }
    );
    assert_eq!(rng.draws(), 3);
}

/// The `gHpDealt` contract, at unit level: [`drain_amount`] takes the HP the
/// target *really* lost, so the same 999-damage formula output drains 2 HP
/// against a 5-HP target and 499 against a healthy one.
///
/// This pins the *arithmetic* only. That the caller actually feeds it the
/// clamped figure — the mistake worth catching — is pinned at turn level in
/// `crates/battle/tests/turn_engine/pipelines.rs`, because nothing at this
/// level can observe the wiring.
#[test]
fn the_drain_derives_from_hp_dealt_and_not_from_the_formula_output() {
    let raw_formula_output = 999;
    let target_hp_remaining = 5;
    assert_eq!(drain_amount(raw_formula_output.min(target_hp_remaining)), 2);
    assert_eq!(drain_amount(raw_formula_output), 499);

    // And the fixture the turn-level test drives really is small enough for
    // an overkill to be reachable: Squirtle's level-5 maximum HP is
    // `CALC_HP(base 44, iv 31, lvl 5)` = `(2*44+31)*5/100 + level + 10` =
    // `595/100 = 5`, `+15` = 20 -- fewer than three of this module's pinned
    // 8-damage Absorbs.
    let dex = Dex::new();
    let squirtle_max_hp = mon(&dex, 7, 5, vec![TACKLE]).stats().max_hp;
    assert_eq!(squirtle_max_hp, 20);
    assert!(3 * PINNED_ABSORB_DAMAGE > squirtle_max_hp);
}

/// An unsupported move never touches the RNG: the sequence is empty, so any
/// draw before the rejection would panic.
#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![ABSORB]);
    let defender = mon(&dex, 7, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_drain_move(&dex, TACKLE, &attacker, &defender, false, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(rng.draws(), 0);
}
