use super::{
    ensure_resolvable, is_multi_hit_effect, resolve_multi_hit, roll_hit_count,
    spend_multi_hit_effect_chance_draw, EFFECT_MULTI_HIT, MAX_HITS, MIN_HITS,
};
use crate::ability::suppresses_critical_hits;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{damage_core, HitOutcome};
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use assets::{MoveId, SpeciesId};

const DOUBLE_SLAP: MoveId = MoveId(3);
const FURY_ATTACK: MoveId = MoveId(31);
const TACKLE: MoveId = MoveId(33);
const BULBASAUR: SpeciesId = SpeciesId(1);
const SQUIRTLE: SpeciesId = SpeciesId(7);
const ANORITH: SpeciesId = SpeciesId(390);
const TEST_LEVEL: u8 = 5;
const ACCURACY_HIT_DRAW: u16 = 0;
const DOUBLE_SLAP_MISS_DRAW: u16 = 85;
const TWO_HIT_COUNT_DRAW: u16 = 0;
const NON_CRITICAL_DRAW: u16 = 1;
const FULL_DAMAGE_DRAW: u16 = 0;
const TEST_PERSONALITY: u32 = 0;
const ADMISSION_DRAWS_WITH_ONE_COUNT_ROLL: usize = 2;
const ORDINARY_DRAWS_PER_HIT: usize = 2;
const CRITICAL_SUPPRESSED_DRAWS_PER_HIT: usize = 1;

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

fn mon(dex: &Dex, species: SpeciesId, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, species, TEST_LEVEL, MAX_IVS, TEST_PERSONALITY, moves).unwrap()
}

#[test]
fn only_effect_multi_hit_is_accepted() {
    let dex = Dex::new();
    for move_id in [DOUBLE_SLAP, FURY_ATTACK] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, EFFECT_MULTI_HIT);
        assert!(is_multi_hit_effect(dex.move_data(move_id).unwrap().effect));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

#[test]
fn hit_count_offsets_zero_and_one_settle_in_one_draw() {
    for (first_hit_offset, expected_hits) in [(0u16, 2u8), (1, 3)] {
        let mut rng = SequenceRng::new([first_hit_offset]);
        assert_eq!(roll_hit_count(&mut rng), expected_hits);
        assert_eq!(rng.draws(), 1, "offset {first_hit_offset} must settle");
    }
}

#[test]
fn hit_count_offsets_two_and_three_redraw_before_setting_the_count() {
    for first_hit_offset in [2u16, 3] {
        for (second_hit_offset, expected_hits) in [(0u16, 2u8), (1, 3), (2, 4), (3, 5)] {
            let mut rng = SequenceRng::new([first_hit_offset, second_hit_offset]);
            assert_eq!(
                roll_hit_count(&mut rng),
                expected_hits,
                "offsets {first_hit_offset} then {second_hit_offset}"
            );
            assert_eq!(rng.draws(), 2);
        }
    }
}

#[test]
fn hit_count_uses_only_the_low_two_bits_of_each_draw() {
    let draw_with_zero_low_bits = !0b11;
    let mut rng = SequenceRng::new([draw_with_zero_low_bits]);
    assert_eq!(roll_hit_count(&mut rng), 2);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn all_low_bit_pairs_produce_the_expected_hit_count_distribution() {
    let mut counts = [0u32; MAX_HITS as usize + 1];
    for first_hit_offset in 0u16..4 {
        for second_hit_offset in 0u16..4 {
            let mut rng = SequenceRng::new([first_hit_offset, second_hit_offset]);
            let hits = roll_hit_count(&mut rng);
            assert!((MIN_HITS..=MAX_HITS).contains(&hits));
            counts[hits as usize] += 1;
        }
    }
    let expected_counts = [(2usize, 6u32), (3, 6), (4, 2), (5, 2)];
    for (hits, expected_count) in expected_counts {
        assert_eq!(counts[hits], expected_count, "{hits} hits");
    }
}

#[test]
fn admission_draws_accuracy_once_then_draws_the_hit_count() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, vec![DOUBLE_SLAP]);
    let defender = mon(&dex, SQUIRTLE, vec![TACKLE]);

    let mut missed_move = SequenceRng::new([DOUBLE_SLAP_MISS_DRAW]);
    assert_eq!(
        resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &defender, &mut missed_move).unwrap(),
        None
    );
    assert_eq!(missed_move.draws(), 1);

    let mut one_draw_hit_count = SequenceRng::new([ACCURACY_HIT_DRAW, TWO_HIT_COUNT_DRAW]);
    assert_eq!(
        resolve_multi_hit(
            &dex,
            DOUBLE_SLAP,
            &attacker,
            &defender,
            &mut one_draw_hit_count
        )
        .unwrap(),
        Some(2)
    );
    assert_eq!(one_draw_hit_count.draws(), 2);

    let mut two_draw_hit_count = SequenceRng::new([ACCURACY_HIT_DRAW, 3, 3]);
    assert_eq!(
        resolve_multi_hit(
            &dex,
            DOUBLE_SLAP,
            &attacker,
            &defender,
            &mut two_draw_hit_count
        )
        .unwrap(),
        Some(5)
    );
    assert_eq!(two_draw_hit_count.draws(), 3);
}

