use super::{
    ensure_resolvable, fixed_damage_for_effect, is_fixed_damage_effect, resolve_fixed_damage_move,
    FixedDamage, DRAGON_RAGE_DAMAGE, EFFECT_DRAGON_RAGE, EFFECT_LEVEL_DAMAGE, EFFECT_PSYWAVE,
    EFFECT_SONICBOOM, FIXED_DAMAGE_EFFECTS, SONIC_BOOM_DAMAGE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::HitOutcome;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use assets::{MoveId, SpeciesId};

const BULBASAUR: SpeciesId = SpeciesId(1);
const SQUIRTLE: SpeciesId = SpeciesId(7);
const RATTATA: SpeciesId = SpeciesId(19);
const GASTLY: SpeciesId = SpeciesId(92);
/// Dark: immune to Psychic.
const POOCHYENA: SpeciesId = SpeciesId::POOCHYENA;
/// `SPECIES_SHEDINJA`: Wonder Guard in its primary (and only) ability slot.
const SHEDINJA: SpeciesId = SpeciesId(303);

const TACKLE: MoveId = MoveId::TACKLE;
const SONIC_BOOM: MoveId = MoveId::SONIC_BOOM;
const SEISMIC_TOSS: MoveId = MoveId::SEISMIC_TOSS;
const DRAGON_RAGE: MoveId = MoveId::DRAGON_RAGE;
const NIGHT_SHADE: MoveId = MoveId::NIGHT_SHADE;
const PSYWAVE: MoveId = MoveId::PSYWAVE;

const HARDY_PERSONALITY: u32 = 0;
const ACCURACY_HIT_DRAW: u16 = 0;
const SONIC_BOOM_MISS_DRAW: u16 = 90;
const SONIC_BOOM_LAST_HIT_DRAW: u16 = 89;
const DISCARDED_EFFECT_CHANCE_DRAW: u16 = u16::MAX;
const LANDED_FIXED_DAMAGE_DRAWS: [u16; 2] = [ACCURACY_HIT_DRAW, DISCARDED_EFFECT_CHANCE_DRAW];

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, species, level, MAX_IVS, HARDY_PERSONALITY, moves).unwrap()
}

#[test]
fn five_moves_map_to_the_four_supported_effects() {
    let dex = Dex::new();
    for (move_id, effect) in [
        (SONIC_BOOM, EFFECT_SONICBOOM),
        (DRAGON_RAGE, EFFECT_DRAGON_RAGE),
        (SEISMIC_TOSS, EFFECT_LEVEL_DAMAGE),
        (NIGHT_SHADE, EFFECT_LEVEL_DAMAGE),
        (PSYWAVE, EFFECT_PSYWAVE),
    ] {
        let move_data = dex.move_data(move_id).unwrap();
        assert_eq!(move_data.effect, effect);
        assert_eq!(move_data.power, 1);
        assert!(is_fixed_damage_effect(effect));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }

    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

#[test]
fn effects_select_literal_or_attacker_level_damage() {
    assert_eq!(
        FIXED_DAMAGE_EFFECTS,
        [
            (EFFECT_DRAGON_RAGE, FixedDamage::Literal(DRAGON_RAGE_DAMAGE)),
            (EFFECT_LEVEL_DAMAGE, FixedDamage::AttackerLevel),
            (EFFECT_PSYWAVE, FixedDamage::LevelScaledRandom),
            (EFFECT_SONICBOOM, FixedDamage::Literal(SONIC_BOOM_DAMAGE)),
        ]
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_SONICBOOM),
        Some(FixedDamage::Literal(SONIC_BOOM_DAMAGE))
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_DRAGON_RAGE),
        Some(FixedDamage::Literal(DRAGON_RAGE_DAMAGE))
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_LEVEL_DAMAGE),
        Some(FixedDamage::AttackerLevel)
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_PSYWAVE),
        Some(FixedDamage::LevelScaledRandom)
    );
    assert_eq!(fixed_damage_for_effect(crate::drain::EFFECT_ABSORB), None);

    let literal_damage = 17;
    let attacker_level = 77;
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        FixedDamage::Literal(literal_damage).amount(attacker_level, &mut rng),
        literal_damage
    );
    assert_eq!(
        FixedDamage::AttackerLevel.amount(attacker_level, &mut rng),
        u32::from(attacker_level)
    );
    assert_eq!(FixedDamage::AttackerLevel.amount(1, &mut rng), 1);
    assert_eq!(rng.draws(), 0, "only the random source draws");
}

