use super::{
    ensure_resolvable, is_stat_change_effect, resolve_stat_change_move, stat_change_for_effect,
    ChangedStat, StatChangeDirection, StatChangeEffect, StatChangeMagnitude, StatChangeOutcome,
    CLEAR_BODY, EFFECT_ACCURACY_DOWN, EFFECT_ATTACK_DOWN, EFFECT_DEFENSE_DOWN,
    EFFECT_DEFENSE_DOWN_TWO, EFFECT_DEFENSE_UP, EFFECT_SPECIAL_ATTACK_UP, EFFECT_SPEED_DOWN,
    HYPER_CUTTER, KEEN_EYE, STAT_CHANGE_EFFECTS, WHITE_SMOKE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use assets::{MoveEffect, MoveId, MoveNames, MoveTarget, SpeciesId};

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

const PRIMARY_ABILITY_PERSONALITY: u32 = 0;

fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        species,
        level,
        MAX_IVS,
        PRIMARY_ABILITY_PERSONALITY,
        moves,
    )
    .unwrap()
}

const LEER: MoveId = MoveId(43);
const GROWL: MoveId = MoveId(45);
const TAIL_WHIP: MoveId = MoveId(39);
const STRING_SHOT: MoveId = MoveId(81);
const SAND_ATTACK: MoveId = MoveId(28);
const SCREECH: MoveId = MoveId(103);
const GROWTH: MoveId = MoveId(74);
const HARDEN: MoveId = MoveId(106);
const TACKLE: MoveId = MoveId(33);
const TAIL_GLOW: MoveId = MoveId(294);
const UNKNOWN_MOVE: MoveId = MoveId(60_000);

const FIRST_MOVE_ID: u16 = 1;
const LAST_EMERALD_MOVE_ID: u16 = 354;
const MIN_STAT_CHANGE_MOVES: usize = 20;

const VOLTORB: SpeciesId = SpeciesId(100);
const TENTACOOL: SpeciesId = SpeciesId(72);
const SKARMORY: SpeciesId = SpeciesId(227);
const ZIGZAGOON: SpeciesId = SpeciesId(288);
const WURMPLE: SpeciesId = SpeciesId(290);
const MAKUHITA: SpeciesId = SpeciesId(335);
const ROSELIA: SpeciesId = SpeciesId(363);
const TORKOAL: SpeciesId = SpeciesId(321);
const CORPHISH: SpeciesId = SpeciesId(326);

const EFFECT_HIT: MoveEffect = MoveEffect(0);
const EFFECT_SPEED_UP_AS_HIT: MoveEffect = MoveEffect(12);
const EFFECT_SPECIAL_DEFENSE_UP_AS_HIT: MoveEffect = MoveEffect(14);
const EFFECT_ACCURACY_UP_AS_HIT: MoveEffect = MoveEffect(15);
const EFFECT_SPECIAL_ATTACK_DOWN_AS_HIT: MoveEffect = MoveEffect(21);
const EFFECT_SPECIAL_DEFENSE_DOWN_AS_HIT: MoveEffect = MoveEffect(22);
const EFFECT_ACCURACY_UP_TWO_AS_HIT: MoveEffect = MoveEffect(55);
const EFFECT_EVASION_UP_TWO_AS_HIT: MoveEffect = MoveEffect(56);
const EFFECT_SPECIAL_ATTACK_DOWN_TWO_AS_HIT: MoveEffect = MoveEffect(61);
const EFFECT_ACCURACY_DOWN_TWO_AS_HIT: MoveEffect = MoveEffect(63);
const EFFECT_EVASION_DOWN_TWO_AS_HIT: MoveEffect = MoveEffect(64);
const EFFECT_MINIMIZE: MoveEffect = MoveEffect(108);
const EFFECT_DEFENSE_CURL: MoveEffect = MoveEffect(156);

