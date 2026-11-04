//! Secondary-effect regressions for [`resolve_hit`]: the Poison Sting
//! (`EFFECT_POISON_HIT`) and Psybeam (`EFFECT_CONFUSE_HIT`) chance rolls and
//! their guards, plus the Levitate-blocked Mud Shot case that must leave the
//! speed drop unapplied.

use super::test_fixtures::{
    mon, ACCURACY_HIT_DRAW, BEST_DAMAGE_DRAW, BULBASAUR, GASTLY, ORDINARY_NON_CRITICAL_DRAWS,
    ORDINARY_NO_CRIT_DRAW, POISON_STING, PSYBEAM, SHEDINJA, SQUIRTLE, TACKLE,
};
use super::{resolve_hit, HitOutcome};
use crate::dex::Dex;
use crate::script_rng::SequenceRng;
use crate::secondary::SecondaryApplication;
use assets::species::AbilityId;
use assets::{MoveId, SpeciesId};

/// `SPECIES_EKANS`: mono Poison-type.
const EKANS: SpeciesId = SpeciesId(23);

/// A draw that clears [`POISON_STING`]'s 30% secondary chance.
const POISON_CHANCE_HIT_DRAW: u16 = 29;

/// A draw that misses it.
const POISON_CHANCE_MISS_DRAW: u16 = 30;

/// `SPECIES_SPINDA`: Own Tempo in its only ability slot.
const SPINDA: SpeciesId = SpeciesId(308);

/// A draw that clears [`PSYBEAM`]'s 10% secondary chance.
const CONFUSE_CHANCE_HIT_DRAW: u16 = 9;

/// A draw that misses it.
const CONFUSE_CHANCE_MISS_DRAW: u16 = 10;

#[test]
fn a_landed_poison_sting_reports_poisons_defender_only_on_a_successful_roll() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 10, vec![POISON_STING]);
    let defender = mon(&dex, SQUIRTLE, 10, vec![TACKLE]);
    assert_eq!(
        dex.move_data(POISON_STING).unwrap().secondary_effect_chance,
        30
    );

    let mut succeeds = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        POISON_CHANCE_HIT_DRAW,
    ]);
    let succeeded = resolve_hit(
        &dex,
        POISON_STING,
        &attacker,
        &defender,
        false,
        &mut succeeds,
    )
    .unwrap();
    assert!(matches!(succeeded.outcome, HitOutcome::Hit { .. }));
    assert_eq!(succeeded.secondary_effect, SecondaryApplication::Poison);
    assert_eq!(succeeds.draws(), 4);

    let mut fails = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        POISON_CHANCE_MISS_DRAW,
    ]);
    let failed = resolve_hit(&dex, POISON_STING, &attacker, &defender, false, &mut fails).unwrap();
    assert!(matches!(failed.outcome, HitOutcome::Hit { .. }));
    assert_eq!(failed.secondary_effect, SecondaryApplication::None);
}

#[test]
fn a_poison_type_or_steel_type_defender_never_reports_poisons_defender() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 10, vec![POISON_STING]);
    let poison_type_defender = mon(&dex, EKANS, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        POISON_CHANCE_HIT_DRAW,
    ]);
    let resolution = resolve_hit(
        &dex,
        POISON_STING,
        &attacker,
        &poison_type_defender,
        false,
        &mut rng,
    )
    .unwrap();
    assert!(
        matches!(resolution.outcome, HitOutcome::Hit { .. }),
        "fixture sanity: Poison is only not-very-effective against Poison, not immune, \
         so the hit must land for this to test the status guard rather than a coincidental miss"
    );
    assert_eq!(resolution.secondary_effect, SecondaryApplication::None);
}

