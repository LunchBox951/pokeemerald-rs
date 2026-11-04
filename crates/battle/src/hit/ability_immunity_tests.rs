//! Ability-driven immunity regressions for the hit pipeline: Levitate,
//! Wonder Guard and Flash Fire (absorb and latched boost), and how
//! [`classify_accuracy_failure`] reports a missed roll against each of them.

use super::test_fixtures::{
    mon, ALWAYS_HIT_NON_CRITICAL_DRAWS, BEST_DAMAGE_DRAW, BULBASAUR, GASTLY, GROWL,
    ORDINARY_NON_CRITICAL_DRAWS, ORDINARY_NO_CRIT_DRAW, RATTATA, SHEDINJA, SLAM, SQUIRTLE,
    STRUGGLE_NON_CRITICAL_DRAWS, TACKLE, TACKLE_MISS_DRAW, UNKNOWN_MOVE, WATER_GUN,
};
use super::{classify_accuracy_failure, damage_before_roll, damage_core, resolve_hit, HitOutcome};
use crate::damage::{DamageInput, MoveCategory, Weather, STRUGGLE};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::script_rng::SequenceRng;
use crate::secondary::SecondaryApplication;
use assets::species::AbilityId;
use assets::{MoveId, SpeciesId, Type};

/// `MOVE_BONE_RUSH` -- `EFFECT_MULTI_HIT`, Ground. Used below only for its
/// typing: no admitted `is_ordinary_hit_effect` move is Ground-type, so
/// [`damage_core`] is exercised directly instead of through [`resolve_hit`],
/// which would reject Bone Rush's effect at [`ensure_resolvable`].
const BONE_RUSH: MoveId = MoveId::BONE_RUSH;

const FAINT_ATTACK: MoveId = MoveId::FAINT_ATTACK;

/// Slam's plain 75 threshold misses roll 75 for an attacker with no accuracy
/// modifier.
const SLAM_MISS_DRAW: u16 = 75;

/// `SPECIES_VULPIX`: Flash Fire in its primary (and only) ability slot.
const VULPIX: SpeciesId = SpeciesId(37);

/// `MOVE_EMBER` -- `EFFECT_BURN_HIT`, so `resolve_hit` would refuse it at
/// admission; the damage entry accepts its move data directly.
const EMBER: MoveId = MoveId::EMBER;

/// `Cmd_typecalc`'s Levitate branch (`battle_script_commands.c:1375-1383`).
/// Pins the ordinary-pipeline half of `damage_before_roll`'s shared boundary
/// directly via `damage_core`, since no admitted ordinary-hit move is
/// Ground-type to exercise it through `resolve_hit` end to end (see the
/// multi-hit turn-level regression for that half via Bone Rush in
/// `crates/battle/tests/turn_engine/move_resolution.rs`).
#[test]
fn levitate_blocks_a_ground_move_before_stab_and_type_effectiveness() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 20, vec![BONE_RUSH]);
    let levitate_defender = mon(&dex, GASTLY, 20, vec![TACKLE]);
    let mut rng = SequenceRng::new([ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW]);

    let outcome = damage_core(
        &dex,
        BONE_RUSH,
        &attacker,
        &levitate_defender,
        false,
        &mut rng,
    )
    .unwrap();

    assert_eq!(outcome, HitOutcome::LevitateBlocked);
    assert_eq!(
        rng.draws(),
        2,
        "a Levitate block still spends the critical and damage-variance \
         draws, exactly like an ordinary type immunity"
    );
}

/// `Cmd_typecalc`'s Wonder Guard branch (`battle_script_commands.c:1409-1418`)
/// blocks any powered, not-strictly-super-effective hit, distinctly from a
/// [`HitOutcome::NoEffect`] typing immunity.
#[test]
fn wonder_guard_blocks_a_neutral_hit_and_still_draws_normally() {
    let dex = Dex::new();
    let attacker = mon(&dex, RATTATA, 10, vec![WATER_GUN]);
    let shedinja_defender = mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    assert_eq!(shedinja_defender.ability(), AbilityId::WONDER_GUARD);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(
        &dex,
        WATER_GUN,
        &attacker,
        &shedinja_defender,
        false,
        &mut rng,
    )
    .unwrap();

    assert_eq!(resolution.outcome, HitOutcome::WonderGuardBlocked);
    assert_eq!(
        resolution.secondary_effect,
        SecondaryApplication::None,
        "a Wonder Guard block must not permit a secondary effect"
    );
    assert_eq!(
        rng.draws(),
        ORDINARY_NON_CRITICAL_DRAWS.len(),
        "a Wonder Guard block still spends the same draws as an ordinary hit"
    );
}

