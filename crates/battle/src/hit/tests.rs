use super::{ensure_resolvable, is_ordinary_hit_effect, resolve_hit, HitOutcome};
use crate::ability::{suppresses_critical_hits, GUTS, HUGE_POWER, MARVEL_SCALE, PURE_POWER};
use crate::accuracy::always_hits;
use crate::damage::{MoveCategory, STRUGGLE};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use crate::status1::Status1;
use assets::species::AbilityId;
use assets::{MoveId, SpeciesId};

const BULBASAUR: SpeciesId = SpeciesId(1);
const SQUIRTLE: SpeciesId = SpeciesId(7);
const RATTATA: SpeciesId = SpeciesId(19);
const ABRA: SpeciesId = SpeciesId(63);
const SHELLDER: SpeciesId = SpeciesId(90);
const GASTLY: SpeciesId = SpeciesId(92);
const MARILL: SpeciesId = SpeciesId(183);
const MEDITITE: SpeciesId = SpeciesId(356);
const ANORITH: SpeciesId = SpeciesId(390);
/// `SPECIES_MAKUHITA`: Guts in ability slot 1 (Thick Fat is slot 0).
const MAKUHITA: SpeciesId = SpeciesId(335);
/// `SPECIES_MILOTIC`: Marvel Scale in its primary (and only) ability slot.
const MILOTIC: SpeciesId = SpeciesId(329);

const DOUBLE_SLAP: MoveId = MoveId(3);
const HORN_DRILL: MoveId = MoveId(32);
const TACKLE: MoveId = MoveId(33);
const GROWL: MoveId = MoveId(45);
const SONIC_BOOM: MoveId = MoveId(49);
const WATER_GUN: MoveId = MoveId(55);
const COUNTER: MoveId = MoveId(68);
const SEISMIC_TOSS: MoveId = MoveId(69);
const ABSORB: MoveId = MoveId(71);
const QUICK_ATTACK: MoveId = MoveId(98);
const SWIFT: MoveId = MoveId(129);
const SLASH: MoveId = MoveId(163);
const CURSE: MoveId = MoveId(174);
const FALSE_SWIPE: MoveId = MoveId(206);
const PURSUIT: MoveId = MoveId(228);
const UNKNOWN_MOVE: MoveId = MoveId(60_000);

const THICK_FAT: AbilityId = AbilityId(47);

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

const ACCURACY_HIT_DRAW: u16 = 0;
const TACKLE_MISS_DRAW: u16 = 95;
const ORDINARY_CRIT_DRAW: u16 = 0;
const ORDINARY_NO_CRIT_DRAW: u16 = 1;
const HIGH_CRIT_ONLY_DRAW: u16 = 8;
const BEST_DAMAGE_DRAW: u16 = 0;
const DISCARDED_EFFECT_DRAW: u16 = 0;

const ORDINARY_NON_CRITICAL_DRAWS: [u16; 4] = [
    ACCURACY_HIT_DRAW,
    ORDINARY_NO_CRIT_DRAW,
    BEST_DAMAGE_DRAW,
    DISCARDED_EFFECT_DRAW,
];
const ORDINARY_CRITICAL_DRAWS: [u16; 4] = [
    ACCURACY_HIT_DRAW,
    ORDINARY_CRIT_DRAW,
    BEST_DAMAGE_DRAW,
    DISCARDED_EFFECT_DRAW,
];
const ALWAYS_HIT_NON_CRITICAL_DRAWS: [u16; 3] = [
    ORDINARY_NO_CRIT_DRAW,
    BEST_DAMAGE_DRAW,
    DISCARDED_EFFECT_DRAW,
];
const CRITICAL_SUPPRESSED_DRAWS: [u16; 3] =
    [ACCURACY_HIT_DRAW, BEST_DAMAGE_DRAW, DISCARDED_EFFECT_DRAW];
const STRUGGLE_NON_CRITICAL_DRAWS: [u16; 3] =
    [ACCURACY_HIT_DRAW, ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW];

const TACKLE_DAMAGE: u32 = 4;
const TACKLE_CRITICAL_DAMAGE: u32 = 8;
const MODEST_WATER_GUN_DAMAGE: u32 = 10;
const STRUGGLE_DAMAGE_TO_GASTLY: u32 = 6;
const THICK_FAT_MARILL_TACKLE_DAMAGE: u32 = 3;
const HUGE_POWER_MARILL_TACKLE_DAMAGE: u32 = 5;
const PURE_POWER_MEDITITE_TACKLE_DAMAGE: u32 = 6;
const HUGE_POWER_BEFORE_STAGE_DAMAGE: u32 = 8;
/// L5 Makuhita (12 raw Attack) versus L5 Milotic (14 raw Defense), Tackle,
/// best roll, no boost active on either side.
const HEALTHY_MAKUHITA_TACKLE_DAMAGE: u32 = 4;
/// The same matchup with the Makuhita attacker paralysed or poisoned, so
/// Guts raises its raw Attack from 12 to 18 before the stage multiply.
const GUTS_MAKUHITA_TACKLE_DAMAGE: u32 = 5;
/// The same base matchup with the Milotic defender paralysed or poisoned, so
/// Marvel Scale raises its raw Defense from 14 to 21 before the stage
/// multiply.
const MARVEL_SCALE_MILOTIC_TACKLE_DAMAGE: u32 = 3;

fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, species, level, MAX_IVS, 0, moves).unwrap()
}

#[test]
fn a_miss_draws_only_for_accuracy() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([TACKLE_MISS_DRAW]);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(resolution.outcome, HitOutcome::Miss);
    assert!(!resolution.poisons_defender);
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
    assert!(
        !resolution.poisons_defender,
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

/// `MOVE_POISON_STING`: `EFFECT_POISON_HIT`.
const POISON_STING: MoveId = MoveId(40);
/// `MOVE_SMOG`: `EFFECT_POISON_HIT`.
const SMOG: MoveId = MoveId(123);
/// `MOVE_SLUDGE`: `EFFECT_POISON_HIT`.
const SLUDGE: MoveId = MoveId(124);
/// `MOVE_SLUDGE_BOMB`: `EFFECT_POISON_HIT`.
const SLUDGE_BOMB: MoveId = MoveId(188);
/// `MOVE_POISON_TAIL`: `EFFECT_POISON_TAIL`, an unported trampoline.
const POISON_TAIL: MoveId = MoveId(342);
/// `SPECIES_EKANS`: mono Poison-type.
const EKANS: SpeciesId = SpeciesId(23);
/// A draw that clears [`POISON_STING`]'s 30% secondary chance.
const POISON_CHANCE_HIT_DRAW: u16 = 29;
/// A draw that misses it.
const POISON_CHANCE_MISS_DRAW: u16 = 30;

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
fn a_landed_poison_sting_reports_poisons_defender_only_on_a_successful_roll() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 10, vec![POISON_STING]);
    let defender = mon(&dex, SQUIRTLE, 10, vec![TACKLE]);
    assert_eq!(
        dex.move_data(POISON_STING).unwrap().secondary_effect_chance,
        30
    );

    let mut succeeds = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        POISON_CHANCE_HIT_DRAW,
    ]);
    let succeeded = resolve_hit(
        &dex,
        POISON_STING,
        &attacker,
        &defender,
        false,
        &mut succeeds,
    )
    .unwrap();
    assert!(matches!(succeeded.outcome, HitOutcome::Hit { .. }));
    assert!(succeeded.poisons_defender);
    assert_eq!(succeeds.draws(), 4);

    let mut fails = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        POISON_CHANCE_MISS_DRAW,
    ]);
    let failed = resolve_hit(&dex, POISON_STING, &attacker, &defender, false, &mut fails).unwrap();
    assert!(matches!(failed.outcome, HitOutcome::Hit { .. }));
    assert!(!failed.poisons_defender);
}

#[test]
fn a_poison_type_or_steel_type_defender_never_reports_poisons_defender() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 10, vec![POISON_STING]);
    let poison_type_defender = mon(&dex, EKANS, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        POISON_CHANCE_HIT_DRAW,
    ]);
    let resolution = resolve_hit(
        &dex,
        POISON_STING,
        &attacker,
        &poison_type_defender,
        false,
        &mut rng,
    )
    .unwrap();
    assert!(
        matches!(resolution.outcome, HitOutcome::Hit { .. }),
        "fixture sanity: Poison is only not-very-effective against Poison, not immune, \
         so the hit must land for this to test the status guard rather than a coincidental miss"
    );
    assert!(!resolution.poisons_defender);
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

#[test]
fn armor_abilities_skip_the_critical_draw_and_prevent_critical_hits() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SLASH]);

    for armor_species in [ANORITH, SHELLDER] {
        let defender = mon(&dex, armor_species, 5, vec![TACKLE]);
        assert!(suppresses_critical_hits(defender.ability()));
        let mut rng = SequenceRng::new(CRITICAL_SUPPRESSED_DRAWS);

        let resolution = resolve_hit(&dex, SLASH, &attacker, &defender, false, &mut rng).unwrap();

        assert!(matches!(
            resolution.outcome,
            HitOutcome::Hit {
                is_critical: false,
                ..
            }
        ));
        assert_eq!(rng.draws(), CRITICAL_SUPPRESSED_DRAWS.len());
    }
}

