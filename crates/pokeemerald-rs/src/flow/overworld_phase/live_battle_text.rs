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
    ///
    /// Upstream prints the saved name bytes as they are
    /// (`pokeemerald/src/battle_message.c:2299-2306, 2594-2598, 2735-2740`).
    /// This port holds names as text, so a name it cannot decode is reported as
    /// [`NarrationError::Text`] rather than swapped for the species name or
    /// printed empty.
    ///
    /// # Errors
    ///
    /// Returns [`NarrationError::Text`] when the lead's nickname or the
    /// player's name does not decode, and [`NarrationError::UnknownName`] when
    /// a species has no name.
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
            .map(|lead| decode_to_string(&lead.record().box_data.nickname()))
            .transpose()
            .map_err(NarrationError::Text)?
            .filter(|name| !name.is_empty());
        let player_name = match nickname {
            Some(name) => name,
            None => species_name(player.species())?,
        };
        let trainer_name =
            decode_to_string(&self.save2.player_name).map_err(NarrationError::Text)?;
        Ok(Scene {
            kind: BattleKind::Wild,
            player_name: trainer_name,
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

#[cfg(test)]
mod tests {
    use assets::MapId;
    use battle::Battle;
    use engine::overworld::{Direction, PlayerState};
    use engine::rng::Rng;
    use engine::save::BOX_NICKNAME_LEN;
    use engine::text::{encode_str, TextError, PLACEHOLDER_BEGIN};

    use super::{BattleText, NarrationError, OverworldPhase};
    use crate::new_game;

    /// A phase whose lead is the starter, filed with `nickname` as its saved
    /// nickname, and the battle it leads into.
    fn phase_and_battle(nickname: [u8; BOX_NICKNAME_LEN]) -> (OverworldPhase, Battle) {
        let mut phase = OverworldPhase::for_test(
            crate::overworld::test_support::synthetic_scene(25, 25),
            MapId("MAP_ROUTE101"),
            PlayerState::new((5, 5), 3, Direction::East),
            None,
        );
        phase.battle_text = BattleText::Played;
        let lead = new_game::provisional_starter();
        let mut record = crate::party::to_save_pokemon(&battle::Dex::new(), &lead);
        record.box_data.set_nickname(nickname);
        phase.set_backed_lead_for_test(0, record, lead.clone());
        let battle = crate::flow::first_battle::start_first_battle(lead, 0, &mut Rng::new(4242))
            .expect("the starter can start the first battle");
        (phase, battle)
    }

    fn nickname_field(text: &str) -> [u8; BOX_NICKNAME_LEN] {
        let encoded = encode_str(text).expect("the test nickname encodes");
        let mut field = [engine::text::EOS; BOX_NICKNAME_LEN];
        field[..encoded.len() - 1].copy_from_slice(&encoded[..encoded.len() - 1]);
        field
    }

    /// Ten glyphs end in a lone `PLACEHOLDER_BEGIN`, a control code that
    /// takes an argument byte the field has no room for.
    fn undecodable_nickname() -> [u8; BOX_NICKNAME_LEN] {
        let mut field = nickname_field("MUDKIPABC");
        field[BOX_NICKNAME_LEN - 1] = PLACEHOLDER_BEGIN;
        field
    }

    /// `name` with a lone `PLACEHOLDER_BEGIN` in its last byte and no
    /// terminator before it: a control code with no room for its argument.
    fn undecodable_player_name<const N: usize>(mut name: [u8; N]) -> [u8; N] {
        name.fill(0);
        name[N - 1] = PLACEHOLDER_BEGIN;
        name
    }

    #[test]
    fn the_scene_names_the_player_and_the_lead_from_the_save() {
        let (mut phase, battle) = phase_and_battle(nickname_field("MUDKIP"));
        let encoded = encode_str("RUSTY").expect("the test name encodes");
        phase.save2.player_name = [engine::text::EOS; 8];
        phase.save2.player_name[..encoded.len()].copy_from_slice(&encoded);
        let scene = phase.wild_scene(&battle).expect("both names decode");
        assert_eq!(scene.player_name, "RUSTY");
        assert_eq!(scene.player.name, "MUDKIP");
    }

    #[test]
    fn a_nickname_that_will_not_decode_is_reported_not_swapped_for_the_species_name() {
        let (phase, battle) = phase_and_battle(undecodable_nickname());
        assert_eq!(
            phase.wild_scene(&battle),
            Err(NarrationError::Text(TextError::Truncated(
                PLACEHOLDER_BEGIN
            )))
        );
    }

    #[test]
    fn a_player_name_that_will_not_decode_is_reported_not_printed_empty() {
        let (mut phase, battle) = phase_and_battle(nickname_field("MUDKIP"));
        phase.save2.player_name = undecodable_player_name(phase.save2.player_name);
        assert_eq!(
            phase.wild_scene(&battle),
            Err(NarrationError::Text(TextError::Truncated(
                PLACEHOLDER_BEGIN
            )))
        );
    }
}
