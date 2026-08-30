//! The wild opponent's two action-selection algorithms (S-6; the
//! `first_battle` half is issue #187) — split out of `battle.rs`
//! `(oop-boundaries)`: both decide what the wild side does this turn from
//! the same kind of inputs (the wild mon's own moveset, plus — `first_battle`
//! only — the player's current HP), and neither needs
//! [`crate::battle::Battle`]'s turn-order/PP/event-emission machinery, which
//! stays where [`EnemyAction`], the type connecting the two back to it, is
//! consumed.
//!
//! [`choose_enemy_move`] is upstream's plain-wild rejection loop
//! (`battle_controller_opponent.c:1594`-`:1601`); [`choose_enemy_action_first_battle`]
//! is the `BATTLE_TYPE_FIRST_BATTLE` AI branch (`:1563`), narrowed to exactly
//! the one AI script that battle type ever runs. See each function's own docs
//! for the full derivation, and [`crate::battle`]'s module docs for how the
//! two fit into a turn.

use assets::MoveId;

use crate::damage::BattleRng;
use crate::pokemon::{BattlePokemon, MAX_MON_MOVES, MOVE_NONE};

/// The wild opponent's chosen action for a turn — what [`choose_enemy_move`]
/// (ordinary) or [`choose_enemy_action_first_battle`] (`first_battle`)
/// settled on, unified so [`crate::battle::Battle::resolve_turn`] and
/// [`crate::battle::Battle::enemy_acts`] do not need to know which path
/// produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnemyAction {
    /// A real, known move in this slot.
    Move(usize),
    /// Every known slot is out of PP — upstream's forced Struggle
    /// (`AreAllMovesUnusable`), which this engine cannot execute (see
    /// [`crate::battle::Battle::enemy_acts`]).
    Struggle,
    /// The `first_battle` AI chose to flee (`AI_CHOICE_FLEE`) instead of a
    /// move — see [`choose_enemy_action_first_battle`] and
    /// [`crate::battle::BattleOutcome::WildFled`]. Never produced by the
    /// ordinary path.
    Flee,
}

/// Upstream's move-selection legality test, in one place: a slot is a legal
/// pick only if it holds a real move.
///
/// `slot_move` is `None` for an index past the mon's known moves — upstream's
/// `moves[i] == MOVE_NONE` for an unfilled slot — and `Some(MOVE_NONE)` for a
/// slot explicitly holding the placeholder. Both are rejected, which is both
/// halves of upstream's rule: the wild opponent's rejection loop redraws while
/// `move == MOVE_NONE` (`battle_controller_opponent.c:1599`-`:1601`) and
/// `CheckMoveLimitations` marks such a slot unselectable for the player's menu
/// (`MOVE_LIMITATION_ZEROMOVE`, `battle_util.c:1098`).
///
/// [`BattlePokemon::new`] already refuses to store a [`MOVE_NONE`] slot, so
/// the `Some(MOVE_NONE)` arm is belt-and-braces for this crate's own types —
/// it is upstream's actual loop condition, kept so the rule lives here rather
/// than being implied by a length comparison.
pub(crate) const fn selectable_slot(slot_move: Option<MoveId>) -> bool {
    match slot_move {
        // Compared field-wise rather than with `!=`: `PartialEq` is not
        // callable from a `const fn` on stable.
        Some(move_id) => move_id.0 != MOVE_NONE.0,
        None => false,
    }
}

/// The wild opponent's move choice: upstream's plain-wild rejection loop
/// (`battle_controller_opponent.c:1594`-`:1601`), reproduced draw for
/// draw —
///
/// ```text
/// do { chosenMoveId = MOD(Random(), MAX_MON_MOVES); } while (move == MOVE_NONE);
/// ```
///
/// — where `MOD(a, 4)` is `a & 3` (`include/global.h:97`), i.e. plain
/// `% 4` for an unsigned draw. The loop's test is on the *move*, not the
/// slot index: it retries while `moveInfo->moves[chosenMoveId]` is
/// `MOVE_NONE` (`battle_controller_opponent.c:1601`), which is what
/// [`selectable_slot`] reproduces. A slot past the end of this mon's known
/// moves is exactly one of those `MOVE_NONE` slots, so it is rejected and
/// redrawn: a one-move wild mon consumes one draw per `0`-mod-4 value and
/// spins otherwise, exactly as upstream does.
///
/// The loop terminates because [`BattlePokemon::new`] rejects both an
/// empty moveset ([`crate::error::BattleError::InvalidMoveCount`]) and a
/// `MOVE_NONE` slot ([`crate::error::BattleError::PlaceholderMove`]), so at
/// least one of the four residues always accepts.
///
/// Returns `None` for the one case upstream never reaches this loop at
/// all: `AreAllMovesUnusable` (`battle_util.c:1125`-`:1140`, checked at
/// `battle_main.c:4184` before `ChooseMove` is ever emitted) forces
/// Struggle through a selection script when **every** slot is unusable —
/// with no items/Disable/Taunt/Torment modelled, exactly "every known
/// slot has 0 PP" (unfilled slots are `MOVE_NONE`, always unusable). The
/// forced pick draws nothing. Note the loop itself ignores PP: a spent
/// slot with a real move is picked upstream and then *fails* at
/// `Cmd_attackcanceler` — no draws, no damage, no deduction, and the
/// turn continues (see [`crate::battle::Battle::act`] and
/// [`crate::battle::BattleEvent::FailedNoPp`]); only the all-spent case
/// diverts to Struggle.
pub(crate) fn choose_enemy_move(enemy: &BattlePokemon, rng: &mut impl BattleRng) -> Option<usize> {
    if enemy.moves().iter().all(|slot| slot.pp == 0) {
        return None;
    }
    loop {
        let slot = usize::from(rng.next_u16()) % MAX_MON_MOVES;
        if selectable_slot(enemy.move_at(slot)) {
            return Some(slot);
        }
    }
}