#[test]
fn dragon_rage_and_sonic_boom_deal_the_game_defined_forty_and_twenty() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 23, vec![DRAGON_RAGE, SONIC_BOOM]);
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);

    for (move_id, expected_damage) in [(DRAGON_RAGE, 40), (SONIC_BOOM, 20)] {
        let mut rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
        let outcome =
            resolve_fixed_damage_move(&dex, move_id, &attacker, &defender, &mut rng).unwrap();

        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: expected_damage,
                is_critical: false,
            }
        );
    }
}

#[test]
fn literal_damage_ignores_attacker_and_defender_stats() {
    let dex = Dex::new();
    let weak_attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let strong_attacker = mon(&dex, BULBASAUR, 100, vec![SONIC_BOOM]);
    let weak_defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let strong_defender = mon(&dex, SQUIRTLE, 100, vec![TACKLE]);

    for (attacker, defender) in [
        (&weak_attacker, &weak_defender),
        (&strong_attacker, &strong_defender),
        (&weak_attacker, &strong_defender),
    ] {
        let mut rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
        let outcome =
            resolve_fixed_damage_move(&dex, SONIC_BOOM, attacker, defender, &mut rng).unwrap();

        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: SONIC_BOOM_DAMAGE,
                is_critical: false,
            }
        );
        assert_eq!(rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());
    }
}

#[test]
fn dragon_rage_uses_its_literal_and_level_damage_uses_the_attacker_level() {
    let dex = Dex::new();
    let attacker_level = 23;
    let attacker = mon(
        &dex,
        BULBASAUR,
        attacker_level,
        vec![DRAGON_RAGE, SEISMIC_TOSS],
    );
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);

    let mut dragon_rage_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let dragon_rage = resolve_fixed_damage_move(
        &dex,
        DRAGON_RAGE,
        &attacker,
        &defender,
        &mut dragon_rage_rng,
    )
    .unwrap();
    assert_eq!(
        dragon_rage,
        HitOutcome::Hit {
            damage: DRAGON_RAGE_DAMAGE,
            is_critical: false,
        }
    );
    assert_eq!(dragon_rage_rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());

    let mut seismic_toss_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let seismic_toss = resolve_fixed_damage_move(
        &dex,
        SEISMIC_TOSS,
        &attacker,
        &defender,
        &mut seismic_toss_rng,
    )
    .unwrap();
    assert_eq!(
        seismic_toss,
        HitOutcome::Hit {
            damage: u32::from(attacker_level),
            is_critical: false,
        }
    );
    assert_eq!(seismic_toss_rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());
}

#[test]
fn type_effectiveness_only_decides_immunity() {
    let dex = Dex::new();
    let attacker_level = 30;
    let attacker = mon(
        &dex,
        BULBASAUR,
        attacker_level,
        vec![SONIC_BOOM, NIGHT_SHADE],
    );
    let ghost_defender = mon(&dex, GASTLY, 30, vec![TACKLE]);
    let normal_defender = mon(&dex, RATTATA, 30, vec![TACKLE]);

    let mut sonic_boom_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let sonic_boom = resolve_fixed_damage_move(
        &dex,
        SONIC_BOOM,
        &attacker,
        &ghost_defender,
        &mut sonic_boom_rng,
    )
    .unwrap();
    assert_eq!(sonic_boom, HitOutcome::NoEffect);
    assert_eq!(sonic_boom_rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());

    let mut immune_night_shade_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let immune_night_shade = resolve_fixed_damage_move(
        &dex,
        NIGHT_SHADE,
        &attacker,
        &normal_defender,
        &mut immune_night_shade_rng,
    )
    .unwrap();
    assert_eq!(immune_night_shade, HitOutcome::NoEffect);
    assert_eq!(
        immune_night_shade_rng.draws(),
        LANDED_FIXED_DAMAGE_DRAWS.len()
    );

    let mut super_effective_night_shade_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let super_effective_night_shade = resolve_fixed_damage_move(
        &dex,
        NIGHT_SHADE,
        &attacker,
        &ghost_defender,
        &mut super_effective_night_shade_rng,
    )
    .unwrap();
    assert_eq!(
        super_effective_night_shade,
        HitOutcome::Hit {
            damage: u32::from(attacker_level),
            is_critical: false,
        }
    );
    assert_eq!(
        super_effective_night_shade_rng.draws(),
        LANDED_FIXED_DAMAGE_DRAWS.len()
    );
}

/// `Cmd_typecalc`'s Wonder Guard branch (`battle_script_commands.c:1409-1418`)
/// also gates fixed damage: Dragon Rage's Dragon typing has no chart row
/// against either of Shedinja's types, so only Wonder Guard can block it.
#[test]
fn wonder_guard_blocks_dragon_rages_neutral_matchup() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 23, vec![DRAGON_RAGE]);
    let shedinja_defender = mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);

    let outcome =
        resolve_fixed_damage_move(&dex, DRAGON_RAGE, &attacker, &shedinja_defender, &mut rng)
            .unwrap();

    assert_eq!(outcome, HitOutcome::WonderGuardBlocked);
    assert_eq!(rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());
}