const HIT_EFFECTS_WITH_STAT_NAMES: [MoveEffect; 10] = [
    EFFECT_SPEED_UP_AS_HIT,
    EFFECT_SPECIAL_DEFENSE_UP_AS_HIT,
    EFFECT_ACCURACY_UP_AS_HIT,
    EFFECT_SPECIAL_ATTACK_DOWN_AS_HIT,
    EFFECT_SPECIAL_DEFENSE_DOWN_AS_HIT,
    EFFECT_ACCURACY_UP_TWO_AS_HIT,
    EFFECT_EVASION_UP_TWO_AS_HIT,
    EFFECT_SPECIAL_ATTACK_DOWN_TWO_AS_HIT,
    EFFECT_ACCURACY_DOWN_TWO_AS_HIT,
    EFFECT_EVASION_DOWN_TWO_AS_HIT,
];

#[test]
fn the_table_covers_the_named_effects_and_nothing_outside_it() {
    assert!(is_stat_change_effect(EFFECT_ATTACK_DOWN));
    assert!(is_stat_change_effect(EFFECT_DEFENSE_DOWN));
    assert!(is_stat_change_effect(EFFECT_SPEED_DOWN));
    assert!(is_stat_change_effect(EFFECT_ACCURACY_DOWN));
    assert!(is_stat_change_effect(EFFECT_DEFENSE_DOWN_TWO));
    assert!(is_stat_change_effect(EFFECT_SPECIAL_ATTACK_UP));
    assert!(is_stat_change_effect(EFFECT_DEFENSE_UP));

    assert!(!is_stat_change_effect(EFFECT_HIT));
    for effect in HIT_EFFECTS_WITH_STAT_NAMES {
        assert!(!is_stat_change_effect(effect));
        assert!(
            crate::hit::is_ordinary_hit_effect(effect),
            "{effect:?} must remain assigned to ordinary hit resolution"
        );
    }
    assert!(!is_stat_change_effect(EFFECT_MINIMIZE));
    assert!(!is_stat_change_effect(EFFECT_DEFENSE_CURL));
}

#[test]
fn every_transcribed_row_agrees_with_the_real_move_table() {
    let dex = Dex::new();
    let mut seen = 0usize;
    for move_id in FIRST_MOVE_ID..=LAST_EMERALD_MOVE_ID {
        let mv = dex.move_data(MoveId(move_id)).unwrap();
        let Some(change) = stat_change_for_effect(mv.effect) else {
            continue;
        };
        seen += 1;
        assert_eq!(mv.power, 0, "move {move_id}: stat-change moves are 0-power");
        match change.direction {
            StatChangeDirection::Raise => {
                assert_eq!(
                    mv.target,
                    MoveTarget::USER,
                    "move {move_id} raises, so it must be MOVE_TARGET_USER"
                );
                if MoveId(move_id) == TAIL_GLOW {
                    assert_eq!(mv.accuracy, 100, "Tail Glow's inert accuracy byte");
                } else {
                    assert_eq!(
                        mv.accuracy, 0,
                        "move {move_id} raises, so upstream leaves its accuracy byte at 0"
                    );
                }
            }
            StatChangeDirection::Lower => {
                assert_ne!(
                    mv.target,
                    MoveTarget::USER,
                    "move {move_id} lowers, so it targets the foe"
                );
                assert!(
                    mv.accuracy > 0,
                    "move {move_id} lowers, so it has a real accuracy to roll"
                );
            }
        }
    }
    assert!(
        seen >= MIN_STAT_CHANGE_MOVES,
        "the sweep must actually reach a useful number of real moves, saw {seen}"
    );
}

#[test]
fn ensure_resolvable_accepts_every_family_member_and_rejects_outsiders() {
    let dex = Dex::new();
    for move_id in [
        GROWL,
        LEER,
        TAIL_WHIP,
        STRING_SHOT,
        SAND_ATTACK,
        SCREECH,
        GROWTH,
        HARDEN,
    ] {
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()), "{move_id:?}");
    }
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(
        ensure_resolvable(&dex, UNKNOWN_MOVE),
        Err(BattleError::UnknownMove(UNKNOWN_MOVE))
    );
}

