//! `AI_CheckViability` routes the two-stage Attack and Defense drops to
//! `AI_CV_AttackDown` and `AI_CV_DefenseDown`
//! (`data/battle_ai_scripts.s:695-696`, `:1090`-`:1134`). Charm and Feather
//! Dance reach the first, Screech the second.

use super::{
    choose_trainer_action, current_hp_percent, ensure_scoreable, score_viability, MoveScores,
    PERCENT_SCALE,
};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use assets::trainers::AiFlags;
use assets::{MoveId, SpeciesId, Type};

const LEVEL: u8 = 50;
const INIT_DRAWS: usize = MAX_MON_MOVES;
const START: i8 = 100;

/// `if_random_less_than 50` ends the branch below 50; 50 is the first -2.
const ROLL_BELOW: u16 = 49;
const ROLL_AT: u16 = 50;
const ROLL_MAX: u16 = 255;
/// `Random()` % 256 is what upstream compares, so the high byte is ignored.
const HIGH_BYTE: u16 = 0x0100;

const ATTACK_DROPS: [MoveId; 2] = [MoveId::CHARM, MoveId::FEATHER_DANCE];
const DEFENSE_DROPS: [MoveId; 1] = [MoveId::SCREECH];

const TREECKO: SpeciesId = SpeciesId(277);
const PINSIR: SpeciesId = SpeciesId(127);
const PIDGEY: SpeciesId = SpeciesId(16);
const ZUBAT: SpeciesId = SpeciesId(41);
const GASTLY: SpeciesId = SpeciesId(92);
const MURKROW: SpeciesId = SpeciesId(198);
const NIDOKING: SpeciesId = SpeciesId(34);

fn mon(species: SpeciesId, moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        species,
        LEVEL,
        Ivs::default(),
        0,
        moves.to_vec(),
    )
    .expect("test Pokemon must be dex-resident")
}

/// Leaves `pokemon` at the largest HP whose truncated percentage is `percent`
/// (`100 * hp / maxHP`, `src/battle_ai_script_commands.c:737`).
fn at_hp_percent(mut pokemon: BattlePokemon, percent: u32) -> BattlePokemon {
    let max = pokemon.stats().max_hp;
    let hp = (1..=max)
        .rev()
        .find(|hp| PERCENT_SCALE * hp / max == percent)
        .expect("some HP truncates to the percentage");
    pokemon.apply_damage(pokemon.current_hp() - hp);
    assert_eq!(current_hp_percent(&pokemon), percent);
    pokemon
}

fn attack_staged(mut pokemon: BattlePokemon, offset: i8) -> BattlePokemon {
    pokemon.stages_mut().attack = StatStage::new(offset).unwrap();
    pokemon
}

fn defense_staged(mut pokemon: BattlePokemon, offset: i8) -> BattlePokemon {
    pokemon.stages_mut().defense = StatStage::new(offset).unwrap();
    pokemon
}

/// Scores slot 0 under `AI_CheckViability` after the simulated-damage
/// initialisation draws, feeding `tail` to the branch and requiring it to be
/// consumed exactly. Returns the score and the draws the branch alone spent.
fn viability(
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
    tail: &[u16],
) -> (i8, usize) {
    let mut script = vec![0; INIT_DRAWS];
    script.extend_from_slice(tail);
    let mut rng = SequenceRng::new(script);
    let mut scores = MoveScores::initialize(user, &mut rng);
    assert_eq!(rng.draws(), INIT_DRAWS);
    score_viability(&Dex::new(), &mut scores, 0, move_id, user, target, &mut rng).unwrap();
    rng.assert_exhausted();
    (scores.values[0], rng.draws() - INIT_DRAWS)
}

/// A full-HP user and a full-HP target of `species`, both unstaged.
fn pair(move_id: MoveId, species: SpeciesId) -> (BattlePokemon, BattlePokemon) {
    (mon(TREECKO, &[move_id]), mon(species, &[MoveId::POUND]))
}

