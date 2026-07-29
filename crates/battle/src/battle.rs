//! The battle state machine (S-6, issue #159): a headless single wild
//! battle from action selection through victory/defeat/run.
//!
//! [`Battle`] is an owned type mirroring the upstream battle-main loop's
//! *observable* order (`pokeemerald/src/battle_main.c`, `battle_util.c`),
//! not its task/callback structure `(oop-boundaries)`: intro → action
//! selection → turn resolution → faint/exp → victory/defeat/run. "Intro" has
//! no state-affecting logic to model (it is pure presentation upstream —
//! sending out both mons, printing the encounter message), so [`Battle::new`]
//! starts directly in the action-selection-ready state; a caller drives the
//! loop by calling [`Battle::take_turn`] once per turn until
//! [`Battle::outcome`] is `Some`.
//!
//! Turn resolution, per upstream's `SetActionsAndBattlersTurnOrder`
//! (`battle_main.c:4756`) plus each battler's `HandleAction_*`:
//!
//! 1. A chosen [`PlayerAction::Run`] always resolves **first**, before any
//!    move (`turnOrderId = 5` early-out — the non-link branch's
//!    `gChosenActionByBattler[0] == B_ACTION_RUN` test at
//!    `battle_main.c:4784`-`:4794`, then the reordering block guarded by
//!    `if (turnOrderId == 5)` at `:4797`; `:4778` is the link-battle variant,
//!    which this slice does not model): success ends the battle immediately;
//!    failure burns the player's turn and the opponent still acts.
//! 2. Otherwise, [`crate::turn_order::resolve_order`] decides who moves
//!    first from each side's chosen move priority and effective Speed.
//! 3. The first mover's move resolves via [`crate::hit::resolve_hit`]; if it
//!    faints the target, the battle ends immediately (win → exp gain via
//!    [`crate::exp::wild_faint_exp`]; loss otherwise) and the second mover
//!    never acts — a fainted battler is skipped when its turn in
//!    `gBattlerByTurnOrder` comes up.
//! 4. Otherwise the second mover's move resolves the same way.
//!
//! End-of-turn residual effects (weather/status ticks) are not modelled —
//! none are modelled anywhere in this slice, so there is nothing to tick.
//!
//! # RNG draw order
//!
//! The battle RNG is a single shared stream upstream, so *where* draws happen
//! is itself observable behaviour `(behavioral-fidelity)`. This type
//! reproduces the whole per-battle sequence:
//!
//! **[`Battle::new`] — one draw, plus a conditional tie draw.**
//! `BattleStartClearSetData` (`battle_main.c:3034`) ends with
//! `gRandomTurnNumber = Random()` at `:3140`; it runs once per battle, from
//! `BeginBattleIntro` (`:3019`). Then `TryDoEventsBeforeFirstTurn` seeds the
//! initial turn order (`:3852`..`:3861`) with `GetWhoStrikesFirst(..,
//! ignoreChosenMoves=TRUE)` — both moves read as `MOVE_NONE`, so priorities
//! tie and an *exact* effective-Speed tie draws one `Random()` (`:4745`..
//! `:4750`) between the `:3140` and `:3923` draws. `new` reproduces that by
//! running [`crate::turn_order::resolve_order`] once with equal priorities
//! and discarding the ordering.
//!
//! **[`Battle::take_turn`] — one draw at the top, every turn.** The same
//! assignment appears twice more, once on each path into
//! `HandleTurnActionSelectionState`: `TryDoEventsBeforeFirstTurn`
//! (`:3841`) draws at `:3923` immediately before handing off to action
//! selection for turn 1, and `BattleTurnPassed` (`:3956`) draws at `:4013`
//! doing the same for every later turn. Exactly one of the two runs per
//! modelled turn, so one draw at the top of `take_turn` covers both. Turn 1
//! is therefore preceded by **two** draws overall (`:3140` then `:3923`) and
//! each later turn by one (`:4013`) — which is what `new` + `take_turn`
//! produces. (`gRandomTurnNumber` itself is only ever *read* by Quick Claw,
//! `:4653`/`:4687`, which this slice does not model; the value is kept in
//! [`Battle::random_turn_number`] so the draw is not merely discarded.)
//!
//! **Action selection.** The player's action is human input and draws
//! nothing. The wild opponent's does draw — see [`Battle::take_turn`] — and
//! its draws land *after* the turn-number draw and *before* turn-order
//! resolution, because upstream runs `HandleTurnActionSelectionState` for
//! every battler to completion before `SetActionsAndBattlersTurnOrder`. That
//! holds even on a turn the player runs: the opponent has already picked a
//! move by the time the run resolves, so a successful escape still consumes
//! those draws.
//!
//! **Turn order** draws 0 or 1 (a genuine Speed tie only —
//! [`crate::turn_order`]), and **each executed move** draws 1 (miss) or 3
//! (hit) — see [`crate::hit`] for why 3 rather than 2.
//!
//! # What the wild opponent chooses
//!
//! Not upstream's trainer AI (`I-5`, explicitly out of scope): a plain wild
//! battle never reaches `BattleAI_ChooseMoveOrAction` at all.
//! `OpponentHandleChooseMove` (`src/battle_controller_opponent.c:1551`) takes
//! the AI branch only for `BATTLE_TYPE_TRAINER | BATTLE_TYPE_FIRST_BATTLE |
//! SAFARI | ROAMER` (`:1563`); an ordinary wild mon falls into the `else` at
//! `:1594`-`:1601`, a plain rejection loop:
//!
//! ```text
//! do {
//!     chosenMoveId = MOD(Random(), MAX_MON_MOVES);
//!     move = moveInfo->moves[chosenMoveId];
//! } while (move == MOVE_NONE);
//! ```
//!
//! That is what [`Battle::choose_enemy_move`] models, draw for draw. Note the
//! loop ignores PP entirely — a wild mon can and does pick a move it has no
//! PP for; upstream then falls back to Struggle, which this slice does not
//! model (see [`crate::error::BattleError::NoPpRemaining`]), so callers must
//! give the wild mon enough PP for the scripted scenario. `BATTLE_TYPE_
//! FIRST_BATTLE` taking the *AI* branch at `:1563` is one more reason this
//! port models the ordinary **post-first-battle** wild encounter rather than
//! the scripted Route 101 one (`src/battle_setup.c:937`) — see
//! [`crate::critical`] and [`crate::escape`] for the other two.
//!
//! Only single wild battles (one player mon, one wild mon, no switching, no
//! doubles) are modelled: a player-mon faint ends the battle in defeat
//! immediately rather than prompting a party switch.

