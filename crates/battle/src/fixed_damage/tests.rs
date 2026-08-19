use super::{
    ensure_resolvable, fixed_damage_for_effect, is_fixed_damage_effect, resolve_fixed_damage_move,
    FixedDamage, EFFECT_DRAGON_RAGE, EFFECT_LEVEL_DAMAGE, EFFECT_SONICBOOM, FIXED_DAMAGE_EFFECTS,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::HitOutcome;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use assets::{MoveId, SpeciesId};

/// `MOVE_SONIC_BOOM` (power 1, accuracy 90, Normal).
const SONIC_BOOM: MoveId = MoveId(49);
/// `MOVE_DRAGON_RAGE` (power 1, accuracy 100, Dragon).
const DRAGON_RAGE: MoveId = MoveId(82);
/// `MOVE_SEISMIC_TOSS` (power 1, accuracy 100, Fighting).
const SEISMIC_TOSS: MoveId = MoveId(69);
/// `MOVE_NIGHT_SHADE` (power 1, accuracy 100, Ghost).
const NIGHT_SHADE: MoveId = MoveId(101);
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

#[test]
fn the_four_moves_carry_the_three_transcribed_effects() {
    let dex = Dex::new();
    assert_eq!(dex.move_data(SONIC_BOOM).unwrap().effect, EFFECT_SONICBOOM);
    assert_eq!(
        dex.move_data(DRAGON_RAGE).unwrap().effect,
        EFFECT_DRAGON_RAGE
    );
    for move_id in [SEISMIC_TOSS, NIGHT_SHADE] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, EFFECT_LEVEL_DAMAGE);
    }
    for move_id in [SONIC_BOOM, DRAGON_RAGE, SEISMIC_TOSS, NIGHT_SHADE] {
        assert!(is_fixed_damage_effect(
            dex.move_data(move_id).unwrap().effect
        ));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
        // The whole point of this pipeline: base power alone would never
        // separate these from an ordinary move, because upstream gives each
        // of them power `1`.
        assert_eq!(dex.move_data(move_id).unwrap().power, 1);
    }
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

/// The literals are the two `setword`s and the one `dmgtolevel`, and the
/// level source really reads the *attacker's* level.
#[test]
fn each_effect_yields_its_own_upstream_figure() {
    assert_eq!(
        FIXED_DAMAGE_EFFECTS.map(|(effect, _)| effect),
        [EFFECT_DRAGON_RAGE, EFFECT_LEVEL_DAMAGE, EFFECT_SONICBOOM]
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_SONICBOOM),
        Some(FixedDamage::Literal(20))
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_DRAGON_RAGE),
        Some(FixedDamage::Literal(40))
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_LEVEL_DAMAGE),
        Some(FixedDamage::AttackerLevel)
    );
    assert_eq!(fixed_damage_for_effect(crate::drain::EFFECT_ABSORB), None);

    // A literal ignores the level; `dmgtolevel` is the level.
    assert_eq!(FixedDamage::Literal(20).amount(77), 20);
    assert_eq!(FixedDamage::AttackerLevel.amount(77), 77);
    assert_eq!(FixedDamage::AttackerLevel.amount(1), 1);
}

/// A landed fixed-damage move costs **2** draws: accuracy and the discarded
/// `seteffectwithchance` roll. No crit draw (there is no `critcalc`), no
/// damage roll (`adjustsetdamage` has none). The sequences below are exactly
/// 2 long, so a stray third draw would panic.
#[test]
fn a_landed_fixed_damage_move_draws_exactly_twice_and_ignores_every_stat() {
    let dex = Dex::new();
    // Two wildly different attackers against two wildly different defenders:
    // the damage is the literal regardless.
    let weak = mon(&dex, 1, 5, vec![SONIC_BOOM]); // Bulbasaur, level 5
    let strong = mon(&dex, 1, 100, vec![SONIC_BOOM]); // the same mon at 100
    let squirtle = mon(&dex, 7, 5, vec![TACKLE]);
    let tough = mon(&dex, 7, 100, vec![TACKLE]); // the same defender at 100

    for (attacker, defender) in [(&weak, &squirtle), (&strong, &tough), (&weak, &tough)] {
        let mut rng = SequenceRng::new([0, 0]);
        let outcome =
            resolve_fixed_damage_move(&dex, SONIC_BOOM, attacker, defender, &mut rng).unwrap();
        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: 20,
                is_critical: false,
            }
        );
        assert_eq!(rng.draws(), 2);
    }
}

/// Dragon Rage's literal is 40, and `EFFECT_LEVEL_DAMAGE` is the attacker's
/// level — both through the same two-draw shape.
#[test]
fn dragon_rage_and_level_damage_use_their_own_figures() {
    let dex = Dex::new();
    let defender = mon(&dex, 7, 50, vec![TACKLE]); // Squirtle: Water

    let attacker = mon(&dex, 1, 23, vec![DRAGON_RAGE, SEISMIC_TOSS]);
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, DRAGON_RAGE, &attacker, &defender, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 40,
            is_critical: false
        }
    );
    assert_eq!(rng.draws(), 2);

    // Seismic Toss from the same level-23 attacker: 23, not 40 and not 20.
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SEISMIC_TOSS, &attacker, &defender, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 23,
            is_critical: false
        }
    );
    assert_eq!(rng.draws(), 2);
}

/// `typecalc` still runs, so a **type immunity** nullifies the move
/// entirely — but the multiplier itself was thrown away by the `setword`, so
/// a resisted or super-effective matchup still takes the flat figure.
///
/// The immunity is discovered *after* the accuracy roll and is the third
/// operand of `Cmd_seteffectwithchance`'s chain, too late to suppress its
/// draw: the immune row still costs 2.
#[test]
fn type_immunity_nullifies_but_a_resisted_matchup_still_takes_the_flat_figure() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 30, vec![SONIC_BOOM, NIGHT_SHADE]);

    // Normal into Gastly (92, Ghost/Poison) is `x0`.
    let ghost = mon(&dex, 92, 30, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &ghost, &mut rng).unwrap(),
        HitOutcome::NoEffect
    );
    assert_eq!(
        rng.draws(),
        2,
        "the immunity is too late in the `&&` chain to suppress the effect-chance draw"
    );

    // Ghost into a pure Normal defender (Rattata, 19) is also `x0` -- the
    // mirror case, so the immunity is not an accident of one type pair.
    let normal = mon(&dex, 19, 30, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, NIGHT_SHADE, &attacker, &normal, &mut rng).unwrap(),
        HitOutcome::NoEffect
    );

    // Ghost into Gastly is `x2` -- and still exactly 30, the attacker's
    // level, because the multiplier never reaches the stored figure.
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, NIGHT_SHADE, &attacker, &ghost, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 30,
            is_critical: false
        }
    );
}

/// Sonic Boom's accuracy is 90, so it can miss — for **1** draw, the whole
/// move over at the `accuracycheck`.
#[test]
fn a_missed_fixed_damage_move_draws_once() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![SONIC_BOOM]);
    let defender = mon(&dex, 7, 5, vec![TACKLE]);
    // roll = draw % 100 + 1; 90 -> 91 > 90 -> miss.
    let mut rng = SequenceRng::new([90]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap(),
        HitOutcome::Miss
    );
    assert_eq!(rng.draws(), 1);
    // And 89 -> 90 <= 90 is the last landing value.
    let mut rng = SequenceRng::new([89, 0]);
    assert!(matches!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap(),
        HitOutcome::Hit { .. }
    ));
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![SONIC_BOOM]);
    let defender = mon(&dex, 7, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(rng.draws(), 0);
}