#[test]
fn charm_and_feather_dance_stay_unchanged_at_neutral_stage_against_a_healthy_physical_target() {
    for move_id in ATTACK_DROPS {
        let (user, target) = pair(move_id, PINSIR);
        // The user's HP is only read once the stage has moved.
        for user_percent in [PERCENT_SCALE, 1] {
            let user = at_hp_percent(user.clone(), user_percent);
            assert_eq!(viability(move_id, &user, &target, &[]), (START, 0));
        }
    }
}

#[test]
fn a_target_with_no_listed_type_rolls_once_and_the_boundary_is_49_then_50() {
    for move_id in ATTACK_DROPS {
        let (user, target) = pair(move_id, TREECKO);
        for roll in [0, ROLL_BELOW, HIGH_BYTE | ROLL_BELOW] {
            assert_eq!(viability(move_id, &user, &target, &[roll]), (START, 1));
        }
        for roll in [ROLL_AT, ROLL_MAX, HIGH_BYTE | ROLL_AT] {
            assert_eq!(viability(move_id, &user, &target, &[roll]), (START - 2, 1));
        }
    }
}

#[test]
fn only_the_six_listed_types_skip_the_type_roll_in_either_slot() {
    // (species, types, whether a listed type is present)
    let cases = [
        (PINSIR, [Type::Bug, Type::Bug], true),
        (PIDGEY, [Type::Normal, Type::Flying], true),
        (NIDOKING, [Type::Poison, Type::Ground], true),
        (ZUBAT, [Type::Poison, Type::Flying], false),
        (GASTLY, [Type::Ghost, Type::Poison], false),
        (MURKROW, [Type::Dark, Type::Flying], false),
        (TREECKO, [Type::Grass, Type::Grass], false),
    ];
    for move_id in ATTACK_DROPS {
        for (species, types, listed) in cases {
            let (user, target) = pair(move_id, species);
            assert_eq!(target.types(), types, "{species:?}");
            let expected = if listed { (START, 0) } else { (START - 2, 1) };
            let tail: &[u16] = if listed { &[] } else { &[ROLL_AT] };
            assert_eq!(
                viability(move_id, &user, &target, tail),
                expected,
                "{move_id:?} against {species:?}"
            );
        }
    }
}

#[test]
fn a_moved_stage_costs_one_and_a_second_point_at_90_percent_or_less_with_no_roll_above_minus_three()
{
    for move_id in ATTACK_DROPS {
        for stage in [-2, -1, 1, 6] {
            let (user, target) = pair(move_id, PINSIR);
            let target = attack_staged(target, stage);
            assert_eq!(
                viability(move_id, &user, &target, &[]),
                (START - 1, 0),
                "{move_id:?} stage {stage}"
            );
            assert_eq!(
                viability(move_id, &at_hp_percent(user.clone(), 91), &target, &[]),
                (START - 1, 0),
                "{move_id:?} stage {stage}: 91% is above the cut-off"
            );
            assert_eq!(
                viability(move_id, &at_hp_percent(user.clone(), 90), &target, &[]),
                (START - 2, 0),
                "{move_id:?} stage {stage}: 90% is at the cut-off"
            );
        }
    }
}

#[test]
fn a_stage_of_minus_three_or_lower_rolls_once_and_the_boundary_is_49_then_50() {
    for move_id in ATTACK_DROPS {
        for stage in [-3, -6] {
            let (user, target) = pair(move_id, PINSIR);
            let target = attack_staged(target, stage);
            assert_eq!(
                viability(move_id, &user, &target, &[ROLL_BELOW]),
                (START - 1, 1),
                "{move_id:?} stage {stage}"
            );
            assert_eq!(
                viability(move_id, &user, &target, &[HIGH_BYTE | ROLL_BELOW]),
                (START - 1, 1),
                "{move_id:?} stage {stage}"
            );
            assert_eq!(
                viability(move_id, &user, &target, &[ROLL_AT]),
                (START - 3, 1),
                "{move_id:?} stage {stage}"
            );
            assert_eq!(
                viability(move_id, &user, &target, &[HIGH_BYTE | ROLL_MAX]),
                (START - 3, 1),
                "{move_id:?} stage {stage}"
            );
        }
    }
}

