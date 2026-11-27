//! `AI_CV_Toxic` as reached by Leech Seed (`data/battle_ai_scripts.s:711`,
//! `:1353`-`:1371`): separate user and target HP penalties behind an
//! attacking-move scan, then a bonus for a known Protect or Special Defense
//! raise. Both scans read known moves, not usable PP
//! (`src/battle_ai_script_commands.c:1092`-`:1106`, `:1901`-`:1917`).

use super::{
    current_hp_percent, ensure_scoreable, is_leech_seed_defensive_effect, score_viability,
    MoveScores, PERCENT_SCALE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId};

const LEVEL: u8 = 50;
const UNCHANGED_SCORE: i8 = 100;
const INIT_DRAWS: usize = MAX_MON_MOVES;
const ONE_PENALTY_SCORE: i8 = 97;
const TWO_PENALTIES_SCORE: i8 = 94;
const BONUS_SCORE: i8 = 102;
const BOTH_PENALTIES_AND_BONUS_SCORE: i8 = 96;
const ONE_PENALTY_AND_BONUS_SCORE: i8 = 99;

/// `if_random_less_than 50` skips a penalty below 50.
const BELOW_PENALTY_ROLL: u16 = 49;
const AT_PENALTY_ROLL: u16 = 50;
/// `if_random_less_than 60` skips the bonus below 60.
const BELOW_BONUS_ROLL: u16 = 59;
const AT_BONUS_ROLL: u16 = 60;
/// Only the low byte is compared.
const HIGH_BYTE: u16 = 0x0100;
/// `if_hp_more_than 50` skips its penalty above 50%.
const HP_AT_THRESHOLD: u32 = 50;
const HP_ABOVE_THRESHOLD: u32 = 51;

const TREECKO: SpeciesId = SpeciesId(277);

fn mon(moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        TREECKO,
        LEVEL,
        Ivs::default(),
        0,
        moves.to_vec(),
    )
    .expect("test Pokemon must be dex-resident")
}

/// Leaves `pokemon` at the largest HP whose truncated percentage is `percent`.
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

fn spend_move(mut pokemon: BattlePokemon, slot: usize) -> BattlePokemon {
    while pokemon.moves()[slot].pp > 0 {
        pokemon.deduct_pp(slot).unwrap();
    }
    pokemon
}

fn user_with(other_moves: &[MoveId]) -> BattlePokemon {
    let mut moves = vec![MoveId::LEECH_SEED];
    moves.extend_from_slice(other_moves);
    mon(&moves)
}

fn target() -> BattlePokemon {
    mon(&[MoveId::POUND])
}

/// Scores slot 0 (Leech Seed) under `AI_CheckViability`, feeding `tail` to the
/// branch and requiring it to be consumed exactly. Returns the score.
fn viability(user: &BattlePokemon, target: &BattlePokemon, tail: &[u16]) -> i8 {
    let mut script = vec![0; INIT_DRAWS];
    script.extend_from_slice(tail);
    let mut rng = SequenceRng::new(script);
    let mut scores = MoveScores::initialize(user, &mut rng);
    assert_eq!(rng.draws(), INIT_DRAWS);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        MoveId::LEECH_SEED,
        user,
        target,
        &mut rng,
    )
    .unwrap();
    rng.assert_exhausted();
    scores.values[0]
}

#[test]
fn a_user_with_no_attacking_move_skips_both_penalties_without_a_draw() {
    for other_moves in [&[][..], &[MoveId::GROWL, MoveId::SWORDS_DANCE]] {
        let user = at_hp_percent(user_with(other_moves), 1);
        let target = at_hp_percent(target(), 1);
        assert_eq!(viability(&user, &target, &[]), UNCHANGED_SCORE);
    }
}

#[test]
fn a_one_power_move_is_an_attacking_move() {
    for move_id in [MoveId::SEISMIC_TOSS, MoveId::DRAGON_RAGE] {
        let user = at_hp_percent(user_with(&[move_id]), HP_AT_THRESHOLD);
        assert_eq!(
            viability(&user, &target(), &[AT_PENALTY_ROLL]),
            ONE_PENALTY_SCORE,
            "{move_id:?}"
        );
    }
}

#[test]
fn a_depleted_only_damaging_slot_still_counts_as_attacking() {
    let user = at_hp_percent(spend_move(user_with(&[MoveId::TACKLE]), 1), HP_AT_THRESHOLD);
    assert_eq!(user.moves()[1].pp, 0);
    assert_eq!(
        viability(&user, &target(), &[AT_PENALTY_ROLL]),
        ONE_PENALTY_SCORE
    );
}