#[test]
fn a_hit_growl_lowers_the_targets_attack_by_one_and_draws_once() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 3, vec![GROWL]);
    let defender = mon(&dex, WURMPLE, 3, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    let StatChangeOutcome::Applied {
        change,
        new_stage,
        capped,
    } = outcome
    else {
        panic!("Growl must connect: {outcome:?}");
    };
    assert_eq!(change.stat, ChangedStat::Attack);
    assert_eq!(change.direction, StatChangeDirection::Lower);
    assert_eq!(change.delta(), -1);
    assert!(!change.affects_user());
    assert_eq!(new_stage, StatStage::new(-1).unwrap());
    assert!(!capped);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_hit_screech_lowers_defense_by_two() {
    let dex = Dex::new();
    let attacker = mon(&dex, VOLTORB, 15, vec![SCREECH]);
    let defender = mon(&dex, WURMPLE, 15, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, SCREECH, &attacker, &defender, &mut rng).unwrap();
    let StatChangeOutcome::Applied {
        change, new_stage, ..
    } = outcome
    else {
        panic!("Screech must connect: {outcome:?}");
    };
    assert_eq!(change.stat, ChangedStat::Defense);
    assert_eq!(change.delta(), -2);
    assert_eq!(new_stage, StatStage::new(-2).unwrap());
    assert_eq!(rng.draws(), 1);
}

#[test]
fn sand_attack_lowers_the_targets_accuracy_stage() {
    let dex = Dex::new();
    let attacker = mon(&dex, MAKUHITA, 15, vec![SAND_ATTACK]);
    let defender = mon(&dex, WURMPLE, 15, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_stat_change_move(&dex, SAND_ATTACK, &attacker, &defender, &mut rng).unwrap();
    let StatChangeOutcome::Applied {
        change, new_stage, ..
    } = outcome
    else {
        panic!("Sand Attack must connect: {outcome:?}");
    };
    assert_eq!(change.stat, ChangedStat::Accuracy);
    assert_eq!(new_stage, StatStage::new(-1).unwrap());
}

#[test]
fn a_stat_raising_move_draws_nothing_and_raises_the_users_own_stage() {
    let dex = Dex::new();
    let attacker = mon(&dex, ROSELIA, 14, vec![GROWTH]);
    let defender = mon(&dex, WURMPLE, 14, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    let outcome = resolve_stat_change_move(&dex, GROWTH, &attacker, &defender, &mut rng).unwrap();
    let StatChangeOutcome::Applied {
        change,
        new_stage,
        capped,
    } = outcome
    else {
        panic!("Growth cannot miss: {outcome:?}");
    };
    assert_eq!(change.stat, ChangedStat::SpAttack);
    assert_eq!(change.direction, StatChangeDirection::Raise);
    assert!(
        change.affects_user(),
        "Growth raises the *user's* Sp. Attack"
    );
    assert_eq!(new_stage, StatStage::new(1).unwrap());
    assert!(!capped);
    assert_eq!(
        rng.draws(),
        0,
        "BattleScript_EffectStatUp has no accuracycheck"
    );
}

#[test]
fn a_raise_reads_the_users_stage_and_a_drop_reads_the_targets() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, ROSELIA, 14, vec![GROWTH, GROWL]);
    let mut defender = mon(&dex, WURMPLE, 14, vec![TACKLE]);
    attacker.stages_mut().sp_attack = StatStage::new(4).unwrap();
    attacker.stages_mut().attack = StatStage::new(-3).unwrap();
    defender.stages_mut().sp_attack = StatStage::MIN;
    defender.stages_mut().attack = StatStage::new(2).unwrap();

    let mut rng = SequenceRng::new([]);
    let raise = resolve_stat_change_move(&dex, GROWTH, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(raise, StatChangeOutcome::Applied { new_stage, .. }
            if new_stage == StatStage::new(5).unwrap()),
        "the raise must start from the *attacker's* +4, not the defender's floor: {raise:?}"
    );

    let mut rng = SequenceRng::new([0]);
    let drop = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(drop, StatChangeOutcome::Applied { new_stage, .. }
            if new_stage == StatStage::new(1).unwrap()),
        "the drop must start from the *defender's* +2, not the attacker's -3: {drop:?}"
    );
}

