//! Escape odds and ability-based run eligibility for wild single battles.
//!
//! Upstream refuses Run at action selection for `BATTLE_TYPE_FIRST_BATTLE`
//! (`pokeemerald/src/battle_main.c:4339`-`:4344`) and for the traps
//! [`ensure_admissible`] checks (`:4038`-`:4062`), both via
//! [`crate::error::BattleError::RunForbidden`] before this formula runs.
//!
//! A Run Away holder that clears admission also bypasses this formula:
//! [`crate::battle::Battle`] resolves its escape unconditionally, drawing
//! nothing and never advancing `run_tries`
//! (`pokeemerald/src/battle_util.c:427`-`:447`, `:475`).

use assets::{AbilityId, Type};

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

const SPEED_RATIO_SCALE: u32 = 128;
const PREVIOUS_ATTEMPT_BONUS: u32 = 30;

/// Rejects a Run selection an opposing trap ability forbids, before any draw.
/// Run Away exempts the runner from both traps; otherwise an opposing Shadow
/// Tag, or Arena Trap against a runner that is neither Levitate nor
/// [`Type::Flying`], refuses the selection
/// (`pokeemerald/src/battle_main.c:4038`-`:4062`).
///
/// # Errors
///
/// Returns [`BattleError::RunForbidden`] when a trap applies.
pub fn ensure_admissible(
    runner: &BattlePokemon,
    opponent: &BattlePokemon,
) -> Result<(), BattleError> {
    if runner.ability() == AbilityId::RUN_AWAY {
        return Ok(());
    }
    if opponent.ability() == AbilityId::SHADOW_TAG {
        return Err(BattleError::RunForbidden);
    }
    let runner_is_grounded =
        runner.ability() != AbilityId::LEVITATE && !runner.types().contains(&Type::Flying);
    if opponent.ability() == AbilityId::ARENA_TRAP && runner_is_grounded {
        return Err(BattleError::RunForbidden);
    }
    Ok(())
}

/// Attempts to run using raw battler speeds and the wrapping count of previous
/// attempts.
///
/// Equal or greater raw Speed succeeds without drawing. A slower attempt draws
/// once. Its computed threshold keeps only the low byte instead of saturating,
/// matching `TryRunFromBattle`'s `u8` assignment
/// (`pokeemerald/src/battle_util.c:463`-`:466`).
#[must_use]
pub fn try_run_from_battle(
    player_raw_speed: u32,
    enemy_raw_speed: u32,
    previous_attempts: u8,
    rng: &mut impl BattleRng,
) -> bool {
    if player_raw_speed >= enemy_raw_speed {
        return true;
    }

    let untruncated_escape_threshold = (player_raw_speed * SPEED_RATIO_SCALE) / enemy_raw_speed
        + u32::from(previous_attempts) * PREVIOUS_ATTEMPT_BONUS;
    let escape_threshold = untruncated_escape_threshold.to_le_bytes()[0];
    let escape_roll = rng.next_u16().to_le_bytes()[0];
    escape_threshold > escape_roll
}

#[cfg(test)]
mod tests {
    use super::{ensure_admissible, try_run_from_battle};
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::pokemon::{BattlePokemon, Ivs};
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
    const SECONDARY_ABILITY_PERSONALITY: u32 = 1;

    /// `SPECIES_RATTATA`: primary Run Away, secondary Guts.
    const RATTATA: SpeciesId = SpeciesId(19);
    /// `SPECIES_CHARMANDER`: an ordinary grounded, non-trapping runner.
    const CHARMANDER: SpeciesId = SpeciesId(4);
    /// `SPECIES_WOBBUFFET`: primary and only ability Shadow Tag.
    const WOBBUFFET: SpeciesId = SpeciesId(202);
    /// `SPECIES_TRAPINCH`: primary Hyper Cutter, secondary Arena Trap.
    const TRAPINCH: SpeciesId = SpeciesId(332);
    /// `SPECIES_HAUNTER`: primary and only ability Levitate.
    const HAUNTER: SpeciesId = SpeciesId(93);
    /// `SPECIES_PIDGEY`: Normal/Flying, no Levitate.
    const PIDGEY: SpeciesId = SpeciesId(16);

    fn mon(dex: &Dex, species: SpeciesId, personality: u32) -> BattlePokemon {
        BattlePokemon::new(dex, species, 5, MAX_IVS, personality, vec![MoveId(33)]).unwrap()
    }

    #[test]
    fn an_ordinary_matchup_is_admissible() {
        let dex = Dex::new();
        let runner = mon(&dex, CHARMANDER, PRIMARY_ABILITY_PERSONALITY);
        let opponent = mon(&dex, CHARMANDER, PRIMARY_ABILITY_PERSONALITY);
        assert_eq!(ensure_admissible(&runner, &opponent), Ok(()));
    }

