//! Single-battle turn resolution.
//!
//! [`Battle`] supports ordinary wild encounters, the scripted first battle,
//! and trainer parties. An accepted turn chooses the opponent's action before
//! resolving a run or move order, skips queued actions once either battler
//! faints, settles the knockouts that leaves, and then applies end-of-turn
//! residuals in that same turn order.
//!
//! Construction consumes a turn-number draw and a conditional Speed-tie draw.
//! Each accepted turn consumes another turn-number draw before opponent action
//! selection. These positions preserve the shared upstream RNG sequence
//! (`src/battle_main.c:3140`, `:3852`-`:3861`, `:3923`, `:4013`).

use assets::trainers::TrainerId;
use assets::MoveId;

use crate::damage::{BattleRng, STRUGGLE};
use crate::defense_curl;
use crate::dex::Dex;
use crate::drain;
use crate::error::BattleError;
use crate::escape::try_run_from_battle;
use crate::exp::{trainer_faint_exp, wild_faint_exp};
use crate::fixed_damage;
use crate::flag_move;
use crate::multi_hit;
use crate::paralyze;
use crate::pokemon::{BattlePokemon, MoveLearnDecision, PendingMoveLearn, MAX_LEVEL};
use crate::secondary;
use crate::stat_change;
use crate::status1::{draws_full_paralysis, poison_residual_damage};
use crate::turn_order::{resolve_order, Order};

mod events;
mod execute;
pub(crate) mod opponent_ai;
pub mod trainer;
pub(crate) mod trainer_ai;

pub use events::{BattleEvent, TurnError};

use opponent_ai::{
    choose_enemy_action_first_battle, choose_enemy_move, selectable_slot, EnemyAction,
};
use trainer::TrainerContext;
use trainer_ai::choose_trainer_action;

const NO_MOVE_PRIORITY: i8 = 0;

/// A player's selected turn action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerAction {
    /// Use the move at this index in the active Pokémon's moveset.
    UseMove(usize),
    /// Attempt to run away.
    Run,
}

/// The result of a finished battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleOutcome {
    /// The opposing side has no usable Pokémon.
    PlayerWon,
    /// The player's active Pokémon fainted.
    PlayerLost,
    /// The player successfully ran away.
    PlayerRan,
    /// The opponent fled from the scripted first battle.
    WildFled,
}

/// Validates that one complete move-effect pipeline can execute a move.
pub(crate) fn ensure_executable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    // The ordinary-hit pipeline cannot apply Struggle's recoil half.
    if move_id == STRUGGLE {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    match crate::hit::ensure_resolvable(dex, move_id) {
        Ok(()) => Ok(()),
        Err(hit_error) => {
            let accepted_by_specialized_pipeline = stat_change::ensure_resolvable(dex, move_id)
                .is_ok()
                || drain::ensure_resolvable(dex, move_id).is_ok()
                || fixed_damage::ensure_resolvable(dex, move_id).is_ok()
                || multi_hit::ensure_resolvable(dex, move_id).is_ok()
                || flag_move::ensure_resolvable(dex, move_id).is_ok()
                || defense_curl::ensure_resolvable(dex, move_id).is_ok()
                || paralyze::ensure_resolvable(dex, move_id).is_ok();
            if accepted_by_specialized_pipeline {
                Ok(())
            } else {
                Err(hit_error)
            }
        }
    }
}

/// An owned single battle driven one turn at a time.
#[derive(Debug, Clone)]
pub struct Battle {
    dex: Dex,
    player: BattlePokemon,
    enemy: BattlePokemon,
    run_attempts: u8,
    random_turn_number: u16,
    outcome: Option<BattleOutcome>,
    kind: BattleKind,
    turn_counter: u8,
    turn_has_started: bool,
    pending_residual_order: Option<Order>,
}

#[derive(Debug, Clone)]
enum BattleKind {
    Wild,
    FirstBattle,
    Trainer(TrainerContext),
}