#[test]
fn a_user_above_half_health_draws_nothing_for_its_own_penalty() {
    let user = at_hp_percent(user_with(&[MoveId::TACKLE]), HP_ABOVE_THRESHOLD);
    assert_eq!(viability(&user, &target(), &[]), UNCHANGED_SCORE);
}

#[test]
fn a_user_at_half_health_rolls_once_and_the_boundary_is_49_then_50() {
    let user = at_hp_percent(user_with(&[MoveId::TACKLE]), HP_AT_THRESHOLD);
    for roll in [0, BELOW_PENALTY_ROLL, HIGH_BYTE | BELOW_PENALTY_ROLL] {
        assert_eq!(viability(&user, &target(), &[roll]), UNCHANGED_SCORE);
    }
    for roll in [AT_PENALTY_ROLL, 255, HIGH_BYTE | AT_PENALTY_ROLL] {
        assert_eq!(viability(&user, &target(), &[roll]), ONE_PENALTY_SCORE);
    }
}

#[test]
fn a_target_above_half_health_draws_nothing_for_its_penalty() {
    let user = user_with(&[MoveId::TACKLE]);
    let target = at_hp_percent(target(), HP_ABOVE_THRESHOLD);
    assert_eq!(viability(&user, &target, &[]), UNCHANGED_SCORE);
}

#[test]
fn a_target_at_half_health_rolls_once_and_the_boundary_is_49_then_50() {
    let user = user_with(&[MoveId::TACKLE]);
    let target = at_hp_percent(target(), HP_AT_THRESHOLD);
    for roll in [0, BELOW_PENALTY_ROLL, HIGH_BYTE | BELOW_PENALTY_ROLL] {
        assert_eq!(viability(&user, &target, &[roll]), UNCHANGED_SCORE);
    }
    for roll in [AT_PENALTY_ROLL, 255, HIGH_BYTE | AT_PENALTY_ROLL] {
        assert_eq!(viability(&user, &target, &[roll]), ONE_PENALTY_SCORE);
    }
}

#[test]
fn a_user_below_half_health_and_a_target_below_it_are_penalised_separately() {
    let user = at_hp_percent(user_with(&[MoveId::TACKLE]), 1);
    let target = at_hp_percent(target(), 1);
    for (rolls, expected) in [
        ([BELOW_PENALTY_ROLL, BELOW_PENALTY_ROLL], UNCHANGED_SCORE),
        ([AT_PENALTY_ROLL, BELOW_PENALTY_ROLL], ONE_PENALTY_SCORE),
        ([BELOW_PENALTY_ROLL, AT_PENALTY_ROLL], ONE_PENALTY_SCORE),
        ([AT_PENALTY_ROLL, AT_PENALTY_ROLL], TWO_PENALTIES_SCORE),
    ] {
        assert_eq!(viability(&user, &target, &rolls), expected, "{rolls:?}");
    }
}

#[test]
fn the_user_penalty_roll_precedes_the_target_penalty_roll() {
    let user = at_hp_percent(user_with(&[MoveId::TACKLE]), HP_AT_THRESHOLD);
    let target = at_hp_percent(target(), HP_AT_THRESHOLD);
    assert_eq!(
        viability(&user, &target, &[AT_PENALTY_ROLL, BELOW_PENALTY_ROLL]),
        ONE_PENALTY_SCORE
    );
}

#[test]
fn a_known_protect_or_detect_rolls_once_for_the_bonus_and_the_boundary_is_59_then_60() {
    for move_id in [MoveId::PROTECT, MoveId::DETECT] {
        let user = user_with(&[MoveId::TACKLE, move_id]);
        for roll in [0, BELOW_BONUS_ROLL, HIGH_BYTE | BELOW_BONUS_ROLL] {
            assert_eq!(
                viability(&user, &target(), &[roll]),
                UNCHANGED_SCORE,
                "{move_id:?}"
            );
        }
        for roll in [AT_BONUS_ROLL, 255, HIGH_BYTE | AT_BONUS_ROLL] {
            assert_eq!(
                viability(&user, &target(), &[roll]),
                BONUS_SCORE,
                "{move_id:?}"
            );
        }
    }
}

#[test]
fn other_defensive_moves_do_not_earn_the_bonus() {
    for move_id in [
        MoveId::AMNESIA,
        MoveId::ENDURE,
        MoveId::HARDEN,
        MoveId::LIGHT_SCREEN,
        MoveId::SUBSTITUTE,
    ] {
        let user = user_with(&[MoveId::TACKLE, move_id]);
        assert_eq!(
            viability(&user, &target(), &[]),
            UNCHANGED_SCORE,
            "{move_id:?}"
        );
    }
}