/// The wild opponent's HP percentage threshold below which
/// [`choose_enemy_action_first_battle`]'s `AI_FirstBattle` script flees
/// (`data/battle_ai_scripts.s:3234`-`:3235`: `if_hp_equal AI_TARGET, 20,
/// Flee` then `if_hp_less_than AI_TARGET, 20, Flee`, i.e. `<= 20`).
/// `Cmd_if_hp_less_than`/`_equal` compute the percentage as `100 * hp /
/// maxHP` (`battle_ai_script_commands.c:713`-`:726`, truncating integer
/// division) — a **percentage** of the *player's* current HP, not a flat HP
/// value.
pub(crate) const FIRST_BATTLE_FLEE_HP_PERCENT: u32 = 20;

/// The wild opponent's move choice under `first_battle`
/// ([`crate::battle::Battle::new`]) — `OpponentHandleChooseMove`'s AI branch
/// (`BATTLE_TYPE_FIRST_BATTLE` is one of four flags routed there,
/// `battle_controller_opponent.c:1563`), narrowed to exactly what
/// `AI_SCRIPT_FIRST_BATTLE` (`assets::trainers::AiFlags::FIRST_BATTLE`,
/// bit 31) can produce — the *only* aiFlags bit this battle type ever
/// sets (`BattleAI_SetupAIData`, `battle_ai_script_commands.c:312`-
/// `:341`), so this is the complete algorithm for this battle type, not
/// a narrowed one:
///
/// 1. `AreAllMovesUnusable` (`battle_util.c:1125`, checked at
///    `battle_main.c:4184` before `OpponentHandleChooseMove` is ever
///    reached) forces Struggle when every known slot has `0` PP,
///    drawing nothing — same rule, same [`EnemyAction::Struggle`]
///    result, as the ordinary [`choose_enemy_move`]'s `None`.
/// 2. Otherwise `BattleAI_SetupAIData(ALL_MOVES_MASK)` seeds every real
///    move slot's score at 100 (a spent slot's score is zeroed by
///    `CheckMoveLimitations`, no RNG involved) and then *unconditionally*
///    draws `Random()` **four** times filling `simulatedRNG[0..4]`
///    (`100 - (Random() % 16)`, `:341`) — read only by AI scripts this
///    battle type never runs (`AI_TryToFaint`/`AI_HPAware`), so the
///    values themselves go unused, but the draws still happen and still
///    cost RNG stream position `(behavioral-fidelity)`.
/// 3. `AI_FirstBattle` is the sole script `AI_SCRIPT_FIRST_BATTLE` runs:
///    flee ([`EnemyAction::Flee`]) once the player's raw current HP
///    drops to [`FIRST_BATTLE_FLEE_HP_PERCENT`] or below of max. No RNG
///    draw either way.
/// 4. Otherwise `ChooseMoveOrAction_Singles` picks the highest-scoring
///    move, breaking ties uniformly at random over however many slots
///    share the max (`Random() % numOfBestMoves`, drawn unconditionally
///    even when only one move ties for the max). Because no script here
///    ever moves a score away from its seeded 100 (only the flee check
///    above reads HP), every un-spent real move slot ties at 100 and
///    every spent one sits at 0 — so this step reduces to "pick
///    uniformly among the slots that still have PP", which is what the
///    `usable` filter below computes directly rather than replaying the
///    tie-break over an untouched score array.
///
/// Unlike the ordinary rejection loop, this algorithm is PP-aware by
/// construction (`usable` excludes spent slots before the draw, not
/// after a pick), so [`crate::battle::BattleEvent::FailedNoPp`] can never
/// fire on this path — a real behavioural divergence from the ordinary
/// wild encounter, not merely an implementation detail.
pub(crate) fn choose_enemy_action_first_battle(
    enemy: &BattlePokemon,
    player: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> EnemyAction {
    let usable: Vec<usize> = (0..MAX_MON_MOVES)
        .filter(|&i| selectable_slot(enemy.move_at(i)) && enemy.moves()[i].pp > 0)
        .collect();
    if usable.is_empty() {
        return EnemyAction::Struggle;
    }
    // BattleAI_SetupAIData's simulatedRNG fill: four discarded draws,
    // unconditional once AreAllMovesUnusable has already passed.
    for _ in 0..4 {
        let _ = rng.next_u16();
    }
    let player_hp_percent = 100 * player.current_hp() / player.stats().max_hp;
    if player_hp_percent <= FIRST_BATTLE_FLEE_HP_PERCENT {
        return EnemyAction::Flee;
    }
    let index = usize::from(rng.next_u16()) % usable.len();
    EnemyAction::Move(usable[index])
}

#[cfg(test)]
mod tests {
    use super::{
        choose_enemy_action_first_battle, choose_enemy_move, selectable_slot, EnemyAction,
    };
    use crate::pokemon::{BattlePokemon, Ivs, MOVE_NONE};
    use crate::script_rng::SequenceRng;
    use assets::{MoveId, SpeciesId};

    const MAX_IVS: Ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };

    fn mon(species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
        BattlePokemon::new(
            &crate::dex::Dex::new(),
            SpeciesId(species),
            level,
            MAX_IVS,
            0,
            moves,
        )
        .unwrap()
    }

    #[test]
    fn the_rejection_loop_only_accepts_slots_holding_a_real_move() {
        // Upstream's loop condition is on the *move*, not the index:
        // `while (move == MOVE_NONE)` (battle_controller_opponent.c:1601).
        assert!(selectable_slot(Some(MoveId(33))));
        assert!(
            !selectable_slot(Some(MOVE_NONE)),
            "an explicit MOVE_NONE slot is what upstream redraws past"
        );
        assert!(
            !selectable_slot(None),
            "a slot past the known moves is upstream's unfilled MOVE_NONE slot"
        );
    }

    #[test]
    fn ordinary_choice_returns_none_when_every_slot_is_spent() {
        let mut enemy = mon(288, 2, vec![MoveId(33)]); // Zigzagoon, Tackle
        for _ in 0..enemy.moves()[0].pp {
            enemy.deduct_pp(0).unwrap();
        }
        let mut rng = SequenceRng::new([]);
        assert_eq!(choose_enemy_move(&enemy, &mut rng), None);
        assert_eq!(rng.draws(), 0, "the forced fallback draws nothing");
    }

    #[test]
    fn first_battle_choice_forces_struggle_before_any_draw_when_every_slot_is_spent() {
        let mut enemy = mon(288, 2, vec![MoveId(33), MoveId(45)]); // Tackle, Growl
        for slot in 0..enemy.moves().len() {
            for _ in 0..enemy.moves()[slot].pp {
                enemy.deduct_pp(slot).unwrap();
            }
        }
        let player = mon(1, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            choose_enemy_action_first_battle(&enemy, &player, &mut rng),
            EnemyAction::Struggle
        );
        assert_eq!(rng.draws(), 0, "AreAllMovesUnusable draws nothing");
    }

    #[test]
    fn first_battle_choice_flees_at_or_below_twenty_percent_and_draws_only_the_setup() {
        let enemy = mon(288, 50, vec![MoveId(33), MoveId(45)]);
        let mut player = mon(1, 50, vec![MoveId(33)]);
        let max_hp = player.stats().max_hp;
        player.apply_damage(max_hp - max_hp * 20 / 100);
        assert!(100 * player.current_hp() / max_hp <= 20);

        let mut rng = SequenceRng::new([0, 0, 0, 0]); // the four discarded simulatedRNG draws
        assert_eq!(
            choose_enemy_action_first_battle(&enemy, &player, &mut rng),
            EnemyAction::Flee
        );
        assert_eq!(
            rng.draws(),
            4,
            "the flee check itself draws nothing, so only setup is spent"
        );
    }

    #[test]
    fn first_battle_choice_never_selects_a_spent_slot() {
        let mut enemy = mon(288, 50, vec![MoveId(33), MoveId(45)]);
        for _ in 0..enemy.moves()[0].pp {
            enemy.deduct_pp(0).unwrap(); // spend Tackle entirely
        }
        let player = mon(1, 50, vec![MoveId(33)]); // full HP: no flee

        // 4 discarded simulatedRNG draws, then a deliberately *even*
        // tie-break value: with the PP screen, only Growl is a candidate
        // (`998 % 1 == 0`); without it, both slots would be and
        // `998 % 2 == 0` would select the spent Tackle -- so this value
        // fails if the screen is ever dropped (999 would pass either way).
        let mut rng = SequenceRng::new([0, 0, 0, 0, 998]);
        assert_eq!(
            choose_enemy_action_first_battle(&enemy, &player, &mut rng),
            EnemyAction::Move(1),
            "slot 1 (Growl) is the only slot left with PP"
        );
    }
}
