//! Ordinary-hit accuracy, critical-hit, damage and RNG-draw tests for
//! [`resolve_hit`]: the draw order (accuracy, critical, damage, discarded
//! effect chance), the pinned best-roll damage, and critical suppression.
//! Admission and move-shape gating live in `admission_tests`; ability and
//! secondary-effect regressions live in their own sibling modules.

use super::test_fixtures::{
    mon, ACCURACY_HIT_DRAW, ALWAYS_HIT_NON_CRITICAL_DRAWS, BEST_DAMAGE_DRAW, BULBASAUR,
    CRITICAL_SUPPRESSED_DRAWS, DISCARDED_EFFECT_DRAW, GASTLY, MAX_IVS, ORDINARY_NON_CRITICAL_DRAWS,
    ORDINARY_NO_CRIT_DRAW, RATTATA, SLASH, SQUIRTLE, STRUGGLE_NON_CRITICAL_DRAWS, SWIFT, TACKLE,
    TACKLE_MISS_DRAW, WATER_GUN,
};
use super::{resolve_hit, HitOutcome};
use crate::accuracy::always_hits;
use crate::damage::STRUGGLE;
use crate::dex::Dex;
use crate::pokemon::BattlePokemon;
use crate::script_rng::SequenceRng;
use crate::secondary::SecondaryApplication;

const ORDINARY_CRIT_DRAW: u16 = 0;

const HIGH_CRIT_ONLY_DRAW: u16 = 8;

const ORDINARY_CRITICAL_DRAWS: [u16; 4] = [
    ACCURACY_HIT_DRAW,
    ORDINARY_CRIT_DRAW,
    BEST_DAMAGE_DRAW,
    DISCARDED_EFFECT_DRAW,
];

const TACKLE_DAMAGE: u32 = 4;

const TACKLE_CRITICAL_DAMAGE: u32 = 8;

const MODEST_WATER_GUN_DAMAGE: u32 = 10;

const STRUGGLE_DAMAGE_TO_GASTLY: u32 = 6;

#[test]
fn a_miss_draws_only_for_accuracy() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([TACKLE_MISS_DRAW]);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(resolution.outcome, HitOutcome::Miss);
    assert_eq!(resolution.secondary_effect, SecondaryApplication::None);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn an_ordinary_hit_draws_accuracy_critical_damage_and_effect_chance() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert!(matches!(resolution.outcome, HitOutcome::Hit { .. }));
    assert_eq!(
        resolution.secondary_effect,
        SecondaryApplication::None,
        "Tackle has no secondary effect"
    );
    assert_eq!(rng.draws(), ORDINARY_NON_CRITICAL_DRAWS.len());
}

#[test]
fn the_discarded_effect_chance_value_never_changes_an_ordinary_hit() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    let mut outcomes = Vec::new();
    for effect_chance_draw in [0u16, 99, u16::MAX] {
        let draws = [
            ACCURACY_HIT_DRAW,
            ORDINARY_NO_CRIT_DRAW,
            BEST_DAMAGE_DRAW,
            effect_chance_draw,
        ];
        let mut rng = SequenceRng::new(draws);
        outcomes.push(resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap());
        assert_eq!(rng.draws(), draws.len());
    }

    assert!(outcomes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn best_roll_non_critical_damage_matches_the_independent_pin() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: TACKLE_DAMAGE,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), ORDINARY_NON_CRITICAL_DRAWS.len());
}

#[test]
fn a_confirmed_critical_hit_doubles_the_pinned_damage_and_is_reported() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(ORDINARY_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: TACKLE_CRITICAL_DAMAGE,
            is_critical: true,
        }
    );
    assert_eq!(TACKLE_CRITICAL_DAMAGE, 2 * TACKLE_DAMAGE);
    assert_eq!(rng.draws(), ORDINARY_CRITICAL_DRAWS.len());
}

#[test]
fn a_special_move_uses_nature_adjusted_special_stats() {
    let dex = Dex::new();
    let modest_personality = 15;
    let calm_personality = 20;
    let attacker = BattlePokemon::new(
        &dex,
        SQUIRTLE,
        10,
        MAX_IVS,
        modest_personality,
        vec![WATER_GUN],
    )
    .unwrap();
    let defender =
        BattlePokemon::new(&dex, RATTATA, 10, MAX_IVS, calm_personality, vec![TACKLE]).unwrap();
    assert_eq!(attacker.stats().sp_attack, 19);
    assert_eq!(defender.stats().sp_defense, 16);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, WATER_GUN, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: MODEST_WATER_GUN_DAMAGE,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), ORDINARY_NON_CRITICAL_DRAWS.len());
}