#[test]
fn a_protect_on_the_target_does_not_earn_the_bonus() {
    let target = mon(&[MoveId::PROTECT, MoveId::DETECT]);
    assert_eq!(
        viability(&user_with(&[MoveId::TACKLE]), &target, &[]),
        UNCHANGED_SCORE
    );
}

#[test]
fn a_depleted_protect_slot_still_earns_the_bonus_roll() {
    let user = spend_move(user_with(&[MoveId::TACKLE, MoveId::PROTECT]), 2);
    assert_eq!(user.moves()[2].pp, 0);
    assert_eq!(viability(&user, &target(), &[AT_BONUS_ROLL]), BONUS_SCORE);
}

#[test]
fn only_the_two_effects_the_script_names_are_defensive() {
    assert!(is_leech_seed_defensive_effect(
        MoveEffect::SPECIAL_DEFENSE_UP
    ));
    assert!(is_leech_seed_defensive_effect(MoveEffect::PROTECT));
    for effect in [
        MoveEffect::SPECIAL_DEFENSE_UP_2,
        MoveEffect::ENDURE,
        MoveEffect::DEFENSE_UP,
        MoveEffect::HIT,
        MoveEffect::LEECH_SEED,
    ] {
        assert!(!is_leech_seed_defensive_effect(effect), "{effect:?}");
    }
}

#[test]
fn a_user_without_an_attacking_move_still_rolls_for_the_bonus() {
    let user = at_hp_percent(user_with(&[MoveId::PROTECT]), 1);
    let target = at_hp_percent(target(), 1);
    assert_eq!(
        viability(&user, &target, &[AT_BONUS_ROLL]),
        BONUS_SCORE,
        "no penalty roll precedes the bonus roll"
    );
}

#[test]
fn penalties_and_the_bonus_roll_in_order_and_combine() {
    let user = at_hp_percent(user_with(&[MoveId::TACKLE, MoveId::PROTECT]), 1);
    let target = at_hp_percent(target(), 1);
    assert_eq!(
        viability(
            &user,
            &target,
            &[AT_PENALTY_ROLL, AT_PENALTY_ROLL, AT_BONUS_ROLL]
        ),
        BOTH_PENALTIES_AND_BONUS_SCORE
    );
    assert_eq!(
        viability(
            &user,
            &target,
            &[AT_PENALTY_ROLL, BELOW_PENALTY_ROLL, AT_BONUS_ROLL]
        ),
        ONE_PENALTY_AND_BONUS_SCORE
    );
    assert_eq!(
        viability(
            &user,
            &target,
            &[BELOW_PENALTY_ROLL, AT_PENALTY_ROLL, BELOW_BONUS_ROLL]
        ),
        ONE_PENALTY_SCORE
    );
    assert_eq!(
        viability(
            &user,
            &target,
            &[AT_PENALTY_ROLL, BELOW_PENALTY_ROLL, BELOW_BONUS_ROLL]
        ),
        ONE_PENALTY_SCORE
    );
    assert_eq!(
        viability(
            &user,
            &target,
            &[BELOW_PENALTY_ROLL, BELOW_PENALTY_ROLL, AT_BONUS_ROLL]
        ),
        BONUS_SCORE
    );
}

#[test]
fn one_penalty_and_the_bonus_net_to_minus_one() {
    let user = at_hp_percent(
        user_with(&[MoveId::TACKLE, MoveId::PROTECT]),
        HP_AT_THRESHOLD,
    );
    assert_eq!(
        viability(&user, &target(), &[AT_PENALTY_ROLL, AT_BONUS_ROLL]),
        ONE_PENALTY_AND_BONUS_SCORE
    );
}

#[test]
fn viability_admits_leech_seed_but_still_refuses_first_turn_setup_and_toxic() {
    let dex = Dex::new();
    for flags in [
        AiFlags::CHECK_VIABILITY,
        AiFlags::CHECK_BAD_MOVE | AiFlags::CHECK_VIABILITY,
        AiFlags::CHECK_BAD_MOVE | AiFlags::TRY_TO_FAINT | AiFlags::CHECK_VIABILITY,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, MoveId::LEECH_SEED, flags),
            Ok(()),
            "{flags:?}"
        );
    }
    for flags in [
        AiFlags::SETUP_FIRST_TURN,
        AiFlags::CHECK_VIABILITY | AiFlags::SETUP_FIRST_TURN,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, MoveId::LEECH_SEED, flags),
            Err(BattleError::UnscoreableMoveEffect(MoveId::LEECH_SEED)),
            "{flags:?}"
        );
    }
    assert_eq!(
        ensure_scoreable(&dex, MoveId::TOXIC, AiFlags::CHECK_VIABILITY),
        Err(BattleError::UnscoreableMoveEffect(MoveId::TOXIC))
    );
}
