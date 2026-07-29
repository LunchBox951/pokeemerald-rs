//! The player's run-away formula (S-6): `TryRunFromBattle`.
//!
//! The issue's suggested name, `Cmd_getescapevalue`, does not exist in the
//! upstream checkout at `pokeemerald/` (no hit for `getescapevalue`/
//! `EscapeValue` anywhere under `src/`/`include/`); the actual formula lives
//! in `TryRunFromBattle` (`pokeemerald/src/battle_util.c:407`), called from
//! `HandleAction_Run` (`:487`) when the player chooses Run.
//!
//! This slice models only the plain single-battle path: no held item
//! (`HOLD_EFFECT_CAN_ALWAYS_RUN`), no `ABILITY_RUN_AWAY`, no Battle
//! Frontier/Trainer Hill trainer-battle auto-success, and no Battle Pyramid
//! run multiplier — all ability/item/mode-gated and out of scope
//! (`(behavioral-fidelity)`'s "as far as the first-encounter species need").
//! That leaves upstream's final `else` branch (`:452`, non-Pyramid
//! single-battle sub-branch `:463`-`:468`):
//!
//! ```text
//! if playerSpeed >= enemySpeed:
//!     success                      // no RNG draw
//! else:
//!     speedVar: u8 = (playerSpeed * 128 / enemySpeed + runTries * 30) as u8  // wraps
//!     success = speedVar > (Random() & 0xFF)
//! runTries += 1                    // always, win or lose
//! ```
//!
//! One gate sits *upstream of* this function and is deliberately not modelled
//! here: `IsRunningFromBattleImpossible` (`src/battle_main.c:4021`) returns
//! `BATTLE_RUN_FORBIDDEN` for `BATTLE_TYPE_FIRST_BATTLE` (`:4078`-`:4082`,
//! the "don't leave Prof. Birch!" message), so the scripted Route 101
//! Poochyena battle — the only battle that flag is ever set for
//! (`src/battle_setup.c:937`) — can never run at all and never reaches
//! `TryRunFromBattle`. This module models the ordinary **post-first-battle**
//! wild encounter, where that flag is clear.
//!
//! `speedVar` is upstream `u8`: the right-hand expression is computed then
//! **truncated** (silently wraps, does not saturate) on assignment — modelled
//! here with an explicit `as u8` on the same expression, not a clamp.

use crate::damage::BattleRng;

/// Attempt to run from a wild battle. Returns `true` on success.
///
/// `player_speed`/`enemy_speed` are each battler's **raw** `gBattleMons[].speed`
/// — the computed battle stat, *not* the stat-stage-modified effective Speed.
/// Upstream reads the struct field directly at
/// `pokeemerald/src/battle_util.c:463` (`gBattleMons[battler].speed <
/// gBattleMons[BATTLE_OPPOSITE(battler)].speed`) and again at `:465`
/// (`speedVar = (gBattleMons[battler].speed * 128) / ...`) with no
/// `APPLY_STAT_MOD`/`GetWhoStrikesFirst` detour, so an Agility or String Shot
/// in play changes turn order but **not** escape odds `(behavioral-fidelity)`.
/// Callers pass [`crate::pokemon::Stats::speed`], never
/// [`crate::pokemon::BattlePokemon::effective_speed`].
///
/// `run_tries` is the number of *previous* attempts this battle (upstream
/// `gBattleStruct->runTries`, owned by [`crate::battle::Battle`] and
/// incremented by the caller after this call — see the module docs for why
/// that increment is unconditional).
#[must_use]
pub fn try_run_from_battle(
    player_speed: u32,
    enemy_speed: u32,
    run_tries: u32,
    rng: &mut impl BattleRng,
) -> bool {
    if player_speed >= enemy_speed {
        return true;
    }
    let raw = (player_speed * 128) / enemy_speed + run_tries * 30;
    #[allow(clippy::cast_possible_truncation)]
    let speed_var = raw as u8;
    let roll = (rng.next_u16() & 0xFF) as u8;
    speed_var > roll
}

#[cfg(test)]
mod tests {
    use super::try_run_from_battle;
    use crate::damage::BattleRng;

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
        // playerSpeed 50, enemySpeed 100, runTries 0:
        // speedVar = 50*128/100 + 0 = 64.
        // Random() & 0xFF == 63 -> 64 > 63 -> success.
        let mut rng = FixedRng(63);
        assert!(try_run_from_battle(50, 100, 0, &mut rng));
        // Random() & 0xFF == 64 -> 64 > 64 is false -> failure.
        let mut rng = FixedRng(64);
        assert!(!try_run_from_battle(50, 100, 0, &mut rng));
    }

    #[test]
    fn run_tries_raises_the_threshold_by_thirty_per_previous_attempt() {
        // Same speeds as above but runTries=1: speedVar = 64 + 30 = 94.
        let mut rng = FixedRng(93);
        assert!(try_run_from_battle(50, 100, 1, &mut rng));
        let mut rng = FixedRng(94);
        assert!(!try_run_from_battle(50, 100, 1, &mut rng));
    }

    #[test]
    fn speed_var_wraps_like_the_upstream_u8_assignment() {
        // playerSpeed 100, enemySpeed 101 (still strictly slower, so this
        // stays on the RNG-driven branch), runTries 9:
        // raw = 100*128/101 + 9*30 = 126 (12800/101 truncated) + 270 = 396;
        // as u8 truncates (396 % 256 = 140), not saturated to 255. A roll of
        // 200 distinguishes the two: the wrapped value (140) fails
        // (140 > 200 is false) while a saturating implementation (255)
        // would have succeeded.
        let mut rng = FixedRng(200);
        assert!(
            !try_run_from_battle(100, 101, 9, &mut rng),
            "must wrap to 140, not saturate to 255"
        );
        // Just below the wrapped threshold still succeeds.
        let mut rng = FixedRng(139);
        assert!(try_run_from_battle(100, 101, 9, &mut rng));
        // At the wrapped threshold itself, it fails (140 > 140 is false).
        let mut rng = FixedRng(140);
        assert!(!try_run_from_battle(100, 101, 9, &mut rng));
    }
}
