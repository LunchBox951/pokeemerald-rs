//! Rollout and Ice Ball (S-6): the five-attempt power series, Defense Curl,
//! the start rules on a hit and a miss, forced selection, the cancellations,
//! and exact draw counts. Upstream: `Cmd_rolloutdamagecalculation`
//! (`src/battle_script_commands.c:8536`-`:8571`), `BattleScript_EffectRollout`
//! (`data/battle_scripts_1.s:1596`-`:1606`), `HandleAction_UseMove`
//! (`src/battle_util.c:107`-`:110`).

mod cancellation;
mod selection;
mod series;

use crate::common::{max_iv_mon, script, SequenceRng};
use assets::MoveId;
use battle::battle::BattlerSide;
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction};

pub(super) const ROLLOUT: MoveId = MoveId::ROLLOUT;
pub(super) const ICE_BALL: MoveId = MoveId::ICE_BALL;
pub(super) const SPLASH: MoveId = MoveId::SPLASH;
pub(super) const QUICK_ATTACK: MoveId = MoveId::QUICK_ATTACK;

pub(super) const RATTATA: u16 = 19;
pub(super) const SHEDINJA: u16 = 303;
pub(super) const SHROOMISH: u16 = 306;
pub(super) const SHARPEDO: u16 = 331;
pub(super) const WAILORD: u16 = 314;
pub(super) const DUSCLOPS: u16 = 362;

pub(super) const ATTACKER_LEVEL: u8 = 50;
/// High enough to survive the whole series, even Defense Curl's.
pub(super) const TARGET_LEVEL: u8 = 100;

/// Rollout's PP (`src/data/battle_moves.h:2668`).
pub(super) const ROLLOUT_PP: u8 = 20;

/// Construction's turn-number draw.
pub(super) const CONSTRUCTION: [u16; 1] = [0];
/// The turn-start draw and a wild opponent's move-slot pick.
pub(super) const WILD_TURN_OVERHEAD: [u16; 2] = [0, 0];
/// The turn-start draw alone, for a turn in which the opponent is locked.
pub(super) const TURN_START: [u16; 1] = [0];
/// `Random() % 100 + 1 > 90` misses Rollout's 90 accuracy.
pub(super) const ACCURACY_HITS: u16 = 89;
pub(super) const ACCURACY_MISSES: u16 = 90;
const NO_CRIT: u16 = 1;
const FULL_VARIANCE: u16 = 0;
const DISCARDED_EFFECT_CHANCE: u16 = 0;

/// A landed attempt: accuracy, critical-hit, variance and effect-chance draws.
pub(super) const LANDED: [u16; 4] = [
    ACCURACY_HITS,
    NO_CRIT,
    FULL_VARIANCE,
    DISCARDED_EFFECT_CHANCE,
];
/// A missed attempt stops after the accuracy draw.
pub(super) const MISSED: [u16; 1] = [ACCURACY_MISSES];

pub(super) fn attacker(moves: &[MoveId]) -> BattlePokemon {
    max_iv_mon(&Dex::new(), RATTATA, ATTACKER_LEVEL, moves.to_vec())
}

pub(super) fn target(species: u16, moves: &[MoveId]) -> BattlePokemon {
    max_iv_mon(&Dex::new(), species, TARGET_LEVEL, moves.to_vec())
}

pub(super) fn wild_battle(
    player: BattlePokemon,
    enemy: BattlePokemon,
    rng: &mut SequenceRng,
) -> Battle {
    Battle::new(Dex::new(), player, enemy, false, rng).unwrap()
}

pub(super) fn player_lock(battle: &Battle) -> (Option<MoveId>, Option<u8>) {
    let state = battle.battler_state(BattlerSide::Player);
    (state.locked_move(), state.lock_attempts_left())
}

pub(super) fn enemy_lock(battle: &Battle) -> (Option<MoveId>, Option<u8>) {
    let state = battle.battler_state(BattlerSide::Enemy);
    (state.locked_move(), state.lock_attempts_left())
}

/// The damage `CalculateBaseDamage` yields for a neutral, non-STAB,
/// non-critical hit at full variance (`src/pokemon.c:3181`-`:3330`).
pub(super) fn neutral_damage(level: u8, power: u32, attack: u32, defense: u32) -> u32 {
    (2 * u32::from(level) / 5 + 2) * power * attack / defense / 50 + 2
}

/// The damage the player's hit event reports.
pub(super) fn player_hit_damage(events: &[BattleEvent]) -> u32 {
    events
        .iter()
        .find_map(|event| match event {
            BattleEvent::Hit {
                by_player: true,
                damage,
                ..
            } => Some(*damage),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no player hit in {events:?}"))
}

pub(super) fn take(
    battle: &mut Battle,
    action: PlayerAction,
    rng: &mut SequenceRng,
) -> Vec<BattleEvent> {
    battle.take_turn(action, rng).unwrap()
}

pub(super) fn turn_script(groups: &[&[u16]]) -> Vec<u16> {
    script(groups)
}