/// Poison Sting's secondary is modelled (unlike Water Gun's, which has no
/// trampoline at all and so cannot distinguish this suppression from a
/// coincidental `false`). Wonder Guard must suppress it even on a roll that
/// would otherwise succeed, matching `MOVE_RESULT_MISSED`'s membership in
/// `MOVE_RESULT_NO_EFFECT` (`include/constants/battle.h:220-228`).
#[test]
fn wonder_guard_suppresses_poison_stings_secondary_on_a_successful_chance_roll() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 10, vec![POISON_STING]);
    let shedinja_defender = mon(&dex, SHEDINJA, 10, vec![TACKLE]);
    assert_eq!(shedinja_defender.ability(), AbilityId::WONDER_GUARD);
    let mut rng = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        POISON_CHANCE_HIT_DRAW,
    ]);

    let resolution = resolve_hit(
        &dex,
        POISON_STING,
        &attacker,
        &shedinja_defender,
        false,
        &mut rng,
    )
    .unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::WonderGuardBlocked,
        "Poison is not-very-effective against Ghost and has no row against \
         Bug, so only Wonder Guard blocks this hit"
    );
    assert_eq!(
        resolution.secondary_effect,
        SecondaryApplication::None,
        "Wonder Guard must suppress the secondary effect even when the \
         chance roll would otherwise succeed"
    );
}

#[test]
fn a_landed_psybeam_reports_confuse_only_on_a_successful_roll() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 10, vec![PSYBEAM]);
    let defender = mon(&dex, SQUIRTLE, 10, vec![TACKLE]);
    assert_eq!(dex.move_data(PSYBEAM).unwrap().secondary_effect_chance, 10);

    let mut succeeds = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        CONFUSE_CHANCE_HIT_DRAW,
    ]);
    let succeeded = resolve_hit(&dex, PSYBEAM, &attacker, &defender, false, &mut succeeds).unwrap();
    assert!(matches!(succeeded.outcome, HitOutcome::Hit { .. }));
    assert_eq!(succeeded.secondary_effect, SecondaryApplication::Confuse);
    assert_eq!(
        succeeds.draws(),
        4,
        "the duration draw is not part of resolve_hit; the caller draws it \
         only once the target's post-damage faint check clears"
    );

    let mut fails = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        CONFUSE_CHANCE_MISS_DRAW,
    ]);
    let failed = resolve_hit(&dex, PSYBEAM, &attacker, &defender, false, &mut fails).unwrap();
    assert!(matches!(failed.outcome, HitOutcome::Hit { .. }));
    assert_eq!(failed.secondary_effect, SecondaryApplication::None);
}

#[test]
fn an_own_tempo_defender_never_reports_confuse() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 10, vec![PSYBEAM]);
    let defender = mon(&dex, SPINDA, 10, vec![TACKLE]);
    assert_eq!(defender.ability(), AbilityId::OWN_TEMPO);
    let mut rng = SequenceRng::new([
        ACCURACY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        CONFUSE_CHANCE_HIT_DRAW,
    ]);
    let resolution = resolve_hit(&dex, PSYBEAM, &attacker, &defender, false, &mut rng).unwrap();
    assert!(matches!(resolution.outcome, HitOutcome::Hit { .. }));
    assert_eq!(resolution.secondary_effect, SecondaryApplication::None);
    assert_eq!(rng.draws(), 4, "the chance draw still happens, discarded");
}

#[test]
fn levitate_blocked_mud_shot_applies_no_speed_drop() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 20, vec![MoveId::MUD_SHOT]);
    let levitate_defender = mon(&dex, GASTLY, 20, vec![TACKLE]);
    assert_eq!(levitate_defender.ability(), AbilityId::LEVITATE);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(
        &dex,
        MoveId::MUD_SHOT,
        &attacker,
        &levitate_defender,
        false,
        &mut rng,
    )
    .unwrap();

    assert_eq!(resolution.outcome, HitOutcome::LevitateBlocked);
    assert_eq!(rng.draws(), ORDINARY_NON_CRITICAL_DRAWS.len());
    assert_eq!(resolution.secondary_effect, SecondaryApplication::None);
}
