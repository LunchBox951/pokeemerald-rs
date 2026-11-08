//! The live battle sequencer: a battle's turns and results wait on the text
//! before them.
//!
//! The turn engine resolves a whole turn when asked, so this sequencer is what
//! keeps a turn from overtaking its text. Every stage that follows a print
//! (the next turn, a move-learning decision, the terminal result) runs only
//! when the print's [`NarrationGate`] reads [`GateStatus::Complete`] and the
//! script's hold after that print has elapsed. [`BattleMessageLayer`] is the
//! only queue; nothing here buffers resolved text.
//!
//! A batch is reserved before its producer runs, as the command that prints
//! stays outstanding until its printer is inactive
//! (`pokeemerald/src/battle_controller_player.c:1339-1342`). The turn's
//! requests then go to the layer one print at a time: each print's
//! `waitmessage` hold (`pokeemerald/src/battle_script_commands.c:2159-2177`)
//! starts only once that print's gate has completed, so the hold cannot be
//! applied to the whole turn at once. A [`NarrationRequest`] carries no frame
//! operand, so the hold comes from [`script_hold`].
//!
//! One [`BattleSequence::tick`] is one presentation frame: the layer ticks
//! once, and at most one stage change follows it.

use std::collections::VecDeque;

use assets::pack::AssetPack;
use battle::{Battle, BattleEvent};
use engine::rng::Rng;
use engine::text::render::PrinterInput;
use rendering::Framebuffer;

use super::first_usable_move::take_first_usable_move_turn;
use super::move_learn::decline_pending_move_learn;
use super::wild_encounter::take_run_turn;
use crate::battle_narration::{
    AssetNames, BattleMessageLayer, Battler, NarrationError, NarrationGate, NarrationRequest,
    Narrator, PresentationError, Scene, Side,
};

/// `B_WAIT_TIME_LONG` (`pokeemerald/include/constants/battle.h:324`).
const WAIT_TIME_LONG: u16 = 64;

/// Why the sequencer could not continue; the caller ends the battle.
#[derive(Debug)]
pub(super) enum SequenceError {
    /// The layer could not be built, or a print did not expand.
    Presentation(PresentationError),
    /// A battle event had no narration.
    Narration(NarrationError),
}

impl std::fmt::Display for SequenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Presentation(error) => error.fmt(f),
            Self::Narration(error) => error.fmt(f),
        }
    }
}

impl From<PresentationError> for SequenceError {
    fn from(error: PresentationError) -> Self {
        Self::Presentation(error)
    }
}

impl From<NarrationError> for SequenceError {
    fn from(error: NarrationError) -> Self {
        Self::Narration(error)
    }
}

impl From<assets::pack::PackError> for SequenceError {
    fn from(error: assets::pack::PackError) -> Self {
        Self::Presentation(error.into())
    }
}

/// What the player does on each turn the sequencer starts; the battle has no
/// action menu, so the flow that owns the battle names its one choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnPolicy {
    /// The first move slot the engine accepts: the scripted Route 101 fight.
    FirstUsableMove,
    /// An attempt to run, until the party escapes or is defeated: a wild
    /// encounter.
    Run,
}

/// What a frame of the sequencer decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Progress {
    /// Text is still printing or a hold is still running.
    Running,
    /// Every print and hold is done. `turn_failed` is set when the turn engine
    /// failed, so the battle ends without a [`battle::BattleOutcome`].
    Finished { turn_failed: bool },
}

/// One print and the `waitmessage` that follows it.
#[derive(Debug)]
struct PrintStep {
    request: NarrationRequest,
    hold: u16,
}

/// A `waitmessage` counter (`gPauseCounterBattle`), started on the frame after
/// the print completes.
#[derive(Debug)]
struct Hold {
    frames: u16,
    waited: Option<u16>,
}

impl Hold {
    const fn new(frames: u16) -> Self {
        Self {
            frames,
            waited: None,
        }
    }

    /// Called once per frame on which the print has completed; true on the
    /// frame the hold releases. A hold of none releases on the completing
    /// frame itself.
    fn released(&mut self) -> bool {
        let Some(waited) = &mut self.waited else {
            self.waited = Some(0);
            return self.frames == 0;
        };
        *waited = waited.saturating_add(1);
        *waited >= self.frames
    }
}

/// The print on screen and what follows it.
#[derive(Debug)]
struct Printing {
    gate: NarrationGate,
    hold: Hold,
    rest: VecDeque<PrintStep>,
}

#[derive(Debug)]
enum Stage {
    Printing(Printing),
    Finished,
}

/// Plays one battle's text and gates its turns on it.
#[derive(Debug)]
pub(super) struct BattleSequence {
    pack: AssetPack,
    layer: BattleMessageLayer<assets::fonts::OwnedFontGlyphSheet>,
    narrator: Narrator,
    stage: Stage,
    policy: TurnPolicy,
    /// The player's reserves in the order the engine sends them out, named as
    /// the replacement's print needs them.
    reserves: Vec<Battler>,
    /// How many replacements have been narrated.
    sent_out: usize,
    turn_failed: bool,
}