#[test]
fn a_missed_lowering_move_reports_miss_and_still_draws_once() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 3, vec![STRING_SHOT]);
    let defender = mon(&dex, ZIGZAGOON, 3, vec![TACKLE]);
    let mut rng = SequenceRng::new([95]);
    let outcome =
        resolve_stat_change_move(&dex, STRING_SHOT, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, StatChangeOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_floored_drop_and_a_capped_raise_both_report_capped_without_moving_the_stage() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, ROSELIA, 14, vec![GROWTH]);
    let mut defender = mon(&dex, WURMPLE, 14, vec![TACKLE]);
    attacker.stages_mut().sp_attack = StatStage::MAX;
    defender.stages_mut().attack = StatStage::MIN;

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_stat_change_move(&dex, GROWTH, &attacker, &defender, &mut rng).unwrap(),
        StatChangeOutcome::Applied {
            change: stat_change_for_effect(dex.move_data(GROWTH).unwrap().effect).unwrap(),
            new_stage: StatStage::MAX,
            capped: true,
        }
    );
    assert_eq!(rng.draws(), 0);

    let growler = mon(&dex, ZIGZAGOON, 3, vec![GROWL]);
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        resolve_stat_change_move(&dex, GROWL, &growler, &defender, &mut rng).unwrap(),
        StatChangeOutcome::Applied {
            change: stat_change_for_effect(dex.move_data(GROWL).unwrap().effect).unwrap(),
            new_stage: StatStage::MIN,
            capped: true,
        }
    );
    assert_eq!(
        rng.draws(),
        1,
        "the floored case still costs the accuracy roll"
    );
}

#[test]
fn a_two_stage_drop_clamps_at_the_floor_without_reporting_capped() {
    let dex = Dex::new();
    let attacker = mon(&dex, VOLTORB, 15, vec![SCREECH]);
    let mut defender = mon(&dex, WURMPLE, 15, vec![TACKLE]);
    defender.stages_mut().defense = StatStage::new(-5).unwrap();

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, SCREECH, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        StatChangeOutcome::Applied {
            change: stat_change_for_effect(dex.move_data(SCREECH).unwrap().effect).unwrap(),
            new_stage: StatStage::MIN,
            capped: false,
        },
        "-5 - 2 clamps to -6 (ChangeStatBuffs' add-then-clamp), and the stage really moved"
    );
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 3, vec![TACKLE]);
    let defender = mon(&dex, WURMPLE, 3, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_stat_change_move(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

#[test]
fn the_transcribed_table_has_no_duplicate_rows() {
    for (index, (id, _)) in STAT_CHANGE_EFFECTS.iter().enumerate() {
        assert!(
            !STAT_CHANGE_EFFECTS[..index]
                .iter()
                .any(|(other, _)| other == id),
            "EFFECT id {} appears twice",
            id.0
        );
    }
}

#[test]
fn a_clear_body_holders_drop_is_blocked_after_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 15, vec![GROWL]);
    let defender = mon(&dex, TENTACOOL, 15, vec![TACKLE]);
    assert_eq!(
        defender.ability(),
        CLEAR_BODY,
        "fixture sanity: personality 0 selects slot 0, Clear Body"
    );

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        StatChangeOutcome::AbilityProtected {
            change: stat_change_for_effect(dex.move_data(GROWL).unwrap().effect).unwrap(),
            ability: CLEAR_BODY,
        }
    );
    assert_eq!(
        rng.draws(),
        1,
        "a blocked drop still costs its one accuracy draw"
    );
}