/// Wonder Guard's own message outranks the ordinary typing-immunity message
/// (`Cmd_resultmessage`, `battle_script_commands.c:2048-2059`), so Sonic
/// Boom's independent Normal-versus-Ghost immunity must still report the
/// Wonder Guard outcome, not [`HitOutcome::NoEffect`].
#[test]
fn wonder_guard_outranks_sonic_booms_independent_immunity() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 23, vec![SONIC_BOOM]);
    let shedinja_defender = mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);

    let outcome =
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &shedinja_defender, &mut rng)
            .unwrap();

    assert_eq!(outcome, HitOutcome::WonderGuardBlocked);
}

/// Night Shade's Ghost typing is super effective against Shedinja's own
/// Ghost type and neutral against its Bug type, so Wonder Guard must let it
/// through.
#[test]
fn wonder_guard_permits_night_shades_super_effective_matchup() {
    let dex = Dex::new();
    let attacker_level = 30;
    let attacker = mon(&dex, BULBASAUR, attacker_level, vec![NIGHT_SHADE]);
    let shedinja_defender = mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);

    let outcome =
        resolve_fixed_damage_move(&dex, NIGHT_SHADE, &attacker, &shedinja_defender, &mut rng)
            .unwrap();

    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: u32::from(attacker_level),
            is_critical: false,
        }
    );
}

#[test]
fn a_miss_consumes_only_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([SONIC_BOOM_MISS_DRAW]);

    let outcome =
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap();

    assert_eq!(outcome, HitOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn sonic_booms_accuracy_boundary_is_inclusive() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let draws = [SONIC_BOOM_LAST_HIT_DRAW, DISCARDED_EFFECT_CHANCE_DRAW];
    let mut rng = SequenceRng::new(draws);

    let outcome =
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap();

    assert!(matches!(outcome, HitOutcome::Hit { .. }));
    assert_eq!(rng.draws(), draws.len());
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);

    assert_eq!(
        resolve_fixed_damage_move(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(rng.draws(), 0);
}

/// Every fixed-damage move carries `power = 1` upstream, so a failed accuracy
/// roll passes `CheckWonderGuardAndLevitate`'s zero-power gate
/// (`battle_script_commands.c:1432-1433`) and its type scan flags
/// `MOVE_RESULT_DOESNT_AFFECT_FOE` against Gastly's Ghost half (`:1454-1469`).
#[test]
fn a_missed_fixed_damage_move_against_an_immune_target_reports_the_immunity() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let ghost_defender = mon(&dex, GASTLY, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([SONIC_BOOM_MISS_DRAW]);

    let outcome =
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &ghost_defender, &mut rng).unwrap();

    assert_eq!(outcome, HitOutcome::NoEffect);
    assert_eq!(
        rng.draws(),
        1,
        "a failed roll still skips the trailing effect-chance draw"
    );
}

/// Wonder Guard overwrites the miss result after the same type scan
/// (`battle_script_commands.c:1490-1497`), and `Cmd_resultmessage` prefers it
/// over the typing-immunity message (`:2055-2059`), so Sonic Boom's
/// independent Normal-versus-Ghost immunity does not win here either.
#[test]
fn a_missed_fixed_damage_move_against_wonder_guard_reports_wonder_guard() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let shedinja_defender = mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([SONIC_BOOM_MISS_DRAW]);

    let outcome =
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &shedinja_defender, &mut rng)
            .unwrap();

    assert_eq!(outcome, HitOutcome::WonderGuardBlocked);
    assert_eq!(
        rng.draws(),
        1,
        "a failed roll still skips the trailing effect-chance draw"
    );
}

/// `Cmd_psywavedamageeffect` (`battle_script_commands.c:7932-7941`): the
/// sample is a 16-way draw accepted only through 10, and the damage is the
/// attacker's level times `(10 * sample + 50)` over 100, truncated.
#[test]
fn psywave_deals_level_scaled_damage_from_the_lowest_and_highest_samples() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 50, vec![PSYWAVE]);
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);

    for (sample_draw, expected_damage) in [(0, 25), (10, 75)] {
        let mut rng =
            SequenceRng::new([ACCURACY_HIT_DRAW, sample_draw, DISCARDED_EFFECT_CHANCE_DRAW]);
        let outcome =
            resolve_fixed_damage_move(&dex, PSYWAVE, &attacker, &defender, &mut rng).unwrap();

        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: expected_damage,
                is_critical: false,
            }
        );
        assert_eq!(rng.draws(), 3, "accuracy, one sample, effect chance");
    }
}