use assets::MoveId;

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::escape::try_run_from_battle;
use crate::exp::wild_faint_exp;
use crate::hit::{resolve_hit, HitOutcome};
use crate::pokemon::{BattlePokemon, MAX_MON_MOVES};
use crate::turn_order::{resolve_order, Order};

/// The action the player commits to for a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerAction {
    /// Use the move in slot `0..MAX_MON_MOVES`
    /// (see [`crate::pokemon::MAX_MON_MOVES`]).
    UseMove(usize),
    /// Attempt to run away.
    Run,
}

/// How a finished battle ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleOutcome {
    /// The wild Pokémon fainted.
    PlayerWon,
    /// The player's Pokémon fainted.
    PlayerLost,
    /// The player successfully ran away.
    PlayerRan,
}

/// A single observable event within a turn, in the order they occurred —
/// enough for a test (or, later, a presentation layer) to reconstruct what
/// happened without re-deriving it from before/after state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleEvent {
    /// A run attempt and whether it succeeded. Always the first event of a
    /// turn where the player chose [`PlayerAction::Run`].
    RunAttempt {
        /// Always `true`: only the player runs from a wild battle this
        /// slice (see the module docs).
        by_player: bool,
        /// Whether the attempt succeeded.
        success: bool,
    },
    /// A move missed its accuracy check.
    Missed {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
    },
    /// A move connected but the target's typing made it deal no damage.
    NoEffect {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
    },
    /// A move connected and dealt damage.
    Hit {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// HP of damage dealt.
        damage: u32,
        /// Whether this was a critical hit.
        is_critical: bool,
    },
    /// A mon's HP reached `0`.
    Fainted {
        /// Whether it was the player's mon that fainted.
        by_player: bool,
    },
    /// The player's mon gained experience for fainting the wild mon.
    ExpGained(u32),
    /// The battle reached a terminal outcome; no further turns are valid.
    Ended(BattleOutcome),
}