#[test]
fn a_white_smoke_holders_drop_is_blocked_identically() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 15, vec![GROWL]);
    let defender = mon(&dex, TORKOAL, 15, vec![TACKLE]);
    assert_eq!(
        defender.ability(),
        WHITE_SMOKE,
        "fixture sanity: Torkoal's only ability slot is White Smoke"
    );

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        StatChangeOutcome::AbilityProtected {
            change: stat_change_for_effect(dex.move_data(GROWL).unwrap().effect).unwrap(),
            ability: WHITE_SMOKE,
        }
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn clear_body_blocks_even_when_already_at_the_floor() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 15, vec![GROWL]);
    let mut defender = mon(&dex, TENTACOOL, 15, vec![TACKLE]);
    defender.stages_mut().attack = StatStage::MIN;

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(outcome, StatChangeOutcome::AbilityProtected { ability, .. } if ability == CLEAR_BODY),
        "{outcome:?}"
    );
}

#[test]
fn clear_body_never_guards_its_holders_own_raise() {
    let dex = Dex::new();
    let attacker = mon(&dex, TENTACOOL, 15, vec![GROWTH]);
    let defender = mon(&dex, WURMPLE, 15, vec![TACKLE]);
    assert_eq!(attacker.ability(), CLEAR_BODY);

    let mut rng = SequenceRng::new([]);
    let outcome = resolve_stat_change_move(&dex, GROWTH, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(outcome, StatChangeOutcome::Applied { capped: false, .. }),
        "Clear Body must not block its own holder's raise: {outcome:?}"
    );
}

#[test]
fn a_non_clear_body_holders_drop_is_unaffected() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 3, vec![GROWL]);
    let defender = mon(&dex, WURMPLE, 3, vec![TACKLE]);
    assert_ne!(defender.ability(), CLEAR_BODY);

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert!(matches!(outcome, StatChangeOutcome::Applied { .. }));
}

#[test]
fn a_keen_eye_holders_accuracy_drop_is_blocked() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 15, vec![SAND_ATTACK]);
    let defender = mon(&dex, SKARMORY, 15, vec![TACKLE]);
    assert_eq!(
        defender.ability(),
        KEEN_EYE,
        "fixture sanity: personality 0 selects slot 0, Keen Eye"
    );

    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_stat_change_move(&dex, SAND_ATTACK, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        StatChangeOutcome::AbilityProtected {
            change: stat_change_for_effect(dex.move_data(SAND_ATTACK).unwrap().effect).unwrap(),
            ability: KEEN_EYE,
        }
    );
    assert_eq!(
        rng.draws(),
        1,
        "a blocked drop still costs its one accuracy draw"
    );
}

#[test]
fn a_keen_eye_holders_other_stats_still_drop() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 15, vec![GROWL]);
    let defender = mon(&dex, SKARMORY, 15, vec![TACKLE]);

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(outcome, StatChangeOutcome::Applied { .. }),
        "Keen Eye must not guard a non-Accuracy stat: {outcome:?}"
    );
}

#[test]
fn a_hyper_cutter_holders_attack_drop_is_blocked() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 15, vec![GROWL]);
    let defender = mon(&dex, CORPHISH, 15, vec![TACKLE]);
    assert_eq!(
        defender.ability(),
        HYPER_CUTTER,
        "fixture sanity: personality 0 selects slot 0, Hyper Cutter"
    );

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        StatChangeOutcome::AbilityProtected {
            change: stat_change_for_effect(dex.move_data(GROWL).unwrap().effect).unwrap(),
            ability: HYPER_CUTTER,
        }
    );
    assert_eq!(
        rng.draws(),
        1,
        "a blocked drop still costs its one accuracy draw"
    );
}

#[test]
fn a_hyper_cutter_holders_other_stats_still_drop() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON, 15, vec![LEER]);
    let defender = mon(&dex, CORPHISH, 15, vec![TACKLE]);

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, LEER, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(outcome, StatChangeOutcome::Applied { .. }),
        "Hyper Cutter must not guard a non-Attack stat: {outcome:?}"
    );
}

