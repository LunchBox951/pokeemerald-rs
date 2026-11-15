//! The text a live battle plays: the sequencer's scene, its per-frame input,
//! and its place in the composed frame (S-6).
//!
//! A battle that plays its text keeps its [`BattleSequence`] in
//! [`OverworldPhase::battle_sequence`] beside the [`ActiveBattle`] it belongs
//! to. The sequencer runs the battle's turns, so the battle slot's frame
//! driver is replaced by [`OverworldPhase::tick_battle_text`] until the
//! sequencer reports the battle over.

use battle::Battle;
use engine::text::decode_to_string;
use platform::ButtonState;
use rendering::Framebuffer;

use crate::battle_narration::{
    AssetNames, BattleKind, Battler, NarrationError, NarrationNames, Scene,
};
use crate::flow::battle_sequence::{BattleSequence, Progress, SequenceError};
use crate::overworld::dialog::confirm_printer_input;

use super::OverworldPhase;

/// Test-only: whether a phase's battles play their text. Production phases
/// always do.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BattleText {
    /// Battles print their text and wait on it.
    Played,
    /// Battles resolve a turn per frame with no text, so the many tests of
    /// battle mechanics and flow need no asset pack.
    Skipped,
}

impl OverworldPhase {
    /// The text sequencer for `battle`, a wild battle against the lead on
    /// loan, or `None` when this test phase's battles skip their text.
    ///
    /// The pack is read once per battle from the phase's [`Self::pack_source`].
    ///
    /// # Errors
    ///
    /// Returns [`SequenceError`] when the pack cannot supply the battle text
    /// or an opening print does not expand.
    pub(super) fn open_wild_battle_text(
        &self,
        battle: &Battle,
    ) -> Result<Option<BattleSequence>, SequenceError> {
        #[cfg(test)]
        if self.battle_text == BattleText::Skipped {
            return Ok(None);
        }
        let scene = self.wild_scene(battle)?;
        let pack = self.pack_source.load()?;
        BattleSequence::begin(pack, self.save2.options_text_speed, scene).map(Some)
    }

    /// The narrator's opening scene for a wild battle: the player's name, the
    /// lead's nickname and ability as `BufferStringBattle` copies them, and
    /// the opponent named after its species (`CreateMon` gives a wild mon its
    /// species name as nickname).
    fn wild_scene(&self, battle: &Battle) -> Result<Scene, NarrationError> {
        let player = battle.player();
        let species_name = |species| {
            AssetNames
                .species_name(species)
                .ok_or(NarrationError::UnknownName("species"))
        };
        let nickname = self
            .lead
            .as_ref()
            .and_then(|lead| decode_to_string(&lead.record().box_data.nickname()).ok())
            .filter(|name| !name.is_empty());
        let player_name = match nickname {
            Some(name) => name,
            None => species_name(player.species())?,
        };
        Ok(Scene {
            kind: BattleKind::Wild,
            player_name: decode_to_string(&self.save2.player_name).unwrap_or_default(),
            player: Battler {
                name: player_name,
                ability: Some(player.ability()),
            },
            opponent: Battler {
                name: species_name(battle.enemy().species())?,
                ability: Some(battle.enemy().ability()),
            },
            trainer: None,
            hp_scale: 0,
        })
    }

    /// Plays one frame of the battle's text and, once it is all printed,
    /// reports how the battle ended: `Some(turn_failed)`, or `None` while the
    /// text or a turn is still to come. A sequencer that cannot continue ends
    /// the battle as a failed turn, the way a turn the engine refuses does.
    pub(super) fn tick_battle_text(
        &mut self,
        battle: &mut Battle,
        buttons: ButtonState,
    ) -> Option<bool> {
        let sequence = self.battle_sequence.as_mut()?;
        let progress = sequence.tick(battle, &mut self.rng, confirm_printer_input(buttons));
        let turn_failed = match progress {
            Ok(Progress::Running) => return None,
            Ok(Progress::Finished { turn_failed }) => turn_failed,
            Err(error) => {
                eprintln!("battle: its text cannot continue ({error}) -- ending the encounter");
                true
            }
        };
        self.battle_sequence = None;
        Some(turn_failed)
    }

    /// Draws the live battle's text over `framebuffer`.
    pub(super) fn draw_battle_text(&self, framebuffer: &mut Framebuffer) {
        if let Some(sequence) = &self.battle_sequence {
            sequence.draw(framebuffer);
        }
    }
}