/// An owned single wild battle `(oop-boundaries)`: one player
/// [`BattlePokemon`] against one wild [`BattlePokemon`], driven one turn at
/// a time via [`Battle::take_turn`].
#[derive(Debug, Clone)]
pub struct Battle {
    dex: Dex,
    player: BattlePokemon,
    enemy: BattlePokemon,
    run_tries: u32,
    random_turn_number: u16,
    outcome: Option<BattleOutcome>,
}

impl Battle {
    /// Start a new battle. See the module docs for why there is no separate
    /// "intro" step to advance through.
    ///
    /// Draws from `rng` exactly once: `BattleStartClearSetData`'s
    /// `gRandomTurnNumber = Random()` (`battle_main.c:3140`). See the module
    /// docs' "RNG draw order" for why that draw belongs here and the
    /// pre-turn-1 one belongs in [`Battle::take_turn`].
    #[must_use]
    pub fn new(
        dex: Dex,
        player: BattlePokemon,
        enemy: BattlePokemon,
        rng: &mut impl BattleRng,
    ) -> Self {
        let random_turn_number = rng.next_u16();
        // `TryDoEventsBeforeFirstTurn` seeds the initial turn order with
        // `ignoreChosenMoves = TRUE` (`battle_main.c:3852`..`:3861`): both
        // priorities read as 0, so an exact Speed tie costs one draw here,
        // before turn 1's own turn-number draw (module docs, "RNG draw
        // order"). The ordering itself is discarded — turn 1 re-resolves it
        // with the real chosen moves.
        let _ = resolve_order(0, 0, player.effective_speed(), enemy.effective_speed(), rng);
        Self {
            dex,
            player,
            enemy,
            run_tries: 0,
            random_turn_number,
            outcome: None,
        }
    }

    /// The player's mon.
    #[must_use]
    pub const fn player(&self) -> &BattlePokemon {
        &self.player
    }

    /// The wild mon.
    #[must_use]
    pub const fn enemy(&self) -> &BattlePokemon {
        &self.enemy
    }

    /// The battle's outcome, or `None` while it is still ongoing.
    #[must_use]
    pub const fn outcome(&self) -> Option<BattleOutcome> {
        self.outcome
    }

    /// The number of previous run attempts this battle (upstream
    /// `gBattleStruct->runTries`).
    #[must_use]
    pub const fn run_tries(&self) -> u32 {
        self.run_tries
    }