#[test]
fn stat_change_magnitude_names_exactly_the_two_upstream_values() {
    assert_eq!(StatChangeMagnitude::One.get(), 1);
    assert_eq!(StatChangeMagnitude::Two.get(), 2);

    let raise_two = StatChangeEffect {
        stat: ChangedStat::Attack,
        magnitude: StatChangeMagnitude::Two,
        direction: StatChangeDirection::Raise,
    };
    assert_eq!(raise_two.delta(), 2);

    let lower_two = StatChangeEffect {
        stat: ChangedStat::Attack,
        magnitude: StatChangeMagnitude::Two,
        direction: StatChangeDirection::Lower,
    };
    assert_eq!(lower_two.delta(), -2);
}

/// Anchors each `STAT_CHANGE_EFFECTS` row to a canonically named move and asserts the exact
/// stat, magnitude, and direction upstream assigns it, then asserts every row was reached.
#[test]
fn every_table_row_maps_its_canonical_move_to_the_exact_stat_and_magnitude() {
    use ChangedStat::{Accuracy, Attack, Defense, Evasion, SpAttack, SpDefense, Speed};
    use StatChangeDirection::{Lower, Raise};
    use StatChangeMagnitude::{One, Two};

    // Move-to-effect assignment cited by its line in `pokeemerald/src/data/battle_moves.h`.
    const ANCHORS: [(&str, ChangedStat, StatChangeMagnitude, StatChangeDirection); 18] = [
        ("MEDITATE", Attack, One, Raise),       // battle_moves.h:1251
        ("HARDEN", Defense, One, Raise),        // battle_moves.h:1381
        ("GROWTH", SpAttack, One, Raise),       // battle_moves.h:965
        ("DOUBLE TEAM", Evasion, One, Raise),   // battle_moves.h:1355
        ("GROWL", Attack, One, Lower),          // battle_moves.h:588
        ("TAIL WHIP", Defense, One, Lower),     // battle_moves.h:510
        ("STRING SHOT", Speed, One, Lower),     // battle_moves.h:1056
        ("SAND-ATTACK", Accuracy, One, Lower),  // battle_moves.h:367
        ("SWEET SCENT", Evasion, One, Lower),   // battle_moves.h:2993
        ("SWORDS DANCE", Attack, Two, Raise),   // battle_moves.h:185
        ("BARRIER", Defense, Two, Raise),       // battle_moves.h:1459
        ("AGILITY", Speed, Two, Raise),         // battle_moves.h:1264
        ("TAIL GLOW", SpAttack, Two, Raise),    // battle_moves.h:3825
        ("AMNESIA", SpDefense, Two, Raise),     // battle_moves.h:1732
        ("CHARM", Attack, Two, Lower),          // battle_moves.h:2655
        ("SCREECH", Defense, Two, Lower),       // battle_moves.h:1342
        ("SCARY FACE", Speed, Two, Lower),      // battle_moves.h:2395
        ("METAL SOUND", SpDefense, Two, Lower), // battle_moves.h:4150
    ];

    let dex = Dex::new();
    let move_names = MoveNames::new();
    let mut reached = Vec::with_capacity(ANCHORS.len());

    for (name, stat, magnitude, direction) in ANCHORS {
        let move_id = (FIRST_MOVE_ID..=LAST_EMERALD_MOVE_ID)
            .map(MoveId)
            .find(|&move_id| move_names.name(move_id) == Ok(name))
            .unwrap_or_else(|| panic!("canonical move name {name:?} is absent from the table"));
        let effect = dex.move_data(move_id).unwrap().effect;

        assert_eq!(
            stat_change_for_effect(effect),
            Some(StatChangeEffect {
                stat,
                magnitude,
                direction
            }),
            "{name} ({move_id:?}, effect {}) must map to its canonical stat and magnitude",
            effect.0
        );
        reached.push(effect);
    }

    for (effect, _) in STAT_CHANGE_EFFECTS {
        assert!(
            reached.contains(&effect),
            "effect {} has no canonical move anchoring its stat and magnitude",
            effect.0
        );
    }
}
