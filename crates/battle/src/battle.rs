//! Single-battle turn resolution.
//!
//! [`Battle`] supports ordinary wild encounters, the scripted first battle,
//! and trainer parties. An accepted turn chooses the opponent's action before
//! resolving a run or move order, skips queued actions once either battler
//! faints, settles the knockouts that leaves, and then applies end-of-turn
//! residuals in an end-turn order rebuilt from current Speed.
//!
//! Construction consumes a turn-number draw and a conditional Speed-tie draw.
//! Each accepted turn consumes another turn-number draw before opponent action
//! selection. These positions preserve the shared upstream RNG sequence
//! (`src/battle_main.c:3140`, `:3852`-`:3861`, `:3923`, `:4013`).
//!
//! `Battle` stays the orchestration boundary: it owns the state, the public
//! accessors, action validation, and the turn sequence. Each owned concept
//! lives in a child module: `constructors` (admission and starting state),
//! `actions` (run and move-slot execution), `residual` (end-of-turn effects),
//! `replacement` (fainted-party disposition and player reserves), `rewards`
//! (defeated-opponent settlement), and `execute` (move effects).

use assets::{AbilityId, MoveId};

use crate::damage::{BattleRng, STRUGGLE};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::escape::ensure_admissible;
use crate::pokemon::{BattlePokemon, MoveLearnDecision, PendingMoveLearn, MOVE_NONE};
use crate::secondary;

mod actions;
pub(crate) mod admission;
pub mod battler_state;
mod constructors;
mod events;
mod execute;
pub(crate) mod held_item;
pub(crate) mod opponent_ai;
mod replacement;
mod residual;
mod rewards;
#[cfg(test)]
mod switch_tests;
pub mod trainer;
pub(crate) mod trainer_ai;

pub use battler_state::{BattlerSide, BattlerState, LastLandedMove, PerishSongTurns};
pub use events::{BattleEvent, TurnError};

use opponent_ai::{
    all_known_moves_are_spent, choose_enemy_action_first_battle, choose_enemy_move,
    selectable_slot, EnemyAction,
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
    /// The player's party has no usable Pokémon: the active member and every
    /// reserve have fainted (`Cmd_checkteamslost`,
    /// `src/battle_script_commands.c:3534`-`:3564`).
    PlayerLost,
    /// The player successfully ran away.
    PlayerRan,
    /// The opponent fled from the scripted first battle.
    WildFled,
}

/// Validates that one complete move-effect pipeline can execute a move.
pub(crate) fn ensure_executable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    execute::MovePipeline::for_move(dex, move_id).map(|_| ())
}

/// An owned single battle driven one turn at a time.
#[derive(Debug, Clone)]
pub struct Battle {
    dex: Dex,
    player: BattlePokemon,
    /// The player's party members other than [`Battle::player`], always in
    /// the order the owning flow passed them.
    /// [`Battle::send_out_next_player_reserve`] returns a fainted active
    /// member to its own position here rather than swapping it into the
    /// sent-out reserve's, so the party order never changes across
    /// replacements, as upstream's `gPlayerParty` records keep their slots
    /// while `gBattlerPartyIndexes` names the active one.
    player_reserves: Vec<BattlePokemon>,
    /// The position of [`Battle::player`] in the party as passed to
    /// [`Battle::new_with_player_reserves`] or
    /// [`Battle::new_trainer_with_player_reserves`]: `0` until a replacement,
    /// and never beyond `player_reserves.len()`. The player-side analogue of
    /// upstream's `gBattlerPartyIndexes` entry for the player's battler.
    player_slot: usize,
    enemy: BattlePokemon,
    run_attempts: u8,
    random_turn_number: u16,
    outcome: Option<BattleOutcome>,
    kind: BattleKind,
    turn_counter: u8,
    turn_has_started: bool,
    pending_residual_selection: Option<ChosenMoves>,
    /// Each battler's last actually chosen move slot, `[player, enemy]`:
    /// upstream's `chosenMovePositions`: zero-initialized and never assigned
    /// for a forced Struggle (`pokeemerald/src/battle_main.c:4183`-`:4189`).
    retained_slots: [usize; 2],
    /// `gCurrentMove` (`pokeemerald/include/battle.h`): the most recently
    /// attempted move, `MOVE_NONE` before any has been. A forced trainer
    /// replacement's most-damage fallback reads this stale value as its
    /// base-damage move, matching upstream
    /// (`pokeemerald/src/battle_ai_switch_items.c:772`-`:779`).
    last_move_used: MoveId,
    /// Move history and Perish Song state per battler, `[player, enemy]`,
    /// reset whenever a battler leaves or enters the field.
    battler_states: [BattlerState; 2],
}