#[test]
fn a_target_at_70_percent_or_less_costs_two_more_with_no_roll() {
    for move_id in ATTACK_DROPS {
        let (user, target) = pair(move_id, PINSIR);
        for (percent, expected) in [(71, START), (70, START - 2), (1, START - 2)] {
            let target = at_hp_percent(target.clone(), percent);
            assert_eq!(
                viability(move_id, &user, &target, &[]),
                (expected, 0),
                "{move_id:?} target at {percent}%"
            );
        }
    }
}

#[test]
fn every_attack_drop_penalty_stacks_with_both_rolls_drawn_in_order() {
    for move_id in ATTACK_DROPS {
        let user = at_hp_percent(mon(TREECKO, &[move_id]), 90);
        let target = at_hp_percent(attack_staged(mon(ZUBAT, &[MoveId::POUND]), -3), 70);
        // -1 stage, -1 user HP, -2 target HP, plus -2 for each roll at 50 or more.
        for (rolls, expected) in [
            ([ROLL_BELOW, ROLL_BELOW], START - 4),
            ([ROLL_AT, ROLL_BELOW], START - 6),
            ([ROLL_BELOW, ROLL_AT], START - 6),
            ([ROLL_AT, ROLL_AT], START - 8),
        ] {
            assert_eq!(
                viability(move_id, &user, &target, &rolls),
                (expected, 2),
                "{move_id:?} rolls {rolls:?}"
            );
        }
    }
}

#[test]
fn the_attack_branch_ignores_the_targets_defense_stage() {
    for move_id in ATTACK_DROPS {
        let (user, target) = pair(move_id, PINSIR);
        let target = defense_staged(target, -6);
        assert_eq!(viability(move_id, &user, &target, &[]), (START, 0));
    }
}

#[test]
fn a_healthy_user_against_a_defense_above_minus_three_skips_the_roll() {
    for move_id in DEFENSE_DROPS {
        // Neither the target's types nor its Attack stage matter here.
        for species in [PINSIR, ZUBAT] {
            let (user, target) = pair(move_id, species);
            for stage in [-2, 0, 6] {
                let target = defense_staged(attack_staged(target.clone(), -6), stage);
                for user_percent in [PERCENT_SCALE, 70] {
                    let user = at_hp_percent(user.clone(), user_percent);
                    assert_eq!(
                        viability(move_id, &user, &target, &[]),
                        (START, 0),
                        "{move_id:?} against {species:?}, stage {stage}, user {user_percent}%"
                    );
                }
            }
        }
    }
}

#[test]
fn a_user_below_70_percent_rolls_once_and_the_boundary_is_49_then_50() {
    for move_id in DEFENSE_DROPS {
        let (user, target) = pair(move_id, TREECKO);
        for user_percent in [69, 1] {
            let user = at_hp_percent(user.clone(), user_percent);
            for roll in [0, ROLL_BELOW, HIGH_BYTE | ROLL_BELOW] {
                assert_eq!(viability(move_id, &user, &target, &[roll]), (START, 1));
            }
            for roll in [ROLL_AT, ROLL_MAX, HIGH_BYTE | ROLL_AT] {
                assert_eq!(viability(move_id, &user, &target, &[roll]), (START - 2, 1));
            }
        }
    }
}

#[test]
fn a_defense_stage_of_minus_three_or_lower_rolls_for_a_healthy_user_too() {
    for move_id in DEFENSE_DROPS {
        for stage in [-3, -6] {
            let (user, target) = pair(move_id, TREECKO);
            let target = defense_staged(target, stage);
            assert_eq!(
                viability(move_id, &user, &target, &[ROLL_BELOW]),
                (START, 1),
                "{move_id:?} stage {stage}"
            );
            assert_eq!(
                viability(move_id, &user, &target, &[ROLL_AT]),
                (START - 2, 1),
                "{move_id:?} stage {stage}"
            );
        }
    }
}