#[test]
fn huge_power_in_ability_slot_two_doubles_a_physical_hit() {
    let dex = Dex::new();
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let thick_fat = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(0);
    let huge_power = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(thick_fat.ability(), THICK_FAT);
    assert_eq!(huge_power.ability(), HUGE_POWER);

    let mut thick_fat_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    assert_eq!(
        resolve_hit(
            &dex,
            TACKLE,
            &thick_fat,
            &defender,
            false,
            &mut thick_fat_rng,
        )
        .unwrap()
        .outcome,
        HitOutcome::Hit {
            damage: THICK_FAT_MARILL_TACKLE_DAMAGE,
            is_critical: false,
        }
    );

    let mut huge_power_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    assert_eq!(
        resolve_hit(
            &dex,
            TACKLE,
            &huge_power,
            &defender,
            false,
            &mut huge_power_rng,
        )
        .unwrap()
        .outcome,
        HitOutcome::Hit {
            damage: HUGE_POWER_MARILL_TACKLE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn pure_power_doubles_a_physical_hit() {
    let dex = Dex::new();
    let attacker = mon(&dex, MEDITITE, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    assert_eq!(attacker.ability(), PURE_POWER);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: PURE_POWER_MEDITITE_TACKLE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn huge_power_never_touches_a_special_move() {
    let dex = Dex::new();
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let thick_fat = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![WATER_GUN])
        .unwrap()
        .with_ability_slot(0);
    let huge_power = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![WATER_GUN])
        .unwrap()
        .with_ability_slot(1);

    let mut thick_fat_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let thick_fat_outcome = resolve_hit(
        &dex,
        WATER_GUN,
        &thick_fat,
        &defender,
        false,
        &mut thick_fat_rng,
    )
    .unwrap();
    let mut huge_power_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let huge_power_outcome = resolve_hit(
        &dex,
        WATER_GUN,
        &huge_power,
        &defender,
        false,
        &mut huge_power_rng,
    )
    .unwrap();

    assert_eq!(thick_fat_outcome.outcome, huge_power_outcome.outcome);
}

#[test]
fn huge_power_doubles_raw_attack_before_stat_stage_scaling() {
    let dex = Dex::new();
    let mut attacker = BattlePokemon::new(&dex, MARILL, 15, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(1);
    attacker.stages_mut().attack = StatStage::new(-2).unwrap();
    let defender = mon(&dex, ABRA, 15, vec![TACKLE]);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: HUGE_POWER_BEFORE_STAGE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn a_statused_guts_attacker_raises_physical_damage() {
    let dex = Dex::new();
    let healthy = BattlePokemon::new(&dex, MAKUHITA, 5, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(healthy.ability(), GUTS);
    let mut statused = healthy.clone();
    statused.set_status1(Status1::Paralysed);
    let defender = mon(&dex, MILOTIC, 5, vec![TACKLE]);

    let mut healthy_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let healthy_outcome =
        resolve_hit(&dex, TACKLE, &healthy, &defender, false, &mut healthy_rng).unwrap();
    assert_eq!(
        healthy_outcome.outcome,
        HitOutcome::Hit {
            damage: HEALTHY_MAKUHITA_TACKLE_DAMAGE,
            is_critical: false,
        }
    );

    let mut statused_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let statused_outcome =
        resolve_hit(&dex, TACKLE, &statused, &defender, false, &mut statused_rng).unwrap();
    assert_eq!(
        statused_outcome.outcome,
        HitOutcome::Hit {
            damage: GUTS_MAKUHITA_TACKLE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn a_statused_marvel_scale_defender_lowers_physical_damage() {
    let dex = Dex::new();
    let attacker = mon(&dex, MAKUHITA, 5, vec![TACKLE]);
    let healthy_defender = mon(&dex, MILOTIC, 5, vec![TACKLE]);
    assert_eq!(healthy_defender.ability(), MARVEL_SCALE);
    let mut statused_defender = healthy_defender.clone();
    statused_defender.set_status1(Status1::Poisoned);

    let mut healthy_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let healthy_outcome = resolve_hit(
        &dex,
        TACKLE,
        &attacker,
        &healthy_defender,
        false,
        &mut healthy_rng,
    )
    .unwrap();
    assert_eq!(
        healthy_outcome.outcome,
        HitOutcome::Hit {
            damage: HEALTHY_MAKUHITA_TACKLE_DAMAGE,
            is_critical: false,
        }
    );

    let mut statused_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let statused_outcome = resolve_hit(
        &dex,
        TACKLE,
        &attacker,
        &statused_defender,
        false,
        &mut statused_rng,
    )
    .unwrap();
    assert_eq!(
        statused_outcome.outcome,
        HitOutcome::Hit {
            damage: MARVEL_SCALE_MILOTIC_TACKLE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn guts_never_touches_special_attack() {
    let dex = Dex::new();
    let mut attacker = BattlePokemon::new(&dex, MAKUHITA, 5, MAX_IVS, 0, vec![WATER_GUN])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(attacker.ability(), GUTS);
    let healthy_special_attack = attacker.attacking_stat(MoveCategory::Special);

    attacker.set_status1(Status1::Paralysed);

    assert_eq!(
        attacker.attacking_stat(MoveCategory::Special),
        healthy_special_attack,
        "Guts must not touch Special Attack even once its holder is statused"
    );
}

#[test]
fn marvel_scale_never_touches_special_defense() {
    let dex = Dex::new();
    let mut defender = mon(&dex, MILOTIC, 5, vec![WATER_GUN]);
    assert_eq!(defender.ability(), MARVEL_SCALE);
    let healthy_special_defense = defender.defending_stat(MoveCategory::Special);

    defender.set_status1(Status1::Poisoned);

    assert_eq!(
        defender.defending_stat(MoveCategory::Special),
        healthy_special_defense,
        "Marvel Scale must not touch Special Defense even once its holder is statused"
    );
}