#[derive(Debug, Clone)]
enum BattleKind {
    Wild,
    FirstBattle,
    Trainer(TrainerContext),
}

/// What one battler selected this turn, as `GetWhoStrikesFirst(.., FALSE)`
/// still sees it when the end-turn pass rebuilds its order: the chosen move
/// position, read from whichever battler holds it at that point, so a faint
/// replacement or a spent slot changes the priority
/// (`pokeemerald/src/battle_util.c:1199`-`:1210`,
/// `pokeemerald/src/battle_main.c:4690`-`:4748`).
#[derive(Debug, Clone, Copy)]
enum Selection {
    /// A run or flee: no move is selected.
    NoMove,
    /// The move slot chosen at selection.
    Slot(usize),
    /// A forced Struggle: the end-turn comparison reads the battler's
    /// retained slot (`pokeemerald/src/battle_util.c:100`-`:104`).
    Struggle,
}

/// Both battlers' selections. Only the selection is retained from the action
/// phase, never which battler won it.
#[derive(Debug, Clone, Copy)]
struct ChosenMoves {
    player: Selection,
    enemy: Selection,
}

/// A resolved priority pair for one comparison.
#[derive(Debug, Clone, Copy)]
struct ResidualPriorities {
    player: i8,
    enemy: i8,
}

#[derive(Debug, Clone, Copy)]
enum ValidatedPlayerAction {
    /// `slot` is `None` for a forced Struggle: every known move is spent, so
    /// upstream substitutes `MOVE_STRUGGLE` at selection with no PP slot to
    /// deduct from (`AreAllMovesUnusable`, `pokeemerald/src/battle_util.c:1125`-
    /// `:1139`, `:100`-`:104`).
    UseMove {
        slot: Option<usize>,
        move_id: MoveId,
    },
    Run,
}

impl Battle {
    /// Returns the opponent's trainer context, if this is a trainer battle.
    #[must_use]
    pub const fn trainer(&self) -> Option<&TrainerContext> {
        match &self.kind {
            BattleKind::Trainer(context) => Some(context),
            BattleKind::Wild | BattleKind::FirstBattle => None,
        }
    }

    /// Returns a battler's move history and Perish Song state.
    #[must_use]
    pub const fn battler_state(&self, side: BattlerSide) -> &BattlerState {
        &self.battler_states[side.index()]
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

    /// Returns every player-side party member in the order it was passed to
    /// [`Battle::new_with_player_reserves`] or
    /// [`Battle::new_trainer_with_player_reserves`] -- the starting active
    /// member first, then each reserve -- whichever member is active now.
    ///
    /// The position is the stable link back to the caller's party slot: it
    /// survives every replacement, so two members that share species,
    /// personality, and original trainer id still reconcile to their own
    /// slots. Each entry carries its final HP, PP, and experience state;
    /// its identity ([`BattlePokemon::personality`],
    /// [`BattlePokemon::original_trainer_id`]) remains available for the
    /// owning flow to check each slot's record against, as it already does
    /// for a single lead.
    pub fn player_members(&self) -> impl Iterator<Item = &BattlePokemon> {
        let (before, after) = self.player_reserves.split_at(self.player_slot);
        before
            .iter()
            .chain(std::iter::once(&self.player))
            .chain(after.iter())
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
    /// then runs the residual pass the prompt held back — `rng` feeds that
    /// released pass, at the stream position it would have drawn from had no
    /// prompt deferred it.
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
        rng: &mut impl BattleRng,
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
            if let Some(chosen) = self.pending_residual_selection.take() {
                // Upstream zeroes `gCurrentMove` as each action closes, before
                // the residual pass runs, regardless of a move-learn prompt
                // deferring that pass to this later call
                // (`pokeemerald/src/battle_util.c:658`-`:671`).
                self.last_move_used = MOVE_NONE;
                self.residual_effects(chosen, &mut events, rng)?;
                self.handle_fainted_mons(&mut events)?;
            }
        }
        Ok(events)
    }