    #[test]
    fn run_away_bypasses_shadow_tag_and_arena_trap() {
        let dex = Dex::new();
        let runner = mon(&dex, RATTATA, PRIMARY_ABILITY_PERSONALITY);
        let shadow_tag = mon(&dex, WOBBUFFET, PRIMARY_ABILITY_PERSONALITY);
        let arena_trap = mon(&dex, TRAPINCH, SECONDARY_ABILITY_PERSONALITY);
        assert_eq!(ensure_admissible(&runner, &shadow_tag), Ok(()));
        assert_eq!(ensure_admissible(&runner, &arena_trap), Ok(()));
    }

    #[test]
    fn shadow_tag_refuses_the_selection_with_no_levitate_or_flying_exemption() {
        let dex = Dex::new();
        let shadow_tag = mon(&dex, WOBBUFFET, PRIMARY_ABILITY_PERSONALITY);
        let grounded_runner = mon(&dex, CHARMANDER, PRIMARY_ABILITY_PERSONALITY);
        let levitate_runner = mon(&dex, HAUNTER, PRIMARY_ABILITY_PERSONALITY);
        let flying_runner = mon(&dex, PIDGEY, PRIMARY_ABILITY_PERSONALITY);
        for runner in [&grounded_runner, &levitate_runner, &flying_runner] {
            assert_eq!(
                ensure_admissible(runner, &shadow_tag),
                Err(BattleError::RunForbidden)
            );
        }
    }

    #[test]
    fn arena_trap_refuses_a_grounded_non_levitate_non_flying_runner() {
        let dex = Dex::new();
        let arena_trap = mon(&dex, TRAPINCH, SECONDARY_ABILITY_PERSONALITY);
        let grounded_runner = mon(&dex, CHARMANDER, PRIMARY_ABILITY_PERSONALITY);
        assert_eq!(
            ensure_admissible(&grounded_runner, &arena_trap),
            Err(BattleError::RunForbidden)
        );
    }

    #[test]
    fn arena_trap_exempts_levitate_and_flying_runners() {
        let dex = Dex::new();
        let arena_trap = mon(&dex, TRAPINCH, SECONDARY_ABILITY_PERSONALITY);
        let levitate_runner = mon(&dex, HAUNTER, PRIMARY_ABILITY_PERSONALITY);
        let flying_runner = mon(&dex, PIDGEY, PRIMARY_ABILITY_PERSONALITY);
        assert_eq!(ensure_admissible(&levitate_runner, &arena_trap), Ok(()));
        assert_eq!(ensure_admissible(&flying_runner, &arena_trap), Ok(()));
    }

    struct FixedRng(u16);
    impl BattleRng for FixedRng {
        fn next_u16(&mut self) -> u16 {
            self.0
        }
    }

    struct CountingRng {
        value: u16,
        draws: u32,
    }
    impl BattleRng for CountingRng {
        fn next_u16(&mut self) -> u16 {
            self.draws += 1;
            self.value
        }
    }

    #[test]
    fn equal_or_faster_always_succeeds_with_no_rng_draw() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        assert!(try_run_from_battle(100, 100, 0, &mut rng));
        assert!(try_run_from_battle(150, 100, 0, &mut rng));
        assert_eq!(rng.draws, 0);
    }

    #[test]
    fn slower_player_draws_exactly_once() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        let _ = try_run_from_battle(50, 100, 0, &mut rng);
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn slower_player_succeeds_or_fails_by_hand_computed_threshold() {
        const ESCAPE_THRESHOLD: u16 = 64;

        let mut rng = FixedRng(ESCAPE_THRESHOLD - 1);
        assert!(try_run_from_battle(50, 100, 0, &mut rng));
        let mut rng = FixedRng(ESCAPE_THRESHOLD);
        assert!(!try_run_from_battle(50, 100, 0, &mut rng));
    }

    #[test]
    fn run_tries_raises_the_threshold_by_thirty_per_previous_attempt() {
        const SECOND_ATTEMPT_THRESHOLD: u16 = 94;

        let mut rng = FixedRng(SECOND_ATTEMPT_THRESHOLD - 1);
        assert!(try_run_from_battle(50, 100, 1, &mut rng));
        let mut rng = FixedRng(SECOND_ATTEMPT_THRESHOLD);
        assert!(!try_run_from_battle(50, 100, 1, &mut rng));
    }

    #[test]
    fn escape_threshold_wraps_instead_of_saturating() {
        const WRAPPED_THRESHOLD: u16 = 140;
        const ROLL_BEATEN_ONLY_BY_A_SATURATED_THRESHOLD: u16 = 200;

        let mut rng = FixedRng(ROLL_BEATEN_ONLY_BY_A_SATURATED_THRESHOLD);
        assert!(
            !try_run_from_battle(100, 101, 9, &mut rng),
            "must wrap to 140, not saturate to 255"
        );
        let mut rng = FixedRng(WRAPPED_THRESHOLD - 1);
        assert!(try_run_from_battle(100, 101, 9, &mut rng));
        let mut rng = FixedRng(WRAPPED_THRESHOLD);
        assert!(!try_run_from_battle(100, 101, 9, &mut rng));
    }
}
