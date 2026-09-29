use super::{
    draw_confusion_duration, ensure_resolvable, is_confuse_effect, resolve_confuse_move,
    ConfuseOutcome, EFFECT_CONFUSE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use assets::{AbilityId, MoveId, SpeciesId};

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

/// `MOVE_SUPERSONIC`: Normal, 55 accuracy.
const SUPERSONIC: MoveId = MoveId(48);
/// `MOVE_CONFUSE_RAY`: Ghost, 100 accuracy.
const CONFUSE_RAY: MoveId = MoveId(109);
/// `MOVE_SWEET_KISS`: Normal, 75 accuracy.
const SWEET_KISS: MoveId = MoveId(186);
/// `MOVE_TACKLE`, outside the family.
const TACKLE: MoveId = MoveId(33);
const UNKNOWN_MOVE: MoveId = MoveId(60_000);

/// `SPECIES_ZIGZAGOON`, an ordinary non-immune target.
const ZIGZAGOON: SpeciesId = SpeciesId(288);
/// `SPECIES_WURMPLE`, an ordinary attacker.
const WURMPLE: SpeciesId = SpeciesId(290);
/// `SPECIES_SPINDA`: Own Tempo in its only ability slot.
const SPINDA: SpeciesId = SpeciesId(308);

#[test]
fn the_family_covers_exactly_the_three_named_moves() {
    assert!(is_confuse_effect(EFFECT_CONFUSE));
    let dex = Dex::new();
    for move_id in [SUPERSONIC, CONFUSE_RAY, SWEET_KISS] {
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()), "{move_id:?}");
        assert_eq!(dex.move_data(move_id).unwrap().effect, EFFECT_CONFUSE);
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
        resolve_confuse_move(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

#[test]
fn an_own_tempo_defender_is_protected_before_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![CONFUSE_RAY]);
    let defender = mon(&dex, SPINDA, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), AbilityId::OWN_TEMPO);
    let mut rng = SequenceRng::new([]);
    let outcome = resolve_confuse_move(&dex, CONFUSE_RAY, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, ConfuseOutcome::OwnTempoProtected);
    assert_eq!(rng.draws(), 0, "Own Tempo precedes accuracycheck");
}

#[test]
fn an_own_tempo_defender_already_carrying_confusion_still_reports_own_tempo_first() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![CONFUSE_RAY]);
    let mut defender = mon(&dex, SPINDA, 10, vec![TACKLE]);
    defender.volatiles_mut().set_confusion(3);
    let mut rng = SequenceRng::new([]);
    let outcome = resolve_confuse_move(&dex, CONFUSE_RAY, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ConfuseOutcome::OwnTempoProtected,
        "jumpifability BS_TARGET, ABILITY_OWN_TEMPO precedes the already-confused \
         jump (data/battle_scripts_1.s:907-909)"
    );
}

#[test]
fn an_already_confused_defender_is_reported_without_drawing() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![CONFUSE_RAY]);
    let mut defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    defender.volatiles_mut().set_confusion(1);
    let mut rng = SequenceRng::new([]);
    let outcome = resolve_confuse_move(&dex, CONFUSE_RAY, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, ConfuseOutcome::AlreadyConfused);
    assert_eq!(
        rng.draws(),
        0,
        "the already-confused guard precedes accuracy too"
    );
    assert_eq!(
        defender.volatiles().confusion_turns,
        1,
        "resolve_confuse_move never mutates the defender"
    );
}

#[test]
fn a_missed_accuracy_check_still_costs_its_one_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![SWEET_KISS]);
    let defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    // Sweet Kiss's 75 accuracy: roll 96 (95 % 100 + 1) exceeds the threshold.
    let mut rng = SequenceRng::new([95]);
    let outcome = resolve_confuse_move(&dex, SWEET_KISS, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, ConfuseOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_landed_hit_draws_exactly_one_more_bit_for_the_duration() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![CONFUSE_RAY]);
    let defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0]);
    let outcome = resolve_confuse_move(&dex, CONFUSE_RAY, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, ConfuseOutcome::Applied { turns: 2 });
    assert_eq!(
        rng.draws(),
        2,
        "one accuracy draw, then exactly one duration draw"
    );
    assert_eq!(
        defender.volatiles().confusion_turns,
        0,
        "resolve_confuse_move reports the outcome; the caller applies it"
    );
}

#[test]
fn the_duration_draw_covers_the_full_two_to_five_range() {
    for (draw, expected) in [(0, 2), (1, 3), (2, 4), (3, 5), (4, 2), (65535, 5)] {
        let mut rng = SequenceRng::new([draw]);
        assert_eq!(draw_confusion_duration(&mut rng), expected, "draw {draw}");
    }
}

#[test]
fn a_poisoned_or_paralysed_target_is_still_eligible() {
    use crate::status1::Status1;

    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![CONFUSE_RAY]);
    for status in [Status1::Poisoned, Status1::Paralysed] {
        let mut defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
        defender.set_status1(status);
        let mut rng = SequenceRng::new([0, 0]);
        let outcome =
            resolve_confuse_move(&dex, CONFUSE_RAY, &attacker, &defender, &mut rng).unwrap();
        assert_eq!(
            outcome,
            ConfuseOutcome::Applied { turns: 2 },
            "confusion is a status2 volatile, independent of status1: {status:?}"
        );
    }
}