/// Resolves `gate` with `requests` against `pack`'s battle strings.
fn resolve(
    layer: &mut BattleMessageLayer<assets::fonts::OwnedFontGlyphSheet>,
    pack: &AssetPack,
    gate: &NarrationGate,
    requests: &[NarrationRequest],
) -> Result<(), SequenceError> {
    let table = pack.battle_messages()?;
    layer.resolve(gate, &table, &AssetNames, requests)?;
    Ok(())
}

/// The `waitmessage` frames after print `position` of `count` that `event`
/// produced.
///
/// The scripts end most prints with `waitmessage B_WAIT_TIME_LONG`
/// (`pokeemerald/data/battle_scripts_1.s:259-266`, `:556-560`), and that is
/// the default. The exceptions print with no hold of their own:
///
/// - the move announcement, whose `attackstring` is followed by `ppreduce`
///   (`:245-247`);
/// - the faint text, returned from `BattleScript_FaintTarget` (`:2815-2823`);
/// - the experience award, printed by `getexp` (`:2825-2828`);
/// - the move-learning questions and answers, which wait on the yes/no box
///   (`:3164-3177`), and the forgetting lines ahead of the learned line
///   (`:3178-3187`);
/// - the defeated-trainer and defeat-speech lines ahead of the prize
///   (`:2920-2943`).
fn script_hold(event: &BattleEvent, position: usize, count: usize) -> u16 {
    let last = position + 1 == count;
    match event {
        BattleEvent::UsedMove { .. }
        | BattleEvent::Fainted { .. }
        | BattleEvent::ExpGained { .. }
        | BattleEvent::MoveLearnPrompt { .. }
        | BattleEvent::MoveLearnDeclined { .. } => 0,
        BattleEvent::MoveReplaced { .. } | BattleEvent::MoneyGained(_) if !last => 0,
        _ => WAIT_TIME_LONG,
    }
}

impl BattleSequence {
    /// Opens the battle's text: the introduction, then each side's first
    /// send-out (`STRINGID_INTROMSG`, `STRINGID_INTROSENDOUT`,
    /// `pokeemerald/src/battle_main.c:3564-3607`), printed before any turn.
    ///
    /// `raw_text_speed` is the saved text-speed option, and `policy` the action
    /// each turn takes. `reserves` are the player's other members in the order
    /// the engine sends them out, so a replacement is named by its nickname and
    /// ability.
    ///
    /// # Errors
    ///
    /// Returns [`SequenceError`] when the pack lacks the battle text or an
    /// opening print does not expand.
    pub(super) fn begin(
        pack: AssetPack,
        raw_text_speed: u8,
        scene: Scene,
        policy: TurnPolicy,
        reserves: Vec<Battler>,
    ) -> Result<Self, SequenceError> {
        let mut layer = BattleMessageLayer::from_pack(&pack, raw_text_speed)?;
        let narrator = Narrator::new(scene);
        let opening: VecDeque<PrintStep> = std::iter::once(narrator.introduction())
            .chain(narrator.initial_send_out(Side::Opponent))
            .chain(narrator.initial_send_out(Side::Player))
            .map(|request| PrintStep { request, hold: 0 })
            .collect();
        let gate = layer.reserve();
        let stage = Self::start_printing(&mut layer, &pack, gate, opening)?;
        Ok(Self {
            pack,
            layer,
            narrator,
            stage,
            policy,
            reserves,
            sent_out: 0,
            turn_failed: false,
        })
    }

    /// The prints for `events` in order, each with its hold.
    ///
    /// A replacement is staged before its send-out is narrated, with the
    /// opponent's health that picks the wording: upstream names the incoming
    /// mon by its nickname (`Cmd_switchindataupdate`,
    /// `pokeemerald/src/battle_script_commands.c:4669`) and runs
    /// `hpthresholds BS_FAINTED` ahead of `STRINGID_SWITCHINMON`
    /// (`pokeemerald/data/battle_scripts_1.s:2887-2888`).
    fn print_steps(
        &mut self,
        battle: &Battle,
        events: &[BattleEvent],
    ) -> Result<VecDeque<PrintStep>, NarrationError> {
        let mut steps = VecDeque::new();
        for event in events {
            if matches!(event, BattleEvent::PlayerSentOut { .. }) {
                let enemy = battle.enemy();
                self.narrator
                    .observe_opponent_health(enemy.current_hp(), enemy.stats().max_hp);
                if let Some(replacement) = self.reserves.get(self.sent_out) {
                    self.narrator
                        .stage_replacement(Side::Player, replacement.clone());
                }
                self.sent_out += 1;
            }
            let requests = self.narrator.requests(&AssetNames, event)?;
            let count = requests.len();
            steps.extend(
                requests
                    .into_iter()
                    .enumerate()
                    .map(|(position, request)| PrintStep {
                        request,
                        hold: script_hold(event, position, count),
                    }),
            );
        }
        Ok(steps)
    }