#[test]
fn struggle_bypasses_stab_type_effectiveness_and_the_effect_chance_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![STRUGGLE]);
    let ghost_defender = mon(&dex, GASTLY, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(STRUGGLE_NON_CRITICAL_DRAWS);

    let resolution =
        resolve_hit(&dex, STRUGGLE, &attacker, &ghost_defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: STRUGGLE_DAMAGE_TO_GASTLY,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), STRUGGLE_NON_CRITICAL_DRAWS.len());

    let ordinary_attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let resolution = resolve_hit(
        &dex,
        TACKLE,
        &ordinary_attacker,
        &ghost_defender,
        false,
        &mut rng,
    )
    .unwrap();
    assert_eq!(resolution.outcome, HitOutcome::NoEffect);
}

#[test]
fn type_immunity_still_draws_critical_damage_and_effect_chance() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 20, vec![TACKLE]);
    let ghost_defender = mon(&dex, GASTLY, 20, vec![TACKLE]);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution =
        resolve_hit(&dex, TACKLE, &attacker, &ghost_defender, false, &mut rng).unwrap();

    assert_eq!(resolution.outcome, HitOutcome::NoEffect);
    assert_eq!(rng.draws(), ORDINARY_NON_CRITICAL_DRAWS.len());
}

#[test]
fn an_accuracy_bypassing_hit_starts_with_the_critical_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SWIFT]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    assert!(always_hits(dex.move_data(SWIFT).unwrap().effect));
    let mut rng = SequenceRng::new(ALWAYS_HIT_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, SWIFT, &attacker, &defender, false, &mut rng).unwrap();

    assert!(matches!(resolution.outcome, HitOutcome::Hit { .. }));
    assert_eq!(rng.draws(), ALWAYS_HIT_NON_CRITICAL_DRAWS.len());

    let draws = [TACKLE_MISS_DRAW, BEST_DAMAGE_DRAW, DISCARDED_EFFECT_DRAW];
    let mut rng = SequenceRng::new(draws);
    let resolution = resolve_hit(&dex, SWIFT, &attacker, &defender, false, &mut rng).unwrap();
    assert!(matches!(resolution.outcome, HitOutcome::Hit { .. }));
    assert_eq!(rng.draws(), draws.len());
}

#[test]
fn a_high_crit_move_crits_on_a_draw_an_ordinary_move_does_not() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SLASH, TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let separating_draws = [
        ACCURACY_HIT_DRAW,
        HIGH_CRIT_ONLY_DRAW,
        BEST_DAMAGE_DRAW,
        DISCARDED_EFFECT_DRAW,
    ];

    let mut slash_rng = SequenceRng::new(separating_draws);
    let slash = resolve_hit(&dex, SLASH, &attacker, &defender, false, &mut slash_rng).unwrap();
    assert!(matches!(
        slash.outcome,
        HitOutcome::Hit {
            is_critical: true,
            ..
        }
    ));

    let mut tackle_rng = SequenceRng::new(separating_draws);
    let tackle = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut tackle_rng).unwrap();
    assert!(matches!(
        tackle.outcome,
        HitOutcome::Hit {
            is_critical: false,
            ..
        }
    ));
}

#[test]
fn caller_critical_suppression_skips_the_critical_draw_and_never_crits() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SLASH]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(CRITICAL_SUPPRESSED_DRAWS);

    let resolution = resolve_hit(&dex, SLASH, &attacker, &defender, true, &mut rng).unwrap();

    assert!(matches!(
        resolution.outcome,
        HitOutcome::Hit {
            is_critical: false,
            ..
        }
    ));
    assert_eq!(rng.draws(), CRITICAL_SUPPRESSED_DRAWS.len());
}

#[test]
fn critical_suppression_removes_only_the_critical_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE, SWIFT]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    let mut ordinary_rng = SequenceRng::new(CRITICAL_SUPPRESSED_DRAWS);
    let ordinary =
        resolve_hit(&dex, TACKLE, &attacker, &defender, true, &mut ordinary_rng).unwrap();
    assert!(matches!(ordinary.outcome, HitOutcome::Hit { .. }));
    assert_eq!(ordinary_rng.draws(), CRITICAL_SUPPRESSED_DRAWS.len());

    let always_hit_draws = [BEST_DAMAGE_DRAW, DISCARDED_EFFECT_DRAW];
    let mut always_hit_rng = SequenceRng::new(always_hit_draws);
    let always_hit =
        resolve_hit(&dex, SWIFT, &attacker, &defender, true, &mut always_hit_rng).unwrap();
    assert!(matches!(always_hit.outcome, HitOutcome::Hit { .. }));
    assert_eq!(always_hit_rng.draws(), always_hit_draws.len());

    let mut miss_rng = SequenceRng::new([TACKLE_MISS_DRAW]);
    let miss = resolve_hit(&dex, TACKLE, &attacker, &defender, true, &mut miss_rng).unwrap();
    assert_eq!(miss.outcome, HitOutcome::Miss);
    assert_eq!(miss_rng.draws(), 1);
}
