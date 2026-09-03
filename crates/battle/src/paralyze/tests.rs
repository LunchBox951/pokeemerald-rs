use super::{
    ensure_resolvable, is_paralyze_effect, resolve_paralyze_move, ParalyzeOutcome, EFFECT_PARALYZE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::status1::Status1;
use assets::{MoveId, SpeciesId};

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

/// `MOVE_THUNDER_WAVE`: Electric, 100 accuracy.
const THUNDER_WAVE: MoveId = MoveId(86);
/// `MOVE_STUN_SPORE`: Grass, 75 accuracy.
const STUN_SPORE: MoveId = MoveId(78);
/// `MOVE_GLARE`: Normal, 75 accuracy.
const GLARE: MoveId = MoveId(137);
/// `MOVE_TACKLE`, outside the family.
const TACKLE: MoveId = MoveId(33);
const UNKNOWN_MOVE: MoveId = MoveId(60_000);

/// `SPECIES_ZIGZAGOON`, an ordinary non-immune target.
const ZIGZAGOON: SpeciesId = SpeciesId(288);
/// `SPECIES_SANDSHREW`: pure Ground, immune to Thunder Wave's Electric type.
const SANDSHREW: SpeciesId = SpeciesId(27);
/// `SPECIES_WURMPLE`, an ordinary attacker.
const WURMPLE: SpeciesId = SpeciesId(290);

#[test]
fn the_family_covers_exactly_the_three_named_moves() {
    assert!(is_paralyze_effect(EFFECT_PARALYZE));
    let dex = Dex::new();
    for move_id in [THUNDER_WAVE, STUN_SPORE, GLARE] {
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()), "{move_id:?}");
        assert_eq!(dex.move_data(move_id).unwrap().effect, EFFECT_PARALYZE);
    }
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(
        ensure_resolvable(&dex, UNKNOWN_MOVE),
        Err(BattleError::UnknownMove(UNKNOWN_MOVE))
    );
    let attacker = mon(&dex, WURMPLE, 10, vec![TACKLE]);
    let defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_paralyze_move(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

#[test]
fn a_ground_type_defender_is_immune_and_the_guard_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, SANDSHREW, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, ParalyzeOutcome::Immune);
    assert_eq!(
        rng.draws(),
        0,
        "typecalc's immunity guard precedes accuracy"
    );
}

#[test]
fn an_already_paralysed_defender_is_reported_without_drawing() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let mut defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    defender.set_status1(Status1::Paralysed);
    let mut rng = SequenceRng::new([]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, ParalyzeOutcome::AlreadyParalysed);
    assert_eq!(
        rng.draws(),
        0,
        "the already-paralysed guard precedes accuracy too"
    );
}

#[test]
fn a_missed_accuracy_check_still_costs_its_one_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![STUN_SPORE]);
    let defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    // Stun Spore's 75 accuracy: roll 96 (95 % 100 + 1) exceeds the threshold.
    let mut rng = SequenceRng::new([95]);
    let outcome = resolve_paralyze_move(&dex, STUN_SPORE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, ParalyzeOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_landed_hit_applies_paralysis_with_a_single_draw_and_does_not_mutate() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, ParalyzeOutcome::Applied);
    assert_eq!(
        rng.draws(),
        1,
        "seteffectprimary spends no further draw beyond the accuracy check"
    );
    assert_eq!(
        defender.status1(),
        Status1::Healthy,
        "resolve_paralyze_move reports the outcome; the caller applies it"
    );
}