    /// The most recent `gRandomTurnNumber` draw (`battle_main.c:208`),
    /// refreshed by [`Battle::new`] and again at the top of every
    /// [`Battle::take_turn`].
    ///
    /// Nothing in this slice consumes it — upstream's only readers are the
    /// two Quick Claw checks in `GetWhoStrikesFirst` (`:4653`, `:4687`), and
    /// held items are out of scope. It is exposed so the draw is observable
    /// rather than silently discarded, and so a later held-item slice has the
    /// value already in the right place.
    #[must_use]
    pub const fn random_turn_number(&self) -> u16 {
        self.random_turn_number
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
    /// `% 4` for an unsigned draw. A slot past the end of this mon's known
    /// moves is upstream's `MOVE_NONE` slot, so the draw is rejected and
    /// retried: a one-move wild mon consumes one draw per `0`-mod-4 value and
    /// spins otherwise, exactly as upstream does.
    ///
    /// The loop terminates because [`BattlePokemon::new`] rejects an empty
    /// moveset ([`BattleError::InvalidMoveCount`]), so at least one of the
    /// four residues always accepts.
    fn choose_enemy_move(&self, rng: &mut impl BattleRng) -> usize {
        let known = self.enemy.moves.len();
        loop {
            let slot = usize::from(rng.next_u16()) % MAX_MON_MOVES;
            if slot < known {
                return slot;
            }
        }
    }

    /// The player's chosen move id, rejecting a slot no upstream selection
    /// menu could have offered (out of range, or out of PP).
    fn validate_player_move(&self, index: usize) -> Result<MoveId, BattleError> {
        let slot = self
            .player
            .moves
            .get(index)
            .ok_or(BattleError::InvalidMoveSlot(index))?;
        if slot.pp == 0 {
            return Err(BattleError::NoPpRemaining(index));
        }
        Ok(slot.move_id)
    }

    /// Resolve one turn: `player_action` for the player, and the wild
    /// opponent's own rejection-loop move pick. Returns the ordered events
    /// that occurred.
    ///
    /// Draws from `rng` in the module docs' documented order: one
    /// turn-number draw, then the opponent's action selection, then turn
    /// order, then each executed move.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::BattleAlreadyOver`] if [`Battle::outcome`] is
    /// already `Some`, [`BattleError::InvalidMoveSlot`] /
    /// [`BattleError::NoPpRemaining`] for an unusable
    /// [`PlayerAction::UseMove`] slot, or an error from
    /// [`crate::hit::resolve_hit`] (e.g. an unsupported move).
    pub fn take_turn(
        &mut self,
        player_action: PlayerAction,
        rng: &mut impl BattleRng,
    ) -> Result<Vec<BattleEvent>, BattleError> {
        if self.outcome.is_some() {
            return Err(BattleError::BattleAlreadyOver);
        }

        // Validated before any draw. An out-of-range or PP-less slot has no
        // upstream counterpart (the selection menu cannot offer one), so it
        // is a caller bug rather than a battle event -- a rejected call must
        // leave both the battle and the shared RNG stream untouched.
        let player_move = match player_action {
            PlayerAction::Run => None,
            PlayerAction::UseMove(index) => Some((index, self.validate_player_move(index)?)),
        };

        // `gRandomTurnNumber = Random()`: TryDoEventsBeforeFirstTurn
        // (`battle_main.c:3923`) on turn 1, BattleTurnPassed (`:4013`)
        // thereafter -- exactly one of the two per turn, both immediately
        // ahead of action selection.
        self.random_turn_number = rng.next_u16();

        // Action selection, in upstream's battler order: the player's is
        // human input and draws nothing; the wild opponent's rejection loop
        // runs here even when the player is about to run, because
        // HandleTurnActionSelectionState completes for every battler before
        // SetActionsAndBattlersTurnOrder looks at any of the choices.
        let enemy_index = self.choose_enemy_move(rng);
        let enemy_move = self.enemy.moves[enemy_index].move_id;

        let mut events = Vec::new();

        let Some((index, player_move)) = player_move else {
            let success = try_run_from_battle(
                // Raw gBattleMons speed on both sides, not the stage-modified
                // effective Speed -- see `crate::escape`'s parameter docs
                // (`battle_util.c:463`-`:465`).
                self.player.stats.speed,
                self.enemy.stats.speed,
                self.run_tries,
                rng,
            );
            self.run_tries += 1;
            events.push(BattleEvent::RunAttempt {
                by_player: true,
                success,
            });
            if success {
                self.finish(&mut events, BattleOutcome::PlayerRan);
                return Ok(events);
            }
            // Failed run: the turn is burned, but the wild mon still acts on
            // the move it already selected above.
            self.enemy.deduct_pp(enemy_index)?;
            self.execute_move(false, enemy_move, rng, &mut events)?;
            return Ok(events);
        };

        let player_priority = self.dex.move_data(player_move)?.priority;
        let enemy_priority = self.dex.move_data(enemy_move)?.priority;
        let order = resolve_order(
            player_priority,
            enemy_priority,
            self.player.effective_speed(),
            self.enemy.effective_speed(),
            rng,
        );

        // (is_player, move, pp-slot-owner) for the mover in each position --
        // PP is deducted only for a mover that actually acts (see the module
        // docs: a fainted target's own move never runs upstream either, so
        // its PP is untouched).
        let (first, first_move, first_slot, second, second_move, second_slot) = match order {
            Order::AttackerFirst => (true, player_move, index, false, enemy_move, enemy_index),
            Order::DefenderFirst => (false, enemy_move, enemy_index, true, player_move, index),
        };

        self.deduct_slot(first, first_slot)?;
        self.execute_move(first, first_move, rng, &mut events)?;
        if self.outcome.is_some() {
            return Ok(events);
        }
        self.deduct_slot(second, second_slot)?;
        self.execute_move(second, second_move, rng, &mut events)?;
        Ok(events)
    }

    /// Deduct PP from `slot` on the player's or enemy's mon.
    fn deduct_slot(&mut self, is_player: bool, slot: usize) -> Result<(), BattleError> {
        if is_player {
            self.player.deduct_pp(slot)
        } else {
            self.enemy.deduct_pp(slot)
        }
    }

    /// Resolve `attacker_is_player`'s use of `move_id` against the other
    /// mon, pushing the resulting events and ending the battle if the
    /// target faints.
    fn execute_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = if attacker_is_player {
                (&self.player, &self.enemy)
            } else {
                (&self.enemy, &self.player)
            };
            resolve_hit(&self.dex, move_id, attacker, defender, rng)?
        };