/// Wonder Guard's own message (`B_MSG_AVOIDED_DMG`, index 3) outranks the
/// ordinary typing-immunity message (`B_MSG_AVOIDED_ATK`, index 2) in
/// `Cmd_resultmessage` (`battle_script_commands.c:2048-2059`), so a move that
/// is independently type-immune must still report the Wonder Guard outcome.
#[test]
fn wonder_guard_outranks_an_independent_typing_immunity() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 20, vec![TACKLE]);
    let shedinja_defender = mon(&dex, SHEDINJA, 20, vec![WATER_GUN]);
    let mut rng = SequenceRng::new([ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW]);

    let outcome =
        damage_core(&dex, TACKLE, &attacker, &shedinja_defender, false, &mut rng).unwrap();

    assert_eq!(outcome, HitOutcome::WonderGuardBlocked);
}

/// Wonder Guard permits a strictly super-effective hit, so the block above
/// is a targeted admission check rather than a general immunity.
#[test]
fn wonder_guard_permits_a_super_effective_hit() {
    let dex = Dex::new();
    let attacker = mon(&dex, RATTATA, 10, vec![FAINT_ATTACK]);
    let shedinja_defender = mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(ALWAYS_HIT_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(
        &dex,
        FAINT_ATTACK,
        &attacker,
        &shedinja_defender,
        false,
        &mut rng,
    )
    .unwrap();

    assert!(matches!(resolution.outcome, HitOutcome::Hit { .. }));
}

/// Struggle bypasses `Cmd_typecalc` entirely (`battle_script_commands.c:1360-1364`),
/// so it must reach a Wonder Guard holder exactly like any other defender.
#[test]
fn wonder_guard_never_blocks_struggle() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![STRUGGLE]);
    let shedinja_defender = mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(STRUGGLE_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(
        &dex,
        STRUGGLE,
        &attacker,
        &shedinja_defender,
        false,
        &mut rng,
    )
    .unwrap();

    assert!(matches!(resolution.outcome, HitOutcome::Hit { .. }));
}

/// `Cmd_accuracycheck` does not stop a failed roll at the generic miss: it
/// calls `CheckWonderGuardAndLevitate` (`battle_script_commands.c:1175-1186`),
/// whose type scan flags `MOVE_RESULT_DOESNT_AFFECT_FOE` (`:1454-1469`) so
/// `Cmd_resultmessage` prints `STRINGID_ITDOESNTAFFECT` instead of
/// `STRINGID_ATTACKMISSED` (`:2055-2059`).
#[test]
fn a_failed_accuracy_roll_against_an_immune_target_reports_the_immunity() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 20, vec![TACKLE]);
    let ghost_defender = mon(&dex, GASTLY, 20, vec![TACKLE]);
    let mut rng = SequenceRng::new([TACKLE_MISS_DRAW]);

    let resolution =
        resolve_hit(&dex, TACKLE, &attacker, &ghost_defender, false, &mut rng).unwrap();

    assert_eq!(resolution.outcome, HitOutcome::NoEffect);
    assert_eq!(resolution.secondary_effect, SecondaryApplication::None);
    assert_eq!(
        rng.draws(),
        1,
        "reclassifying the failed roll must not spend a critical, damage, or \
         effect-chance draw"
    );
}