    /// The scene as the narrator last staged it.
    #[cfg(test)]
    pub(super) const fn scene(&self) -> &Scene {
        self.narrator.scene()
    }

    /// Resolves the first of `steps` into `gate`, or nothing when there is
    /// none, and returns the stage that waits on it.
    fn start_printing(
        layer: &mut BattleMessageLayer<assets::fonts::OwnedFontGlyphSheet>,
        pack: &AssetPack,
        gate: NarrationGate,
        mut steps: VecDeque<PrintStep>,
    ) -> Result<Stage, SequenceError> {
        let first = steps.pop_front();
        let requests: Vec<NarrationRequest> =
            first.iter().map(|step| step.request.clone()).collect();
        resolve(layer, pack, &gate, &requests)?;
        Ok(Stage::Printing(Printing {
            gate,
            hold: Hold::new(first.map_or(0, |step| step.hold)),
            rest: steps,
        }))
    }

    /// Plays one frame with `input`, the frame's confirm-button state.
    ///
    /// # Errors
    ///
    /// Returns [`SequenceError`] when a print does not expand or an event has
    /// no narration. The sequence is then spent and the caller ends the
    /// battle.
    pub(super) fn tick(
        &mut self,
        battle: &mut Battle,
        rng: &mut Rng,
        input: PrinterInput,
    ) -> Result<Progress, SequenceError> {
        self.layer.tick(input);
        let Stage::Printing(printing) = &mut self.stage else {
            return Ok(Progress::Finished {
                turn_failed: self.turn_failed,
            });
        };
        if !self.layer.gate_status(&printing.gate).is_complete() || !printing.hold.released() {
            return Ok(Progress::Running);
        }
        if let Some(step) = printing.rest.pop_front() {
            let gate = self.layer.reserve();
            resolve(
                &mut self.layer,
                &self.pack,
                &gate,
                std::slice::from_ref(&step.request),
            )?;
            printing.gate = gate;
            printing.hold = Hold::new(step.hold);
            return Ok(Progress::Running);
        }
        self.advance_battle(battle, rng)
    }

    /// The stage after the last print and hold: a pending move-learning
    /// decision, the next turn, or the end.
    ///
    /// The batch for the stage's text is reserved before the producer runs. A
    /// pending decision takes precedence over a reported outcome, as the turn
    /// engine defers the outcome until the decision is made
    /// (`Battle::pending_move_learn`).
    fn advance_battle(
        &mut self,
        battle: &mut Battle,
        rng: &mut Rng,
    ) -> Result<Progress, SequenceError> {
        let deciding = battle.pending_move_learn().is_some();
        if self.turn_failed || (!deciding && battle.outcome().is_some()) {
            self.stage = Stage::Finished;
            return Ok(Progress::Finished {
                turn_failed: self.turn_failed,
            });
        }
        let gate = self.layer.reserve();
        let events = if deciding {
            self.decline_prompt(battle, rng)
        } else {
            self.take_turn(battle, rng)
        };
        let steps = self.print_steps(battle, &events)?;
        self.stage = Self::start_printing(&mut self.layer, &self.pack, gate, steps)?;
        Ok(Progress::Running)
    }

    /// The turn the sequence's [`TurnPolicy`] takes. A failed turn keeps the
    /// events it committed.
    fn take_turn(&mut self, battle: &mut Battle, rng: &mut Rng) -> Vec<BattleEvent> {
        let turn = match self.policy {
            TurnPolicy::FirstUsableMove => take_first_usable_move_turn(battle, rng),
            TurnPolicy::Run => take_run_turn(battle, rng),
        };
        match turn {
            Ok(events) => events,
            Err(error) => {
                eprintln!(
                    "battle: turn failed ({:?}) -- ending the encounter",
                    error.error()
                );
                self.turn_failed = true;
                error.into_events()
            }
        }
    }

    /// Declines the one pending prompt, the only choice that preserves the
    /// player's moveset.
    fn decline_prompt(&mut self, battle: &mut Battle, rng: &mut Rng) -> Vec<BattleEvent> {
        decline_pending_move_learn(battle, rng).unwrap_or_else(|error| {
            eprintln!("move learn: declining failed ({error:?}) -- ending the encounter");
            self.turn_failed = true;
            Vec::new()
        })
    }

    /// Draws the battle textbox and the current message over `framebuffer`.
    pub(super) fn draw(&self, framebuffer: &mut Framebuffer) {
        self.layer.draw(framebuffer);
    }
}

#[cfg(test)]
mod tests;