        match outcome {
            HitOutcome::Miss => {
                events.push(BattleEvent::Missed {
                    by_player: attacker_is_player,
                });
            }
            HitOutcome::NoEffect => {
                events.push(BattleEvent::NoEffect {
                    by_player: attacker_is_player,
                });
            }
            HitOutcome::Hit {
                damage,
                is_critical,
            } => {
                events.push(BattleEvent::Hit {
                    by_player: attacker_is_player,
                    damage,
                    is_critical,
                });
                if attacker_is_player {
                    self.enemy.apply_damage(damage);
                } else {
                    self.player.apply_damage(damage);
                }

                let defender_fainted = if attacker_is_player {
                    self.enemy.is_fainted()
                } else {
                    self.player.is_fainted()
                };
                if defender_fainted {
                    events.push(BattleEvent::Fainted {
                        by_player: !attacker_is_player,
                    });
                    if attacker_is_player {
                        let base_exp = self.dex.species(self.enemy.species)?.base_exp;
                        let exp = wild_faint_exp(base_exp, self.enemy.level);
                        events.push(BattleEvent::ExpGained(exp));
                        self.finish(events, BattleOutcome::PlayerWon);
                    } else {
                        self.finish(events, BattleOutcome::PlayerLost);
                    }
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self, events: &mut Vec<BattleEvent>, outcome: BattleOutcome) {
        self.outcome = Some(outcome);
        events.push(BattleEvent::Ended(outcome));
    }
}

#[cfg(test)]
mod tests {
    use super::{Battle, BattleEvent, BattleOutcome, PlayerAction};
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::nature::Nature;
    use crate::pokemon::{BattlePokemon, Ivs};
    use assets::{MoveId, SpeciesId};

    struct SequenceRng {
        values: Vec<u16>,
        index: usize,
    }
    impl SequenceRng {
        fn new(values: impl IntoIterator<Item = u16>) -> Self {
            Self {
                values: values.into_iter().collect(),
                index: 0,
            }
        }
        /// How many draws have been taken so far — one shared counter for a
        /// whole battle, so a test can pin the total against the documented
        /// per-phase draw order.
        fn draws(&self) -> usize {
            self.index
        }
    }
    impl BattleRng for SequenceRng {
        fn next_u16(&mut self) -> u16 {
            let v = self
                .values
                .get(self.index)
                .copied()
                .expect("SequenceRng exhausted");
            self.index += 1;
            v
        }
    }

    fn max_iv_mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
        BattlePokemon::new(
            dex,
            SpeciesId(species),
            level,
            Nature::Hardy,
            Ivs {
                hp: 31,
                attack: 31,
                defense: 31,
                speed: 31,
                sp_attack: 31,
                sp_defense: 31,
            },
            0,
            moves,
        )
        .unwrap()
    }

    #[test]
    fn take_turn_after_the_battle_ended_is_an_error() {
        let dex = Dex::new();
        // Level 50 Charmander (fast, strong Tackle) vs level 2 Rattata: the
        // player one-shots it, so one turn reaches a terminal state.
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]); // Rattata

