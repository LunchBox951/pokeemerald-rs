use super::{
    ensure_admissible, ensure_resolvable, is_paralyze_effect, resolve_paralyze_move,
    resolve_synchronize_reflection, ParalyzeOutcome, SynchronizeReflectionOutcome, EFFECT_PARALYZE,
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

#[test]
fn a_poisoned_defender_reports_already_statused_without_overwriting_or_drawing() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let mut defender = mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    defender.set_status1(Status1::Poisoned);
    let mut rng = SequenceRng::new([]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::AlreadyStatused,
        "jumpifstatus BS_TARGET, STATUS1_ANY (data/battle_scripts_1.s:1016) catches any \
         primary status besides the exact-match STATUS1_PARALYSIS jump at :1015"
    );
    assert_eq!(
        rng.draws(),
        0,
        "the STATUS1_ANY guard precedes accuracycheck just like the exact-match one"
    );
    assert_eq!(
        defender.status1(),
        Status1::Poisoned,
        "resolve_paralyze_move reports the outcome; it never mutates either battler"
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
fn a_healthy_synchronize_defender_is_admitted_and_paralysed() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, RALTS, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), assets::AbilityId::SYNCHRONIZE);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::Applied,
        "ensure_admissible no longer refuses Synchronize; the caller in \
         crate::battle::execute reflects the status at move end"
    );
    assert_eq!(
        rng.draws(),
        1,
        "reflection is not resolved here and draws nothing itself"
    );
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
        "ensure_admissible no longer reads the attacker's status at all; the \
         reflection itself is resolved separately by resolve_synchronize_reflection"
    );
    assert_eq!(rng.draws(), 1, "only accuracycheck draws");
}

#[test]
fn a_poisoned_attacker_is_admitted_against_a_synchronize_defender() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    attacker.set_status1(Status1::Poisoned);
    let defender = mon(&dex, RALTS, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::Applied,
        "an attacker carrying any primary status does not change whether the \
         defender is paralysed; only resolve_synchronize_reflection reads it"
    );
    assert_eq!(rng.draws(), 1, "only accuracycheck draws");
    assert_eq!(
        attacker.status1(),
        Status1::Poisoned,
        "resolve_paralyze_move never mutates the attacker"
    );
}

#[test]
fn resolve_synchronize_reflection_applies_to_a_healthy_attacker() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    assert_eq!(
        resolve_synchronize_reflection(&attacker),
        SynchronizeReflectionOutcome::Applied
    );
}

#[test]
fn resolve_synchronize_reflection_reports_an_existing_status_without_changing_it() {
    let dex = Dex::new();
    for status in [Status1::Paralysed, Status1::Poisoned] {
        let mut attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
        attacker.set_status1(status);
        assert_eq!(
            resolve_synchronize_reflection(&attacker),
            SynchronizeReflectionOutcome::AlreadyStatused,
            "{status:?}"
        );
        assert_eq!(
            attacker.status1(),
            status,
            "resolve_synchronize_reflection never mutates its argument"
        );
    }
}

#[test]
fn resolve_synchronize_reflection_is_blocked_by_the_attackers_own_limber() {
    let dex = Dex::new();
    let attacker = mon(&dex, PERSIAN, 10, vec![THUNDER_WAVE]);
    assert_eq!(attacker.ability(), assets::species::AbilityId::LIMBER);
    assert_eq!(
        resolve_synchronize_reflection(&attacker),
        SynchronizeReflectionOutcome::LimberProtected
    );
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

#[test]
fn an_already_poisoned_shed_skin_defender_is_admitted_not_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let mut defender = mon(&dex, SEVIPER, 10, vec![TACKLE]);
    defender.set_status1(Status1::Poisoned);
    let mut rng = SequenceRng::new([]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::AlreadyStatused,
        "the STATUS1_ANY guard exits before ensure_admissible ever reads Shed Skin"
    );
}

/// `SPECIES_MACHOP`: Fighting, and Guts in its primary ability slot.
const MACHOP: SpeciesId = SpeciesId(66);
/// `SPECIES_MILOTIC`: Water, and Marvel Scale in its primary ability slot.
const MILOTIC: SpeciesId = SpeciesId(329);

#[test]
fn a_guts_defender_is_newly_paralysed_not_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, MACHOP, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), assets::AbilityId::GUTS);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::Applied,
        "Guts is modelled by BattlePokemon::attacking_stat, so admission never refuses it"
    );
    assert_eq!(rng.draws(), 1, "only accuracycheck draws");
}

#[test]
fn a_marvel_scale_defender_is_newly_paralysed_not_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE]);
    let defender = mon(&dex, MILOTIC, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), assets::AbilityId::MARVEL_SCALE);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_paralyze_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        ParalyzeOutcome::Applied,
        "Marvel Scale is modelled by BattlePokemon::defending_stat, so admission never refuses it"
    );
    assert_eq!(rng.draws(), 1, "only accuracycheck draws");
}

/// The stat-reading pair is admitted for every paralyzing move, not just one,
/// since the interaction is modelled generically at the accessor boundary.
#[test]
fn every_paralyze_move_admits_the_stat_reading_abilities() {
    let dex = Dex::new();
    let attacker = mon(&dex, WURMPLE, 10, vec![THUNDER_WAVE, STUN_SPORE, GLARE]);
    for species in [MACHOP, MILOTIC] {
        let defender = mon(&dex, species, 10, vec![TACKLE]);
        for move_id in [THUNDER_WAVE, STUN_SPORE, GLARE] {
            assert_eq!(
                ensure_admissible(&dex, move_id, &attacker, &defender),
                Ok(()),
                "{move_id:?} against {species:?}"
            );
        }
    }
}