/// The same helper lets Wonder Guard overwrite `MISS_TYPE` with
/// `B_MSG_AVOIDED_DMG` (`battle_script_commands.c:1490-1497`), which
/// `Cmd_resultmessage` prefers over the typing-immunity message
/// (`:2055-2059`). Slam is Normal, so Shedinja's Ghost half makes it
/// independently immune as well: Wonder Guard still wins.
#[test]
fn a_failed_accuracy_roll_against_wonder_guard_reports_wonder_guard() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 20, vec![SLAM]);
    let shedinja_defender = mon(&dex, SHEDINJA, 20, vec![TACKLE]);
    assert_eq!(shedinja_defender.ability(), AbilityId::WONDER_GUARD);
    let mut rng = SequenceRng::new([SLAM_MISS_DRAW]);

    let resolution =
        resolve_hit(&dex, SLAM, &attacker, &shedinja_defender, false, &mut rng).unwrap();

    assert_eq!(resolution.outcome, HitOutcome::WonderGuardBlocked);
    assert_eq!(
        rng.draws(),
        1,
        "reclassifying the failed roll must not spend a critical, damage, or \
         effect-chance draw"
    );
}

/// Levitate returns from `CheckWonderGuardAndLevitate` before its type scan
/// (`battle_script_commands.c:1435-1443`), so a Ground move that misses a
/// Levitate holder reports the Ground miss. No admitted ordinary-hit move is
/// Ground-type, so the classifier is exercised directly here; the multi-hit
/// pipeline covers it end to end with Bone Rush.
#[test]
fn a_failed_accuracy_roll_against_levitate_reports_levitate() {
    let dex = Dex::new();
    let levitate_defender = mon(&dex, GASTLY, 20, vec![TACKLE]);
    assert_eq!(levitate_defender.ability(), AbilityId::LEVITATE);

    assert_eq!(
        classify_accuracy_failure(&dex, BONE_RUSH, &levitate_defender),
        Ok(HitOutcome::LevitateBlocked)
    );
}

/// `CheckWonderGuardAndLevitate` returns immediately for Struggle and for a
/// zero-power move (`battle_script_commands.c:1432-1433`), leaving
/// `MISS_TYPE = B_MSG_MISSED`. An ordinary matchup keeps the generic miss for
/// the same reason: nothing in the helper matches.
#[test]
fn struggle_and_an_ordinary_matchup_keep_the_generic_miss() {
    let dex = Dex::new();
    let ghost_defender = mon(&dex, GASTLY, 20, vec![TACKLE]);
    let ordinary_defender = mon(&dex, SQUIRTLE, 20, vec![TACKLE]);

    assert_eq!(
        classify_accuracy_failure(&dex, STRUGGLE, &ghost_defender),
        Ok(HitOutcome::Miss),
        "Struggle skips the helper entirely, so its immunity never applies"
    );
    assert_eq!(
        classify_accuracy_failure(&dex, GROWL, &ordinary_defender),
        Ok(HitOutcome::Miss),
        "a zero-power move returns before the helper reads any typing"
    );
    assert_eq!(
        classify_accuracy_failure(&dex, TACKLE, &ordinary_defender),
        Ok(HitOutcome::Miss)
    );
}

/// The classifier reads only move data and the defender, so it can never
/// disturb the draw sequence the failed accuracy roll left behind.
#[test]
fn classifying_a_failed_roll_rejects_an_unknown_move_and_draws_nothing() {
    let dex = Dex::new();
    let defender = mon(&dex, SQUIRTLE, 20, vec![TACKLE]);

    assert_eq!(
        classify_accuracy_failure(&dex, UNKNOWN_MOVE, &defender),
        Err(BattleError::UnknownMove(UNKNOWN_MOVE))
    );
}