        // One RNG for the whole battle: battle-start turn number, then the
        // turn's own turn number, the opponent's move pick, and the player's
        // hit (accuracy / no crit / best roll). No speed-tie draw at this gap.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng);
        let _ = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert!(battle.outcome().is_some());
        assert_eq!(rng.draws(), 6);
        // The rejected call must not draw: the sequence is exhausted, so a
        // stray draw would panic rather than silently pass.
        assert_eq!(
            battle.take_turn(PlayerAction::UseMove(0), &mut rng),
            Err(BattleError::BattleAlreadyOver)
        );
        assert_eq!(rng.draws(), 6, "an already-over battle draws nothing");
    }

    #[test]
    fn battle_start_draws_the_initial_turn_order_tie_on_equal_speeds() {
        // `TryDoEventsBeforeFirstTurn` seeds the initial turn order with
        // `ignoreChosenMoves = TRUE` (`battle_main.c:3852`..`:3861`), so a
        // mirror match (identical species/level, all stages neutral) hits
        // the exact-Speed-tie draw (`:4745`..`:4750`) before turn 1.
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0x1234, 0]);
        let _battle = Battle::new(dex, player, enemy, &mut rng);
        assert_eq!(
            rng.draws(),
            2,
            "an exact Speed tie costs one extra pre-turn-1 draw"
        );
    }

    #[test]
    fn battle_start_and_every_turn_each_refresh_the_turn_number() {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        // Distinguishable turn-number values, then the ordinary tail of the
        // turn (opponent's move pick + the player's hit).
        let mut rng = SequenceRng::new([0x1234, 0xABCD, 0, 0, 1, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng);
        assert_eq!(
            battle.random_turn_number(),
            0x1234,
            "BattleStartClearSetData's draw (battle_main.c:3140)"
        );
        assert_eq!(
            rng.draws(),
            1,
            "Battle::new draws exactly once when speeds differ (no tie draw)"
        );
        let _ = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(
            battle.random_turn_number(),
            0xABCD,
            "the turn's own draw (battle_main.c:3923 / :4013) comes first"
        );
    }

    #[test]
    fn full_wild_battle_runs_to_a_faint_and_reports_victory() {
        let dex = Dex::new();
        // A genuinely multi-turn, evenly matched fight, hand computed from
        // the same formulas the unit tests pin:
        //
        //   player Rattata L5 max-IV Hardy: atk 12, def 10, speed 13, hp 19
        //   enemy Bulbasaur L5 max-IV Hardy: atk 11, def 11, speed 11, hp 21
        //
        // Rattata is faster, so it moves first every turn.
        //   Rattata's Tackle: 12*35=420, *4=1680, /11=152, /50=3, +2=5,
        //     STAB (Normal on a Normal-type) *15/10 = 7 per hit.
        //   Bulbasaur's Tackle: 11*35=385, *4=1540, /10=154, /50=3, +2=5,
        //     no STAB (Grass/Poison), Normal is neutral into both = 5.
        // So Bulbasaur (21 hp) falls on the third player hit (7/14/21) while
        // Rattata (19 hp) has taken two 5s and is at 9.
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
        let enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

        // One scripted RNG for the entire battle, in the module docs' order.
        // Per full turn: turn number, opponent's move pick, then two hits of
        // (accuracy / no crit / best roll) -- no speed-tie draw, the speeds
        // differ. The last turn stops after the player's hit: the enemy
        // faints, so the second mover never acts and never draws.
        let mut rng = SequenceRng::new([
            0, // Battle::new: battle-start turn number
            0, 0, 0, 1, 0, 0, 1, 0, // turn 1
            0, 0, 0, 1, 0, 0, 1, 0, // turn 2
            0, 0, 0, 1, 0, // turn 3: player's hit faints the enemy
        ]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng);

        let mut turns = 0;
        let mut won = false;
        // Cap the loop so a logic bug fails the test instead of hanging.
        for _ in 0..20 {
            let events = battle
                .take_turn(PlayerAction::UseMove(0), &mut rng)
                .unwrap();
            turns += 1;
            if let Some(BattleEvent::Ended(outcome)) = events.last() {
                assert_eq!(*outcome, BattleOutcome::PlayerWon);
                won = true;
                break;
            }
        }
        assert!(won, "battle did not conclude within 20 turns");
        assert_eq!(turns, 3, "three player hits of 7 to drop a 21-hp Bulbasaur");
        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
        assert_eq!(battle.enemy().current_hp, 0);
        assert_eq!(battle.player().current_hp, 9, "two enemy hits of 5 from 19");
        assert_eq!(
            rng.draws(),
            22,
            "1 (battle start) + 8 + 8 (full turns) + 5 (final turn)"
        );
    }

    #[test]
    fn a_successful_run_ends_the_battle_immediately_without_either_mon_acting() {
        let dex = Dex::new();
        // Player far faster than the enemy: try_run_from_battle succeeds
        // unconditionally (player_speed >= enemy_speed), no escape draw.
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let player_hp = player.current_hp;
        let enemy_hp = enemy.current_hp;

        // Battle-start turn number, the turn's turn number, and the wild
        // mon's move pick -- which happens even though it never gets to act,
        // because action selection completes for both battlers before the
        // run is resolved.
        let mut rng = SequenceRng::new([0, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng);
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events,
            vec![
                BattleEvent::RunAttempt {
                    by_player: true,
                    success: true,
                },
                BattleEvent::Ended(BattleOutcome::PlayerRan),
            ]
        );
        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
        assert_eq!(rng.draws(), 3, "no escape draw, but selection still drew");
        // Neither mon took any action/damage.
        assert_eq!(battle.player().current_hp, player_hp);
        assert_eq!(battle.enemy().current_hp, enemy_hp);
    }

    #[test]
    fn a_failed_run_burns_the_turn_and_the_enemy_still_acts() {
        let dex = Dex::new();
        // Player slower than the enemy: forces the RNG-driven branch, fed a
        // roll that fails (see crate::escape's own tests for the formula).
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow Rattata
        let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast Charmander

        // draws: battle-start turn number, turn number, opponent's move pick,
        // escape roll (65000 & 0xFF = 232 >= speedVar 19 -> failure), then
        // the enemy's hit (accuracy / no crit / best roll).
        let mut rng = SequenceRng::new([0, 0, 0, 65000, 0, 1, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng);
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events[0],
            BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            }
        );
        // The enemy's move resolved afterward (by_player: false).
        assert!(events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: false,
                ..
            }
        )));
        assert_eq!(battle.run_tries(), 1);
        assert_eq!(rng.draws(), 7);
    }

    #[test]
    fn the_wild_opponent_rejects_move_slots_it_does_not_know() {
        let dex = Dex::new();
        // A one-move wild mon: only a draw congruent to 0 mod 4 selects a
        // real slot, every other residue is upstream's MOVE_NONE and is
        // redrawn (battle_controller_opponent.c:1594-1601).
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: run succeeds
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0, 0, 1, 2, 3, 4]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng);
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events.last(),
            Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
        );
        assert_eq!(
            rng.draws(),
            6,
            "1 battle start + 1 turn number + 4 rejection-loop draws"
        );
    }

    #[test]
    fn the_wild_opponent_uses_the_slot_the_rejection_loop_landed_on() {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: the run fails
        let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]); // Tackle, Scratch

        // draw 1 -> 1 % 4 = 1, a slot this mon knows: Scratch, first try.
        let mut rng = SequenceRng::new([0, 0, 1, 65000, 0, 1, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng);
        let _ = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            battle.enemy().moves[0].pp,
            35,
            "Tackle was not the chosen slot, so its PP is untouched"
        );
        assert_eq!(
            battle.enemy().moves[1].pp,
            34,
            "Scratch (slot 1) was chosen and spent a PP"
        );
        assert_eq!(rng.draws(), 7);
    }
}