#[test]
fn epilogue_discards_exactly_one_effect_chance_draw_per_move() {
    let dex = Dex::new();
    assert_eq!(
        dex.move_data(DOUBLE_SLAP).unwrap().secondary_effect_chance,
        0
    );
    for had_effect in [true, false] {
        for value in [0u16, 99, u16::MAX] {
            let mut rng = SequenceRng::new([value]);
            assert_eq!(
                spend_multi_hit_effect_chance_draw(&dex, DOUBLE_SLAP, had_effect, &mut rng),
                Ok(())
            );
            assert_eq!(rng.draws(), 1);
        }
    }
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, vec![DOUBLE_SLAP]);
    let defender = mon(&dex, SQUIRTLE, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_multi_hit(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn a_battle_armor_defender_drops_every_processed_hit_by_one_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, vec![DOUBLE_SLAP]);

    let ordinary_defender = mon(&dex, SQUIRTLE, vec![TACKLE]);
    let admission_draws = [ACCURACY_HIT_DRAW, TWO_HIT_COUNT_DRAW];
    let ordinary_per_hit_draws = [
        NON_CRITICAL_DRAW,
        FULL_DAMAGE_DRAW,
        NON_CRITICAL_DRAW,
        FULL_DAMAGE_DRAW,
    ];
    let mut ordinary_rng =
        SequenceRng::new(admission_draws.into_iter().chain(ordinary_per_hit_draws));
    let hits = resolve_multi_hit(
        &dex,
        DOUBLE_SLAP,
        &attacker,
        &ordinary_defender,
        &mut ordinary_rng,
    )
    .unwrap()
    .expect("the scripted accuracy draw must land");
    assert_eq!(hits, 2);
    for _ in 0..hits {
        damage_core(
            &dex,
            DOUBLE_SLAP,
            &attacker,
            &ordinary_defender,
            false,
            &mut ordinary_rng,
        )
        .unwrap();
    }
    assert_eq!(
        ordinary_rng.draws(),
        ADMISSION_DRAWS_WITH_ONE_COUNT_ROLL + ORDINARY_DRAWS_PER_HIT * usize::from(hits),
        "ordinary defender: critical and damage draw per hit"
    );

    let battle_armor_defender = mon(&dex, ANORITH, vec![TACKLE]);
    assert!(suppresses_critical_hits(battle_armor_defender.ability()));
    let damage_draws_without_critical_rolls = [FULL_DAMAGE_DRAW, FULL_DAMAGE_DRAW];
    let mut battle_armor_rng = SequenceRng::new(
        admission_draws
            .into_iter()
            .chain(damage_draws_without_critical_rolls),
    );
    let hits = resolve_multi_hit(
        &dex,
        DOUBLE_SLAP,
        &attacker,
        &battle_armor_defender,
        &mut battle_armor_rng,
    )
    .unwrap()
    .expect("the scripted accuracy draw must land");
    assert_eq!(hits, 2);
    for _ in 0..hits {
        let outcome = damage_core(
            &dex,
            DOUBLE_SLAP,
            &attacker,
            &battle_armor_defender,
            false,
            &mut battle_armor_rng,
        )
        .unwrap();
        assert!(
            matches!(
                outcome,
                HitOutcome::Hit {
                    is_critical: false,
                    ..
                }
            ),
            "{outcome:?}"
        );
    }
    assert_eq!(
        battle_armor_rng.draws(),
        ADMISSION_DRAWS_WITH_ONE_COUNT_ROLL + CRITICAL_SUPPRESSED_DRAWS_PER_HIT * usize::from(hits),
        "Battle Armor defender: damage draw only per hit"
    );
}