#[test]
fn a_defense_drop_target_at_70_percent_or_less_costs_two_more_with_no_roll() {
    for move_id in DEFENSE_DROPS {
        let (user, target) = pair(move_id, TREECKO);
        for (percent, expected) in [(71, START), (70, START - 2), (1, START - 2)] {
            let target = at_hp_percent(target.clone(), percent);
            assert_eq!(
                viability(move_id, &user, &target, &[]),
                (expected, 0),
                "{move_id:?} target at {percent}%"
            );
        }
    }
}

#[test]
fn the_defense_roll_and_the_target_hp_penalty_stack() {
    for move_id in DEFENSE_DROPS {
        let user = at_hp_percent(mon(TREECKO, &[move_id]), 69);
        let target = at_hp_percent(mon(TREECKO, &[MoveId::POUND]), 70);
        assert_eq!(
            viability(move_id, &user, &target, &[ROLL_BELOW]),
            (START - 2, 1)
        );
        assert_eq!(
            viability(move_id, &user, &target, &[ROLL_AT]),
            (START - 4, 1)
        );
    }
}

#[test]
fn admission_widens_for_the_two_effects_under_check_viability_only() {
    let dex = Dex::new();
    let viability = AiFlags::CHECK_VIABILITY;
    for move_id in ATTACK_DROPS.into_iter().chain(DEFENSE_DROPS) {
        for flags in [
            viability,
            viability.union(AiFlags::CHECK_BAD_MOVE),
            viability
                .union(AiFlags::CHECK_BAD_MOVE)
                .union(AiFlags::TRY_TO_FAINT),
        ] {
            assert_eq!(
                ensure_scoreable(&dex, move_id, flags),
                Ok(()),
                "{move_id:?}"
            );
        }
        for flags in [
            AiFlags::SETUP_FIRST_TURN,
            viability.union(AiFlags::SETUP_FIRST_TURN),
        ] {
            assert_eq!(
                ensure_scoreable(&dex, move_id, flags),
                Err(BattleError::UnscoreableMoveEffect(move_id)),
                "{move_id:?}"
            );
        }
    }
}

#[test]
fn other_unported_stat_labels_stay_refused_under_check_viability() {
    let dex = Dex::new();
    for move_id in [
        MoveId::FAKE_TEARS,
        MoveId::SAND_ATTACK,
        MoveId::SWEET_SCENT,
        MoveId::SWORDS_DANCE,
        MoveId::TAIL_GLOW,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, AiFlags::CHECK_VIABILITY),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

#[test]
fn a_viability_trainer_scores_charm_and_screech_through_the_full_pipeline() {
    let dex = Dex::new();
    let flags = AiFlags::CHECK_VIABILITY;
    let player = mon(TREECKO, &[MoveId::POUND]);

    // Charm's type roll lands at 50: 98 loses to Pound's 100; one tie-break draw.
    let enemy = mon(TREECKO, &[MoveId::CHARM, MoveId::POUND]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, ROLL_AT, 0]);
    assert_eq!(
        choose_trainer_action(&dex, &enemy, &player, flags, 0, &mut rng),
        Ok(EnemyAction::Move(1))
    );
    assert_eq!(rng.draws(), INIT_DRAWS + 2);
    rng.assert_exhausted();

    // A healthy user at neutral Defense: Screech spends no roll and ties Pound.
    let enemy = mon(TREECKO, &[MoveId::SCREECH, MoveId::POUND]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1]);
    assert_eq!(
        choose_trainer_action(&dex, &enemy, &player, flags, 0, &mut rng),
        Ok(EnemyAction::Move(1))
    );
    assert_eq!(rng.draws(), INIT_DRAWS + 1);
    rng.assert_exhausted();
}