    /// Returns the number of ordinary run attempts made in this battle. A
    /// Run Away holder's escape does not advance this counter
    /// (`pokeemerald/src/battle_util.c:427`-`:447`, `:475`).
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
        // `Cmd_attackcanceler`'s no-PP abort exempts Struggle
        // (`battle_script_commands.c:934`).
        if slot.pp == 0 && slot.move_id != STRUGGLE {
            return Err(BattleError::NoPpRemaining(index));
        }
        ensure_executable(&self.dex, slot.move_id)?;
        secondary::ensure_admissible(&self.dex, slot.move_id, &self.player, &self.enemy)?;
        Ok(slot.move_id)
    }

    /// Re-screens the enemy's moveset with [`secondary::ensure_admissible`],
    /// the same PP gate construction used, against the current battlers
    /// rather than the ones construction saw; see the crate root docs for why.
    fn revalidate_enemy_admission(&self) -> Result<(), BattleError> {
        for slot in self.enemy.moves() {
            if slot.pp > 0 {
                secondary::ensure_admissible(&self.dex, slot.move_id, &self.enemy, &self.player)?;
            }
        }
        Ok(())
    }

    /// Resolves one player action and returns the resulting events in order.
    ///
    /// The action is validated before RNG is consumed. The opponent then chooses
    /// an action before a run attempt or move ordering resolves.
    ///
    /// # Errors
    ///
    /// Returns [`TurnError`] for an invalid state or action. Pre-turn failures
    /// consume no RNG and contain no events. A pending move-learning decision
    /// takes precedence over an existing outcome.
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

    /// Run Away or equal-or-greater raw Speed escapes without a draw and ends
    /// the turn before the enemy acts (`pokeemerald/src/battle_util.c:427-447,463-481`).
    fn escape_is_guaranteed(&self, action: ValidatedPlayerAction) -> bool {
        matches!(action, ValidatedPlayerAction::Run)
            && (self.player.ability() == AbilityId::RUN_AWAY
                || self.player.stats().speed >= self.enemy.stats().speed)
    }

    fn resolve_turn(
        &mut self,
        player_action: PlayerAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let player_action = self.validate_player_action(player_action)?;
        if !self.escape_is_guaranteed(player_action) {
            self.revalidate_enemy_admission()?;
        }
        self.start_turn(rng);
        let enemy_action = self.choose_enemy_action(rng)?;
        self.resolve_selected_actions(player_action, enemy_action, rng, events)
    }

    /// Resolves the turn's two selected actions, then the end-of-turn pass.
    fn resolve_selected_actions(
        &mut self,
        player_action: ValidatedPlayerAction,
        enemy_action: EnemyAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        if let (ValidatedPlayerAction::UseMove { move_id, .. }, EnemyAction::Switch(slot)) =
            (player_action, enemy_action)
        {
            // The player's move was admitted against the outgoing defender.
            let incoming = match &self.kind {
                BattleKind::Trainer(context) => context.switch_target(slot)?,
                BattleKind::Wild | BattleKind::FirstBattle => {
                    return Err(BattleError::InvalidTrainerSwitch(slot));
                }
            };
            secondary::ensure_admissible(&self.dex, move_id, &self.player, incoming)?;
        }
        let chosen = ChosenMoves {
            player: match player_action {
                ValidatedPlayerAction::UseMove {
                    slot: Some(slot), ..
                } => Selection::Slot(slot),
                ValidatedPlayerAction::UseMove { slot: None, .. } => Selection::Struggle,
                ValidatedPlayerAction::Run => Selection::NoMove,
            },
            enemy: match enemy_action {
                EnemyAction::Move(slot) => Selection::Slot(slot),
                EnemyAction::Struggle => Selection::Struggle,
                EnemyAction::Flee | EnemyAction::Switch(_) => Selection::NoMove,
            },
        };
        if let Selection::Slot(slot) = chosen.player {
            self.retained_slots[0] = slot;
        }
        if let Selection::Slot(slot) = chosen.enemy {
            self.retained_slots[1] = slot;
        }
        let priorities = self.priorities_of(chosen, false)?;
        match player_action {
            ValidatedPlayerAction::UseMove { slot, move_id, .. } => {
                self.resolve_move_exchange(slot, move_id, priorities, enemy_action, rng, events)?;
            }
            ValidatedPlayerAction::Run => {
                self.resolve_run_attempt(enemy_action, rng, events)?;
                if self.outcome.is_some() {
                    return Ok(());
                }
            }
        }

        self.pass_turn(chosen, events, rng)
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
                // Run Away is admitted before this refusal
                // (`pokeemerald/src/battle_main.c:4038`-`:4039`, `:4078`-`:4082`).
                BattleKind::FirstBattle => {
                    if self.player.ability() == AbilityId::RUN_AWAY {
                        Ok(ValidatedPlayerAction::Run)
                    } else {
                        Err(BattleError::RunForbidden)
                    }
                }
                BattleKind::Wild => {
                    ensure_admissible(&self.player, &self.enemy)?;
                    Ok(ValidatedPlayerAction::Run)
                }
            },
            // `AreAllMovesUnusable` (`pokeemerald/src/battle_util.c:1125`-
            // `:1139`) forces Struggle before the chosen slot is read; the
            // cursor can only name an existing slot
            // (`src/battle_controller_player.c:552`-`:597`).
            PlayerAction::UseMove(slot) if all_known_moves_are_spent(&self.player) => {
                if slot >= self.player.moves().len() {
                    return Err(BattleError::InvalidMoveSlot(slot));
                }
                Ok(ValidatedPlayerAction::UseMove {
                    slot: None,
                    move_id: STRUGGLE,
                })
            }
            PlayerAction::UseMove(slot) => Ok(ValidatedPlayerAction::UseMove {
                slot: Some(slot),
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

    /// Priorities of the two selections against the battlers as they stand
    /// now; a missing slot reads as no move.
    ///
    /// `end_turn` is the rebuilt end-turn comparison, which reads a forced
    /// Struggle's retained slot instead of Struggle.
    fn priorities_of(
        &self,
        chosen: ChosenMoves,
        end_turn: bool,
    ) -> Result<ResidualPriorities, BattleError> {
        let slot_priority = |battler: &BattlePokemon, slot: usize| {
            battler.moves().get(slot).map_or(Ok(NO_MOVE_PRIORITY), |m| {
                self.dex.move_data(m.move_id).map(|data| data.priority)
            })
        };
        let priority =
            |selection: Selection, battler: &BattlePokemon, retained: usize| match selection {
                Selection::NoMove => Ok(NO_MOVE_PRIORITY),
                Selection::Slot(slot) => slot_priority(battler, slot),
                Selection::Struggle if end_turn => slot_priority(battler, retained),
                Selection::Struggle => self.dex.move_data(STRUGGLE).map(|data| data.priority),
            };
        Ok(ResidualPriorities {
            player: priority(chosen.player, &self.player, self.retained_slots[0])?,
            enemy: priority(chosen.enemy, &self.enemy, self.retained_slots[1])?,
        })
    }

    fn pass_turn(
        &mut self,
        chosen: ChosenMoves,
        events: &mut Vec<BattleEvent>,
        rng: &mut impl BattleRng,
    ) -> Result<(), BattleError> {
        // Upstream reaches `HandleFaintedMonActions` twice a turn: from each
        // action's own `Cmd_end`, and again behind `BattleTurnPassed`'s
        // residual pass (`src/battle_main.c:549`, `:3960`-`:3968`).
        self.handle_fainted_mons(events)?;
        if self.player.pending_move_learn().is_some() {
            self.pending_residual_selection = Some(chosen);
            return Ok(());
        }
        // Upstream zeroes `gCurrentMove` as each action closes, before the
        // residual pass runs (`pokeemerald/src/battle_util.c:658`-`:671`).
        self.last_move_used = MOVE_NONE;
        self.residual_effects(chosen, events, rng)?;
        self.handle_fainted_mons(events)
    }

    fn finish(&mut self, events: &mut Vec<BattleEvent>, outcome: BattleOutcome) {
        self.outcome = Some(outcome);
        events.push(BattleEvent::Ended(outcome));
    }
}

#[cfg(test)]
mod tests {
    use assets::trainers::{AiFlags, TrainerId};
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
    const ABSORB: MoveId = MoveId::ABSORB;
    const TACKLE: MoveId = MoveId::TACKLE;
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
            player_reserves: Vec::new(),
            player_slot: 0,
            enemy,
            run_attempts: 0,
            random_turn_number: 0,
            outcome: None,
            kind: BattleKind::Trainer(trainer),
            turn_counter: 0,
            turn_has_started: false,
            pending_residual_selection: None,
            retained_slots: [0; 2],
            last_move_used: MOVE_NONE,
            battler_states: [BattlerState::default(); 2],
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