#[test]
fn flash_fire_absorbs_a_fire_move_without_drawing_and_latches_once() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 20, vec![EMBER]);
    let mut vulpix = mon(&dex, VULPIX, 20, vec![TACKLE]);
    assert_eq!(vulpix.ability(), AbilityId::FLASH_FIRE);
    let mut rng = SequenceRng::new([]);

    let first = damage_core(&dex, EMBER, &attacker, &vulpix, false, &mut rng).unwrap();
    assert_eq!(
        first,
        HitOutcome::FlashFireBlocked {
            first_activation: true
        }
    );
    assert!(vulpix.volatiles_mut().activate_flash_fire());

    // Later activations still cancel the move but report no boost, and the
    // latch is not a counter.
    let second = damage_core(&dex, EMBER, &attacker, &vulpix, false, &mut rng).unwrap();
    assert_eq!(
        second,
        HitOutcome::FlashFireBlocked {
            first_activation: false
        }
    );
    assert!(!vulpix.volatiles_mut().activate_flash_fire());
    assert_eq!(rng.draws(), 0, "absorption precedes every draw");

    // Unlike Volt Absorb and Water Absorb, there is no power gate and no
    // `MOVE_RESULT_NO_EFFECT` write; non-Fire moves are unaffected.
    let mut rng = SequenceRng::new([ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW]);
    let tackle = damage_core(&dex, TACKLE, &attacker, &vulpix, false, &mut rng).unwrap();
    assert!(matches!(tackle, HitOutcome::Hit { .. }));
}

#[test]
fn a_frozen_flash_fire_holder_takes_a_fire_move_like_any_other_target() {
    // `ABILITY_FLASH_FIRE` requires `!(status1 & STATUS1_FREEZE)`
    // (`battle_util.c:2703-2704`), so the move is not absorbed, latches
    // nothing, and spends the ordinary crit and damage draws.
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 20, vec![EMBER]);
    let mut vulpix = mon(&dex, VULPIX, 20, vec![TACKLE]);
    vulpix.set_status1(crate::Status1::Frozen);
    let mut rng = SequenceRng::new([ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW]);

    let outcome = damage_core(&dex, EMBER, &attacker, &vulpix, false, &mut rng).unwrap();

    assert!(matches!(outcome, HitOutcome::Hit { .. }), "{outcome:?}");
    assert_eq!(rng.draws(), 2);
    assert!(!vulpix.volatiles().flash_fire, "nothing latched");
}

#[test]
fn a_latched_flash_fire_boosts_ember_before_the_final_plus_two() {
    let dex = Dex::new();
    let control = mon(&dex, VULPIX, 20, vec![EMBER]);
    let mut latched = mon(&dex, VULPIX, 20, vec![EMBER]);
    latched.volatiles_mut().activate_flash_fire();
    let defender = mon(&dex, BULBASAUR, 20, vec![TACKLE]);
    let draws = [ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW];

    let unboosted = damage_before_roll(
        &dex,
        EMBER,
        &control,
        &defender,
        false,
        &mut SequenceRng::new(draws),
    )
    .unwrap()
    .damage;
    let boosted = damage_before_roll(
        &dex,
        EMBER,
        &latched,
        &defender,
        false,
        &mut SequenceRng::new(draws),
    )
    .unwrap()
    .damage;

    // Reproduce the pre-STAB base so the expectation fixes the order: the
    // 15/10 multiplier truncates before the constant 2 is added.
    let input = |boost: bool| {
        let move_data = dex.move_data(EMBER).unwrap();
        let category = MoveCategory::Special;
        let (attack, attack_stage) = control.attacking_stat(category);
        let (defense, defense_stage) = defender.defending_stat(category);
        let input = DamageInput {
            attacker_level: 20,
            power: u32::from(move_data.power),
            move_type: Type::Fire,
            attack_stat: attack,
            attack_stage,
            defense_stat: defense,
            defense_stage,
            attacker_burned: false,
            reflect: false,
            light_screen: false,
            weather: Weather::None,
            is_solar_beam: false,
            attacker_pinch_boost: false,
        };
        crate::damage::base_damage_with_flash_fire(&input, boost)
    };
    let plain = input(false);
    let with_boost = input(true);
    assert_eq!(with_boost, (plain - 2) * 15 / 10 + 2);
    assert!(with_boost > plain);
    assert!(boosted > unboosted, "{boosted} vs {unboosted}");

    // Only Fire moves are boosted.
    let tackle_control = damage_before_roll(
        &dex,
        TACKLE,
        &control,
        &defender,
        false,
        &mut SequenceRng::new(draws),
    )
    .unwrap()
    .damage;
    let tackle_latched = damage_before_roll(
        &dex,
        TACKLE,
        &latched,
        &defender,
        false,
        &mut SequenceRng::new(draws),
    )
    .unwrap()
    .damage;
    assert_eq!(tackle_latched, tackle_control);
}