#[derive(Debug, Clone, Copy)]
enum ValidatedPlayerAction {
    UseMove { slot: usize, move_id: MoveId },
    Run,
}

fn initialize_turn_rng_state(
    player: &BattlePokemon,
    enemy: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> u16 {
    let random_turn_number = rng.next_u16();
    // Upstream seeds a MOVE_NONE order here. A Speed tie consumes RNG even
    // though the order is discarded (`src/battle_main.c:3852`-`:3861`).
    let _initial_order = resolve_order(
        NO_MOVE_PRIORITY,
        NO_MOVE_PRIORITY,
        player.speed_for_turn_order(),
        enemy.speed_for_turn_order(),
        rng,
    );
    random_turn_number
}

impl Battle {
    /// Starts an ordinary wild battle or the scripted first battle.
    ///
    /// The enemy's full moveset is validated before RNG is consumed because
    /// its action selection can choose any slot. Player moves are validated
    /// when selected. The scripted first battle suppresses critical hits,
    /// forbids running, and uses its dedicated opponent AI.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::FaintedBattler`] for a fainted participant or
    /// the first move or ability validation error from the enemy's moveset.
    /// Errors leave the RNG untouched.
    pub fn new(
        dex: Dex,
        player: BattlePokemon,
        enemy: BattlePokemon,
        first_battle: bool,
        rng: &mut impl BattleRng,
    ) -> Result<Self, BattleError> {
        if player.is_fainted() {
            return Err(BattleError::FaintedBattler(true));
        }
        if enemy.is_fainted() {
            return Err(BattleError::FaintedBattler(false));
        }
        for slot in enemy.moves() {
            ensure_executable(&dex, slot.move_id)?;
            // Zero-PP moves stop before applying effects
            // (`src/battle_script_commands.c:934`-`:939`).
            if slot.pp > 0 {
                paralyze::ensure_admissible(&dex, slot.move_id, &enemy, &player)?;
                secondary::ensure_admissible(&dex, slot.move_id, &enemy, &player)?;
            }
        }
        let random_turn_number = initialize_turn_rng_state(&player, &enemy, rng);
        Ok(Self {
            dex,
            player,
            enemy,
            run_attempts: 0,
            random_turn_number,
            outcome: None,
            kind: if first_battle {
                BattleKind::FirstBattle
            } else {
                BattleKind::Wild
            },
            turn_counter: 0,
            turn_has_started: false,
            pending_residual_order: None,
        })
    }

    /// Starts a trainer battle with `party[0]` active and the rest on the bench.
    ///
    /// Trainer metadata, AI flags, active battlers, and every party move are
    /// validated before this constructor consumes RNG.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::EmptyTrainerParty`] for an empty party,
    /// [`BattleError::UnknownTrainer`] for an unknown trainer,
    /// [`BattleError::FaintedBattler`] for a fainted active battler, or the
    /// first AI, move, or ability validation error. Errors leave the RNG untouched.
    pub fn new_trainer(
        dex: Dex,
        player: BattlePokemon,
        trainer: TrainerId,
        mut party: Vec<BattlePokemon>,
        rng: &mut impl BattleRng,
    ) -> Result<Self, BattleError> {
        if party.is_empty() {
            return Err(BattleError::EmptyTrainerParty(trainer));
        }
        let data = trainer::trainer_data(trainer)?;
        trainer_ai::ensure_supported_flags(data.ai_flags)?;
        if player.is_fainted() {
            return Err(BattleError::FaintedBattler(true));
        }
        if party[0].is_fainted() {
            return Err(BattleError::FaintedBattler(false));
        }
        for mon in &party {
            for slot in mon.moves() {
                trainer::ensure_move_playable(&dex, slot.move_id)?;
                if slot.pp > 0 {
                    paralyze::ensure_admissible(&dex, slot.move_id, mon, &player)?;
                    secondary::ensure_admissible(&dex, slot.move_id, mon, &player)?;
                }
            }
        }

        let enemy = party.remove(0);
        let random_turn_number = initialize_turn_rng_state(&player, &enemy, rng);
        Ok(Self {
            dex,
            player,
            enemy,
            run_attempts: 0,
            random_turn_number,
            outcome: None,
            kind: BattleKind::Trainer(TrainerContext::new(trainer, data, party)),
            turn_counter: 0,
            turn_has_started: false,
            pending_residual_order: None,
        })
    }

    /// Returns the opponent's trainer context, if this is a trainer battle.
    #[must_use]
    pub const fn trainer(&self) -> Option<&TrainerContext> {
        match &self.kind {
            BattleKind::Trainer(context) => Some(context),
            BattleKind::Wild | BattleKind::FirstBattle => None,
        }
    }

    const fn is_first_battle(&self) -> bool {
        matches!(self.kind, BattleKind::FirstBattle)
    }

    /// Returns the zero-based battle-turn counter used by trainer AI.
    #[must_use]
    pub const fn turn_counter(&self) -> u8 {
        self.turn_counter
    }

    /// Returns the player's active Pokémon.
    #[must_use]
    pub const fn player(&self) -> &BattlePokemon {
        &self.player
    }

    /// Returns the opponent's active Pokémon.
    #[must_use]
    pub const fn enemy(&self) -> &BattlePokemon {
        &self.enemy
    }

    /// Returns the outcome once the battle has ended.
    #[must_use]
    pub const fn outcome(&self) -> Option<BattleOutcome> {
        self.outcome
    }

    /// Returns the level-up move waiting for a learn or decline decision.
    ///
    /// A pending decision blocks another turn and defers the battle outcome or
    /// trainer replacement until [`Battle::resolve_move_learn`] resolves it.
    #[must_use]
    pub const fn pending_move_learn(&self) -> Option<PendingMoveLearn> {
        self.player.pending_move_learn()
    }

    /// Resolves the pending move-learning decision and returns its ordered events.
    ///
    /// Another prompt may follow immediately. Resolving the final prompt also
    /// releases any deferred replacement, prize money, and battle outcome, and
    /// then runs the residual pass the prompt held back.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::NoMoveLearnPending`] when no decision is waiting,
    /// [`BattleError::InvalidMoveSlot`] for an invalid replacement slot, or
    /// [`BattleError::HmMoveCantBeForgotten`] for an HM replacement; all three
    /// preserve the pending decision. Returns [`BattleError::UnknownSpecies`]
    /// when the released residual pass fells a battler whose reward needs a
    /// dex lookup.
    pub fn resolve_move_learn(
        &mut self,
        decision: MoveLearnDecision,
    ) -> Result<Vec<BattleEvent>, BattleError> {
        let asked_move = self
            .player
            .pending_move_learn()
            .ok_or(BattleError::NoMoveLearnPending)?
            .move_id();
        let resolution = self.player.resolve_move_learn(&self.dex, decision)?;

        let mut events = Vec::new();
        match resolution.learned {
            Some(learned) => events.push(BattleEvent::MoveReplaced {
                learned: learned.move_id,
                forgotten: learned.forgotten,
                slot: learned.slot,
            }),
            None => events.push(BattleEvent::MoveLearnDeclined {
                move_id: asked_move,
            }),
        }
        if let Some(next) = resolution.next {
            events.push(BattleEvent::MoveLearnPrompt {
                move_id: next.move_id(),
            });
        } else {
            self.settle_fainted_enemy(&mut events)?;
            // Upstream's yes/no box sits inside `HandleFaintedMonActions`,
            // which every path to the residual pass crosses first
            // (`src/battle_util.c:1912`-`:1923`).
            if let Some(order) = self.pending_residual_order.take() {
                self.residual_effects(order, &mut events);
                self.handle_fainted_mons(&mut events)?;
            }
        }
        Ok(events)
    }

    /// Returns the number of run attempts made in this battle.
    #[must_use]
    pub const fn run_tries(&self) -> u8 {
        self.run_attempts
    }

    /// Returns the turn-number draw captured during construction or turn start.
    #[must_use]
    pub const fn random_turn_number(&self) -> u16 {
        self.random_turn_number
    }

    fn validate_player_move(&self, index: usize) -> Result<MoveId, BattleError> {
        let slot = self
            .player
            .moves()
            .get(index)
            .ok_or(BattleError::InvalidMoveSlot(index))?;
        if !selectable_slot(Some(slot.move_id)) {
            return Err(BattleError::PlaceholderMove(index));
        }
        if slot.pp == 0 {
            return Err(BattleError::NoPpRemaining(index));
        }
        ensure_executable(&self.dex, slot.move_id)?;
        paralyze::ensure_admissible(&self.dex, slot.move_id, &self.player, &self.enemy)?;
        secondary::ensure_admissible(&self.dex, slot.move_id, &self.player, &self.enemy)?;
        Ok(slot.move_id)
    }

    /// Resolves one player action and returns the resulting events in order.
    ///
    /// The action is validated before RNG is consumed. The opponent then chooses
    /// an action before a run attempt or move ordering resolves.
    ///
    /// # Errors
    ///
    /// Returns [`TurnError`] for an invalid state or action. Pre-turn failures
    /// consume no RNG and contain no events. A wild opponent forced to use the
    /// unsupported Struggle may fail after draws or earlier events; those events
    /// remain in [`TurnError::events`]. A pending move-learning decision takes
    /// precedence over an existing outcome.
    pub fn take_turn(
        &mut self,
        player_action: PlayerAction,
        rng: &mut impl BattleRng,
    ) -> Result<Vec<BattleEvent>, TurnError> {
        let mut events = Vec::new();
        match self.resolve_turn(player_action, rng, &mut events) {
            Ok(()) => Ok(events),
            Err(error) => Err(TurnError { events, error }),
        }
    }

    fn resolve_turn(
        &mut self,
        player_action: PlayerAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let player_action = self.validate_player_action(player_action)?;
        self.start_turn(rng);
        let enemy_action = self.choose_enemy_action(rng)?;

        let order = match player_action {
            ValidatedPlayerAction::UseMove { slot, move_id } => {
                self.resolve_move_exchange(slot, move_id, enemy_action, rng, events)?
            }
            ValidatedPlayerAction::Run => {
                self.resolve_run_attempt(enemy_action, rng, events)?;
                if self.outcome.is_some() {
                    return Ok(());
                }
                // A chosen run always takes the first slot in
                // `gBattlerByTurnOrder` (`src/battle_main.c:4797`-`:4808`).
                Order::AttackerFirst
            }
        };

        self.pass_turn(order, events)
    }

    fn validate_player_action(
        &self,
        action: PlayerAction,
    ) -> Result<ValidatedPlayerAction, BattleError> {
        if let Some(pending) = self.player.pending_move_learn() {
            return Err(BattleError::MoveLearnPending(pending.move_id()));
        }
        if self.outcome.is_some() {
            return Err(BattleError::BattleAlreadyOver);
        }

        match action {
            PlayerAction::Run => match self.kind {
                BattleKind::Trainer(_) => Err(BattleError::NoRunningFromTrainer),
                BattleKind::FirstBattle => Err(BattleError::RunForbidden),
                BattleKind::Wild => Ok(ValidatedPlayerAction::Run),
            },
            PlayerAction::UseMove(slot) => Ok(ValidatedPlayerAction::UseMove {
                slot,
                move_id: self.validate_player_move(slot)?,
            }),
        }
    }

    fn start_turn(&mut self, rng: &mut impl BattleRng) {
        if self.turn_has_started {
            self.turn_counter = self.turn_counter.saturating_add(1);
        }
        self.turn_has_started = true;
        self.random_turn_number = rng.next_u16();
    }

    fn choose_enemy_action(&self, rng: &mut impl BattleRng) -> Result<EnemyAction, BattleError> {
        match &self.kind {
            BattleKind::FirstBattle => Ok(choose_enemy_action_first_battle(
                &self.enemy,
                &self.player,
                rng,
            )),
            BattleKind::Trainer(context) => choose_trainer_action(
                &self.dex,
                &self.enemy,
                &self.player,
                context.ai_flags(),
                self.turn_counter,
                rng,
            ),
            BattleKind::Wild => Ok(match choose_enemy_move(&self.enemy, rng) {
                Some(index) => EnemyAction::Move(index),
                None => EnemyAction::Struggle,
            }),
        }
    }

    fn resolve_run_attempt(
        &mut self,
        enemy_action: EnemyAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        // Escape uses raw battle stats, not stage-modified turn-order Speed
        // (`src/battle_util.c:463`-`:465`).
        let success = try_run_from_battle(
            self.player.stats().speed,
            self.enemy.stats().speed,
            self.run_attempts,
            rng,
        );
        self.run_attempts = self.run_attempts.wrapping_add(1);
        events.push(BattleEvent::RunAttempt {
            by_player: true,
            success,
        });
        if success {
            self.finish(events, BattleOutcome::PlayerRan);
            return Ok(());
        }
        self.resolve_enemy_action(enemy_action, rng, events)
    }

    fn resolve_move_exchange(
        &mut self,
        player_slot: usize,
        player_move: MoveId,
        enemy_action: EnemyAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<Order, BattleError> {
        let player_priority = self.dex.move_data(player_move)?.priority;
        let enemy_priority = match enemy_action {
            EnemyAction::Move(slot) => {
                self.dex
                    .move_data(self.enemy.moves()[slot].move_id)?
                    .priority
            }
            EnemyAction::Struggle => self.dex.move_data(STRUGGLE)?.priority,
            EnemyAction::Flee => NO_MOVE_PRIORITY,
        };
        let order = resolve_order(
            player_priority,
            enemy_priority,
            self.player.speed_for_turn_order(),
            self.enemy.speed_for_turn_order(),
            rng,
        );

        match order {
            Order::AttackerFirst => {
                self.act(true, player_move, player_slot, rng, events)?;
                if self.both_battlers_can_act() {
                    self.resolve_enemy_action(enemy_action, rng, events)?;
                }
            }
            Order::DefenderFirst => {
                self.resolve_enemy_action(enemy_action, rng, events)?;
                if self.both_battlers_can_act() {
                    self.act(true, player_move, player_slot, rng, events)?;
                }
            }
        }
        Ok(order)
    }

    fn both_battlers_can_act(&self) -> bool {
        self.outcome.is_none() && !self.player.is_fainted() && !self.enemy.is_fainted()
    }

    fn pass_turn(
        &mut self,
        order: Order,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        // Upstream reaches `HandleFaintedMonActions` twice a turn: from each
        // action's own `Cmd_end`, and again behind `BattleTurnPassed`'s
        // residual pass (`src/battle_main.c:549`, `:3960`-`:3968`).
        self.handle_fainted_mons(events)?;
        if self.player.pending_move_learn().is_some() {
            self.pending_residual_order = Some(order);
            return Ok(());
        }
        self.residual_effects(order, events);
        self.handle_fainted_mons(events)
    }

    fn residual_effects(&mut self, order: Order, events: &mut Vec<BattleEvent>) {
        if self.outcome.is_some() {
            return;
        }
        // `DoBattlerEndTurnEffects` walks `gBattlerByTurnOrder` -- this turn's
        // own move order -- and reaches ENDTURN_POISON before ENDTURN_CHARGE
        // within each battler's pass (`src/battle_util.c:1442`-`:1474`).
        let player_first = matches!(order, Order::AttackerFirst);
        for is_player in [player_first, !player_first] {
            let already_fainted = if is_player {
                self.player.is_fainted()
            } else {
                self.enemy.is_fainted()
            };
            if already_fainted {
                continue;
            }
            self.apply_poison_residual(is_player, events);
            if is_player {
                self.player.volatiles_mut().tick_charge();
            } else {
                self.enemy.volatiles_mut().tick_charge();
            }
            let now_fainted = if is_player {
                self.player.is_fainted()
            } else {
                self.enemy.is_fainted()
            };
            if now_fainted {
                self.settle_faint(is_player, events);
                if self.fainting_decides_the_battle(is_player) {
                    break;
                }
            }
        }
    }

    fn fainting_decides_the_battle(&self, is_player: bool) -> bool {
        // `Cmd_checkteamslost` totals a whole party's HP
        // (`src/battle_script_commands.c:3534`-`:3577`); this crate benches
        // nothing for the player, so any player faint exhausts that side.
        if is_player {
            return true;
        }
        match &self.kind {
            BattleKind::Trainer(context) => context.bench().iter().all(BattlePokemon::is_fainted),
            BattleKind::Wild | BattleKind::FirstBattle => true,
        }
    }

    fn apply_poison_residual(&mut self, is_player: bool, events: &mut Vec<BattleEvent>) {
        let battler = if is_player { &self.player } else { &self.enemy };
        if battler.is_fainted() || !battler.status1().is_poisoned() {
            return;
        }
        let damage = poison_residual_damage(battler.stats().max_hp);
        let target = if is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        let dealt = damage.min(target.current_hp());
        target.apply_damage(dealt);
        events.push(BattleEvent::HurtByPoison {
            by_player: is_player,
            damage: dealt,
        });
    }

    fn handle_fainted_mons(&mut self, events: &mut Vec<BattleEvent>) -> Result<(), BattleError> {
        if self.outcome.is_some() || self.player.pending_move_learn().is_some() {
            return Ok(());
        }
        // A double faint sets both of `Cmd_checkteamslost`'s outcome bits at
        // once, and BattleScript_HandleFaintedMon then skips the reward and
        // the switch-in entirely (`data/battle_scripts_1.s:2831`-`:2832`).
        if self.player.is_fainted() {
            self.finish(events, BattleOutcome::PlayerLost);
            return Ok(());
        }
        if !self.enemy.is_fainted() {
            return Ok(());
        }
        // Case 1 runs `BattleScript_GiveExp` to completion, the yes/no box
        // included, before case 4 replaces or pays out
        // (`src/battle_util.c:1912`-`:1946`).
        self.settle_enemy_reward(events)?;
        if self.player.pending_move_learn().is_some() {
            return Ok(());
        }
        self.settle_fainted_enemy(events)?;
        Ok(())
    }

    fn settle_enemy_reward(&mut self, events: &mut Vec<BattleEvent>) -> Result<(), BattleError> {
        // `Cmd_getexp` case 2 zeroes the award and jumps past both the string
        // and `MonGainEVs` for a recipient already at the cap
        // (`src/battle_script_commands.c:3351`-`:3356`).
        if self.player.level() >= MAX_LEVEL {
            return Ok(());
        }
        let defeated = self.dex.species(self.enemy.species())?;
        let level = self.enemy.level();
        let exp = if self.trainer().is_some() {
            trainer_faint_exp(defeated.base_exp, level)
        } else {
            wild_faint_exp(defeated.base_exp, level)
        };
        // `MonGainEVs` runs ahead of the exp and level-up sequence, so a
        // level crossed this turn snapshots the gain
        // (`src/battle_script_commands.c:3420`).
        self.player.gain_evs(defeated.ev_yield);
        let pending = self.player.apply_experience(&self.dex, exp)?;
        events.push(BattleEvent::ExpGained(exp));
        if let Some(prompt) = pending {
            events.push(BattleEvent::MoveLearnPrompt {
                move_id: prompt.move_id(),
            });
        }
        Ok(())
    }

    fn settle_fainted_enemy(&mut self, events: &mut Vec<BattleEvent>) -> Result<(), BattleError> {
        if self.outcome.is_some() || !self.enemy.is_fainted() {
            return Ok(());
        }
        let BattleKind::Trainer(context) = &mut self.kind else {
            self.finish(events, BattleOutcome::PlayerWon);
            return Ok(());
        };
        if let Some(next) = context.send_out_next(&self.dex, &self.enemy, &self.player)? {
            let species = next.species();
            let bench_remaining = context.bench_len();
            self.enemy = next;
            events.push(BattleEvent::TrainerSentOut {
                species,
                bench_remaining,
            });
            return Ok(());
        }
        let money = context.money();
        events.push(BattleEvent::MoneyGained(money));
        self.finish(events, BattleOutcome::PlayerWon);
        Ok(())
    }

    fn act(
        &mut self,
        player_is_attacker: bool,
        move_id: MoveId,
        slot: usize,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let attacker_status1 = if player_is_attacker {
            self.player.status1()
        } else {
            self.enemy.status1()
        };
        if draws_full_paralysis(attacker_status1, rng) {
            events.push(BattleEvent::FullyParalyzed {
                by_player: player_is_attacker,
                move_id,
            });
            return Ok(());
        }
        if player_is_attacker {
            self.player.deduct_pp(slot)?;
        } else if self.enemy.moves()[slot].pp == 0 {
            events.push(BattleEvent::FailedNoPp {
                by_player: false,
                move_id,
            });
            return Ok(());
        } else {
            self.enemy.deduct_pp(slot)?;
        }
        self.execute_move(player_is_attacker, move_id, rng, events)
    }

    fn resolve_enemy_action(
        &mut self,
        action: EnemyAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        match action {
            EnemyAction::Move(slot) => {
                self.act(false, self.enemy.moves()[slot].move_id, slot, rng, events)
            }
            EnemyAction::Struggle => {
                // Struggle reaches the paralysis gate before its unsupported
                // recoil path (`data/battle_scripts_1.s:241`-`:247`).
                if draws_full_paralysis(self.enemy.status1(), rng) {
                    events.push(BattleEvent::FullyParalyzed {
                        by_player: false,
                        move_id: STRUGGLE,
                    });
                    Ok(())
                } else {
                    Err(BattleError::UnsupportedMoveEffect(STRUGGLE))
                }
            }
            EnemyAction::Flee => {
                events.push(BattleEvent::WildFled);
                self.finish(events, BattleOutcome::WildFled);
                Ok(())
            }
        }
    }

    fn finish(&mut self, events: &mut Vec<BattleEvent>, outcome: BattleOutcome) {
        self.outcome = Some(outcome);
        events.push(BattleEvent::Ended(outcome));
    }
}

#[cfg(test)]
mod tests {
    use assets::trainers::AiFlags;
    use assets::SpeciesId;

    use super::*;
    use crate::pokemon::Ivs;
    use crate::script_rng::SequenceRng;

    const MAX_IVS: Ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };
    const ABSORB: MoveId = MoveId(71);
    const TACKLE: MoveId = MoveId(33);
    const BULBASAUR: SpeciesId = SpeciesId(1);
    const SQUIRTLE: SpeciesId = SpeciesId(7);
    const TENTACOOL: SpeciesId = SpeciesId(72);
    const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
    const ENEMY_LEVEL: u8 = 50;
    const PLAYER_LEVEL: u8 = 30;
    const ENEMY_REMAINING_HP: u32 = 5;
    const ENEMY_SPEED: u32 = 65;
    const PLAYER_SPEED: u32 = 56;
    const LIQUID_OOZE_PERSONALITY: u32 = 1;
    const PLAYER_MOVE_SLOT: usize = 0;
    const DRAWS_THROUGH_ENEMY_ABSORB: usize = 9;

    fn trainer_battle_with_faster_absorb_user() -> (Battle, u32, u8) {
        let dex = Dex::new();
        let mut enemy = BattlePokemon::new(&dex, BULBASAUR, ENEMY_LEVEL, MAX_IVS, 0, vec![ABSORB])
            .expect("valid enemy");
        assert_eq!(enemy.stats().speed, ENEMY_SPEED);
        enemy.apply_damage(enemy.stats().max_hp - ENEMY_REMAINING_HP);
        let player = BattlePokemon::new(
            &dex,
            TENTACOOL,
            PLAYER_LEVEL,
            MAX_IVS,
            LIQUID_OOZE_PERSONALITY,
            vec![TACKLE],
        )
        .expect("valid player");
        assert_eq!(player.stats().speed, PLAYER_SPEED);
        assert_eq!(player.ability(), crate::ability::LIQUID_OOZE);
        let player_max_hp = player.stats().max_hp;

        let mut trainer_without_ai =
            *trainer::trainer_data(MAY_ROUTE_103_MUDKIP).expect("known trainer");
        trainer_without_ai.ai_flags = AiFlags::NONE;
        let bench =
            vec![
                BattlePokemon::new(&dex, SQUIRTLE, PLAYER_LEVEL, MAX_IVS, 0, vec![TACKLE])
                    .expect("valid bench mon"),
            ];
        let trainer = TrainerContext::new(MAY_ROUTE_103_MUDKIP, &trainer_without_ai, bench);
        let player_move_max_pp = dex.move_data(TACKLE).expect("known move").pp;

        let battle = Battle {
            dex,
            player,
            enemy,
            run_attempts: 0,
            random_turn_number: 0,
            outcome: None,
            kind: BattleKind::Trainer(trainer),
            turn_counter: 0,
            turn_has_started: false,
            pending_residual_order: None,
        };
        (battle, player_max_hp, player_move_max_pp)
    }

    #[test]
    fn liquid_ooze_recoil_ko_skips_the_players_queued_move() {
        let (mut battle, player_max_hp, player_move_max_pp) =
            trainer_battle_with_faster_absorb_user();

        let mut rng = SequenceRng::new([0; DRAWS_THROUGH_ENEMY_ABSORB]);
        let events = battle
            .take_turn(PlayerAction::UseMove(PLAYER_MOVE_SLOT), &mut rng)
            .unwrap();

        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, BattleEvent::Fainted { by_player: false }))
                .count(),
            1,
            "the enemy should faint exactly once: {events:?}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, BattleEvent::ExpGained(_)))
                .count(),
            1,
            "experience should be awarded exactly once: {events:?}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(
                    e,
                    BattleEvent::Hit {
                        by_player: true,
                        ..
                    }
                ))
                .count(),
            0,
            "the player's queued move should not execute: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, BattleEvent::Fainted { by_player: true })),
            "the player never fainted here: {events:?}"
        );
        let player_damage_taken: u32 = events
            .iter()
            .filter_map(|e| match e {
                BattleEvent::Hit {
                    by_player: false,
                    damage,
                    ..
                } => Some(*damage),
                _ => None,
            })
            .sum();
        assert_eq!(
            battle.player().current_hp(),
            player_max_hp - player_damage_taken,
            "only the enemy's hit should reduce player HP: {events:?}"
        );
        assert_eq!(
            battle.player().moves()[PLAYER_MOVE_SLOT].pp,
            player_move_max_pp,
            "the skipped move should preserve PP"
        );
        assert_eq!(
            battle.outcome(),
            None,
            "the trainer's bench is not empty, so the battle continues"
        );
        assert!(
            events.contains(&BattleEvent::TrainerSentOut {
                species: SQUIRTLE,
                bench_remaining: 0,
            }),
            "the replacement should enter after the turn: {events:?}"
        );
        assert_eq!(
            battle.enemy().species(),
            SQUIRTLE,
            "the replacement is active by the time `take_turn` returns"
        );
    }
}