#[test]
fn psywave_truncates_the_scaled_damage() {
    let dex = Dex::new();
    // 33 * (7 * 10 + 50) / 100 = 3960 / 100.
    let attacker = mon(&dex, BULBASAUR, 33, vec![PSYWAVE]);
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);
    let mut rng = SequenceRng::new([ACCURACY_HIT_DRAW, 7, DISCARDED_EFFECT_CHANCE_DRAW]);

    let outcome = resolve_fixed_damage_move(&dex, PSYWAVE, &attacker, &defender, &mut rng).unwrap();

    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: 39,
            is_critical: false,
        }
    );
}

/// A level-1 user's lowest samples scale below one HP and truncate to zero
/// (`1 * 50 / 100`); upstream applies no floor of one on this path.
#[test]
fn psywave_at_level_one_can_deal_zero_damage() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 1, vec![PSYWAVE]);
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);

    for (sample_draw, expected_damage) in [(0, 0), (4, 0), (5, 1), (10, 1)] {
        let mut rng =
            SequenceRng::new([ACCURACY_HIT_DRAW, sample_draw, DISCARDED_EFFECT_CHANCE_DRAW]);
        let outcome =
            resolve_fixed_damage_move(&dex, PSYWAVE, &attacker, &defender, &mut rng).unwrap();

        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: expected_damage,
                is_critical: false,
            }
        );
    }
}

/// Samples above 10 are redrawn, each consuming a draw. The draw is taken
/// modulo 16, so 27 (11) and 31 (15) are rejected and 26 (10) is kept.
#[test]
fn psywave_redraws_rejected_samples_and_each_consumes_a_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 50, vec![PSYWAVE]);
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);
    let script = [
        ACCURACY_HIT_DRAW,
        11,
        15,
        27,
        31,
        26,
        DISCARDED_EFFECT_CHANCE_DRAW,
    ];
    let mut rng = SequenceRng::new(script);

    let outcome = resolve_fixed_damage_move(&dex, PSYWAVE, &attacker, &defender, &mut rng).unwrap();

    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: 75,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), script.len());
}

/// A landed Psywave is accuracy, the sample, and the discarded effect-chance
/// draw: no critical-hit draw and no damage-variance draw follow.
#[test]
fn an_effective_psywave_hit_draws_exactly_three_times_with_no_rejection() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 50, vec![PSYWAVE]);
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);
    let mut rng = SequenceRng::new([ACCURACY_HIT_DRAW, 5, DISCARDED_EFFECT_CHANCE_DRAW]);

    let outcome = resolve_fixed_damage_move(&dex, PSYWAVE, &attacker, &defender, &mut rng).unwrap();

    assert!(matches!(outcome, HitOutcome::Hit { damage: 50, .. }));
    assert_eq!(rng.draws(), 3);
}

/// 80 accuracy: a roll of 80 yields `80 % 100 + 1 == 81`, so the move misses
/// before the sample is drawn.
#[test]
fn a_psywave_miss_consumes_only_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 50, vec![PSYWAVE]);
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);
    let mut rng = SequenceRng::new([80]);

    let outcome = resolve_fixed_damage_move(&dex, PSYWAVE, &attacker, &defender, &mut rng).unwrap();

    assert_eq!(outcome, HitOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

/// The script runs `psywavedamageeffect` before the immunity result is
/// consulted, so a Dark target still costs the sample (including a rejected
/// one) and the trailing draw, and takes nothing.
#[test]
fn psywave_against_a_type_immune_target_still_spends_its_sample_draws() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 50, vec![PSYWAVE]);
    let defender = mon(&dex, POOCHYENA, 50, vec![TACKLE]);
    let script = [ACCURACY_HIT_DRAW, 12, 3, DISCARDED_EFFECT_CHANCE_DRAW];
    let mut rng = SequenceRng::new(script);

    let outcome = resolve_fixed_damage_move(&dex, PSYWAVE, &attacker, &defender, &mut rng).unwrap();

    assert_eq!(outcome, HitOutcome::NoEffect);
    assert_eq!(rng.draws(), script.len());
}

/// Shedinja's Bug/Ghost typing leaves Psychic neutral, so only Wonder Guard
/// blocks it, after the sample is drawn.
#[test]
fn psywave_against_wonder_guard_still_spends_its_sample_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 50, vec![PSYWAVE]);
    let defender = mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    let script = [ACCURACY_HIT_DRAW, 12, 3, DISCARDED_EFFECT_CHANCE_DRAW];
    let mut rng = SequenceRng::new(script);

    let outcome = resolve_fixed_damage_move(&dex, PSYWAVE, &attacker, &defender, &mut rng).unwrap();

    assert_eq!(outcome, HitOutcome::WonderGuardBlocked);
    assert_eq!(rng.draws(), script.len());
}
