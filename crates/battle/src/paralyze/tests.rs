use super::{
    ensure_admissible, ensure_resolvable, is_paralyze_effect, resolve_paralyze_move,
    ParalyzeOutcome, EFFECT_PARALYZE,
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

/// `SPECIES_PERSIAN`: Normal, and Limber in its only ability slot.
const PERSIAN: SpeciesId = SpeciesId(53);

#[test]
fn a_limber_defender_is_protected_before_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, PERSIAN, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), assets::species::AbilityId::LIMBER);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_ne!(
        outcome,
        ParalyzeOutcome::Applied,
        "jumpifability BS_TARGET, ABILITY_LIMBER exits the script (data/battle_scripts_1.s:1011)"
    );
    assert_eq!(
        rng.draws(),
        0,
        "the Limber guard precedes typecalc and accuracycheck"
    );
}

#[test]
fn a_limber_defender_reports_the_limber_protected_outcome() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, PERSIAN, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::LimberProtected,
        "BattleScript_LimberProtected (data/battle_scripts_1.s:1034-1038) is its own exit, \
         distinct from the type-immunity exit"
    );
}

/// `SPECIES_RALTS`: Psychic, and Synchronize in its primary ability slot.
const RALTS: SpeciesId = SpeciesId(392);

#[test]
fn a_synchronize_defender_is_refused_before_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, RALTS, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), assets::AbilityId::SYNCHRONIZE);
    let mut rng = SequenceRng::new([0]);
    let refused =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap_err();
    assert_eq!(
        refused,
        BattleError::UnportedAbilityInteraction(assets::AbilityId::SYNCHRONIZE),
        "the unmodelled MOVEEND_SYNCHRONIZE_TARGET reflection fails closed"
    );
    assert_eq!(rng.draws(), 0, "the refusal precedes accuracycheck");
}

#[test]
fn an_already_paralysed_synchronize_defender_is_admitted_not_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let mut defender = mon(&dex, RALTS, 10, vec![TACKLE]);
    defender.set_status1(Status1::Paralysed);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::AlreadyParalysed,
        "jumpifstatus BS_TARGET, STATUS1_PARALYSIS exits before seteffectprimary \
         (data/battle_scripts_1.s:1015), so Synchronize never arms"
    );
    assert_eq!(rng.draws(), 0, "the exit precedes accuracycheck");
}

/// Why [`ensure_admissible`]'s type-immunity arm carries no fixture: no
/// Synchronize holder in the species table is immune to a paralyze move, so
/// the guard that would admit one is unreachable through real data.
#[test]
fn no_synchronize_holder_is_type_immune_to_a_paralyze_move() {
    use assets::{Effectiveness, SpeciesTable};

    let dex = Dex::new();
    let table = SpeciesTable::new();
    let mut holders = 0;
    for raw in 0..u16::try_from(SpeciesTable::LEN).unwrap() {
        let Some(info) = table.get(SpeciesId(raw)) else {
            continue;
        };
        if !info.abilities.contains(&assets::AbilityId::SYNCHRONIZE) {
            continue;
        }
        holders += 1;
        for move_id in [THUNDER_WAVE, STUN_SPORE, GLARE] {
            let move_type = dex
                .move_data(move_id)
                .unwrap()
                .move_type
                .battle_type()
                .unwrap();
            assert!(
                info.types
                    .iter()
                    .all(|&t| dex.effectiveness(move_type, t) != Effectiveness::NoEffect),
                "species {raw} fields Synchronize and resists move {move_id:?} outright"
            );
        }
    }
    assert!(
        holders > 0,
        "the scan must actually find Synchronize holders"
    );
}

#[test]
fn a_paralysed_attacker_is_admitted_against_a_synchronize_defender() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    attacker.set_status1(Status1::Paralysed);
    let defender = mon(&dex, RALTS, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::Applied,
        "the reflection's SetMoveEffect pass writes nothing to an already-statused \
         attacker (src/battle_script_commands.c:2422-2423), so nothing is unmodelled"
    );
    assert_eq!(rng.draws(), 1, "only accuracycheck draws");
}

/// `SPECIES_SEVIPER`: Poison, and Shed Skin in its primary ability slot.
const SEVIPER: SpeciesId = SpeciesId(379);

#[test]
fn a_shed_skin_defender_is_refused_before_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, SEVIPER, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), assets::AbilityId::SHED_SKIN);
    let mut rng = SequenceRng::new([0]);
    let refused =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap_err();
    assert_eq!(
        refused,
        BattleError::UnportedAbilityInteraction(assets::AbilityId::SHED_SKIN),
        "the unmodelled end-turn cure roll fails closed"
    );
    assert_eq!(rng.draws(), 0, "the refusal precedes accuracycheck");
}

#[test]
fn an_already_paralysed_shed_skin_defender_is_admitted_not_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let mut defender = mon(&dex, SEVIPER, 10, vec![TACKLE]);
    defender.set_status1(Status1::Paralysed);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::AlreadyParalysed,
        "this move applies nothing, so it is not what made Shed Skin reachable"
    );
}

/// `SPECIES_MACHOP`: Fighting, and Guts in its primary ability slot.
const MACHOP: SpeciesId = SpeciesId(66);
/// `SPECIES_MILOTIC`: Water, and Marvel Scale in its primary ability slot.
const MILOTIC: SpeciesId = SpeciesId(329);

#[test]
fn a_guts_defender_is_refused_before_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, MACHOP, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), assets::AbilityId::GUTS);
    let mut rng = SequenceRng::new([0]);
    let refused =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap_err();
    assert_eq!(
        refused,
        BattleError::UnportedAbilityInteraction(assets::AbilityId::GUTS),
        "CalculateBaseDamage raises a statused Guts holder's physical Attack, which \
         BattlePokemon::attacking_stat does not model"
    );
    assert_eq!(rng.draws(), 0, "the refusal precedes accuracycheck");
}

#[test]
fn a_marvel_scale_defender_is_refused_before_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, MILOTIC, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), assets::AbilityId::MARVEL_SCALE);
    let mut rng = SequenceRng::new([0]);
    let refused =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap_err();
    assert_eq!(
        refused,
        BattleError::UnportedAbilityInteraction(assets::AbilityId::MARVEL_SCALE),
        "CalculateBaseDamage raises a statused Marvel Scale holder's Defense, which \
         BattlePokemon::defending_stat does not model"
    );
    assert_eq!(rng.draws(), 0, "the refusal precedes accuracycheck");
}

/// The stat-reading pair is refused for the damage every later turn would
/// miscompute, not for a draw, so the refusal must not depend on which
/// paralyzing move carries it.
#[test]
fn every_paralyze_move_refuses_the_stat_reading_abilities() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE, STUN_SPORE, GLARE]);
    for (species, ability) in [
        (MACHOP, assets::AbilityId::GUTS),
        (MILOTIC, assets::AbilityId::MARVEL_SCALE),
    ] {
        let defender = mon(&dex, species, 10, vec![TACKLE]);
        for move_id in [THUNDER_WAVE, STUN_SPORE, GLARE] {
            assert_eq!(
                ensure_admissible(&dex, move_id, &attacker, &defender),
                Err(BattleError::UnportedAbilityInteraction(ability)),
                "{move_id:?} against {species:?}"
            );
        }
    }
}
