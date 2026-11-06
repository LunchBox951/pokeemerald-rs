//! `AI_CBM_LeechSeed` (`data/battle_ai_scripts.s:410`-`:416`) scoring.

use super::{
    ensure_scoreable, leech_seed_penalty, score_bad_move, MoveScores, STRONGLY_DISCOURAGE,
};
use crate::battle::BattlerSide;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::{MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNPENALIZED_SCORE: i8 = 100;
const SCORE_AFTER_ONE_PENALTY: i8 = 90;

const TREECKO: SpeciesId = SpeciesId(277);
const MUDKIP: SpeciesId = SpeciesId(283);
/// Grass in its second type slot.
const LOTAD: SpeciesId = SpeciesId(295);

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

fn seeded(mut pokemon: BattlePokemon) -> BattlePokemon {
    pokemon.volatiles_mut().set_seeded_by(BattlerSide::Enemy);
    pokemon
}

fn score(user: &BattlePokemon, target: &BattlePokemon) -> i8 {
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES]);
    let mut scores = MoveScores::initialize(user, &mut rng);
    score_bad_move(
        &Dex::new(),
        &mut scores,
        0,
        MoveId::LEECH_SEED,
        user,
        target,
    )
    .unwrap();
    assert_eq!(
        rng.draws(),
        MAX_MON_MOVES,
        "scoring Leech Seed must draw nothing beyond the simulated-damage initialisation"
    );
    scores.values[0]
}

#[test]
fn a_seedable_target_keeps_the_initial_score() {
    let user = mon(MUDKIP, &[MoveId::LEECH_SEED]);
    assert_eq!(
        score(&user, &mon(MUDKIP, &[MoveId::TACKLE])),
        UNPENALIZED_SCORE
    );
}

#[test]
fn a_grass_type_in_the_first_slot_scores_minus_ten() {
    let user = mon(MUDKIP, &[MoveId::LEECH_SEED]);
    assert_eq!(
        score(&user, &mon(TREECKO, &[MoveId::POUND])),
        SCORE_AFTER_ONE_PENALTY
    );
}

/// Every Hoenn species with Grass second has two abilities, which the
/// scored-opponent guess refuses, so the second slot is checked on the
/// penalty itself.
#[test]
fn a_grass_type_in_the_second_slot_is_penalised() {
    assert_eq!(
        leech_seed_penalty(&mon(LOTAD, &[MoveId::POUND])),
        Some(STRONGLY_DISCOURAGE)
    );
    assert_eq!(leech_seed_penalty(&mon(MUDKIP, &[MoveId::POUND])), None);
}

#[test]
fn an_already_seeded_target_scores_minus_ten() {
    let user = mon(MUDKIP, &[MoveId::LEECH_SEED]);
    assert_eq!(
        score(&user, &seeded(mon(MUDKIP, &[MoveId::TACKLE]))),
        SCORE_AFTER_ONE_PENALTY
    );
}

#[test]
fn a_seeded_grass_target_takes_the_penalty_once() {
    let user = mon(MUDKIP, &[MoveId::LEECH_SEED]);
    assert_eq!(
        score(&user, &seeded(mon(TREECKO, &[MoveId::POUND]))),
        SCORE_AFTER_ONE_PENALTY,
        "the script ends at its first penalty"
    );
}

#[test]
fn only_check_bad_move_has_a_ported_leech_seed_branch() {
    let dex = Dex::new();
    assert_eq!(
        ensure_scoreable(&dex, MoveId::LEECH_SEED, AiFlags::CHECK_BAD_MOVE),
        Ok(())
    );
    for flags in [
        AiFlags::CHECK_BAD_MOVE | AiFlags::CHECK_VIABILITY,
        AiFlags::CHECK_BAD_MOVE | AiFlags::SETUP_FIRST_TURN,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, MoveId::LEECH_SEED, flags),
            Err(BattleError::UnscoreableMoveEffect(MoveId::LEECH_SEED)),
            "{flags:?}"
        );
    }
}
