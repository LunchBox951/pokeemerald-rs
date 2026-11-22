//! The Repel step counter and its expiry message (`UpdateRepelCounter`,
//! `pokeemerald/src/wild_encounter.c:850-872`), one of the step-count consumers
//! [`super::step`] runs for a completed step, beside [`super::field_poison`] and
//! [`super::field_friendship`].
//!
//! Nothing in this port starts a Repel yet, so the counter only ever holds a value a loaded save
//! brought in. The gate that reads it lives in [`crate::flow::wild_encounter`].

use engine::event_data::RepelStep;
use engine::overworld::{TilePos, WarpTrigger};
use engine::text::Token;

use super::OverworldPhase;

/// `Text_RepelWoreOff` (`pokeemerald/data/scripts/repel.inc:5-6`), shown by `EventScript_RepelWoreOff`
/// as a sign-style box (`MSGBOX_SIGN`, `data/scripts/std_msgbox.inc:10-16`): locked until it is
/// dismissed.
///
/// The apostrophe is the `’` (`B4`) the charmap decodes both `'` and `’` to
/// (`pokeemerald/charmap.txt:87-88`), the token the text codec itself yields.
fn repel_wore_off_message() -> Vec<Token> {
    "REPEL’s effect wore off…"
        .chars()
        .map(Token::Char)
        .chain(std::iter::once(Token::End))
        .collect()
}

impl OverworldPhase {
    /// `UpdateRepelCounter` for a completed step: the last of `TryStartStepBasedScript`'s consumers
    /// (`field_control_avatar.c:483-496`), after the coord event, the door-shaped warp and
    /// `TryStartStepCountScript`, so a step any of those claimed never reaches it, and ahead of
    /// the wild-encounter roll. Counts one step off [`engine::event_data::VAR_REPEL_STEP_COUNT`]
    /// and, when that reaches zero, opens the expiry message. Returns whether the step is claimed,
    /// which only that last step is (`:866-869`). Draws nothing from the RNG.
    ///
    /// Unlike poison it ignores forced-movement tiles and a Secret Base; the caller's `landed`
    /// already withholds the forced landing that sets no `tookStep`. The counter is the saved
    /// event var, so Save and Continue keep it mid-countdown.
    ///
    /// Not modelled: the Battle Pike and Battle Pyramid exemptions (`:854-855`), whose maps this
    /// port has no way to reach; [`Self::repel_counting_suspended`] is where they would go.
    pub(super) fn update_repel_counter(
        &mut self,
        landed: Option<TilePos>,
        door_warp: Option<WarpTrigger>,
    ) -> bool {
        if landed.is_none() || door_warp.is_some() || self.repel_counting_suspended() {
            return false;
        }
        if self.save1.event_data.count_down_repel() != RepelStep::WoreOff {
            return false;
        }
        // `ScriptContext_SetupScript` locks the player's field controls like any script.
        self.take_field_lock();
        // A box that cannot be built is logged and the field carries on; the counter is spent
        // and the step claimed either way.
        let _ = self.show_field_message(repel_wore_off_message());
        true
    }

    /// The maps on which `UpdateRepelCounter` returns before touching the counter: the Battle
    /// Pike, the Battle Pyramid (`wild_encounter.c:854-855`) and the Union Room (`:856-857`,
    /// `union_room.c:4307-4311`). Only the Union Room is a map this port can be standing in, and
    /// it is already how [`Self::update_friendship_step_counter`] reads it; the other two are the
    /// seam the Pike and Pyramid work fills in.
    fn repel_counting_suspended(&self) -> bool {
        self.map_id == assets::MapId("MAP_UNION_ROOM")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The message is `Text_RepelWoreOff` byte for byte: the codec accepts every character, and
    /// the apostrophe and the ellipsis are the single glyphs, not three periods.
    #[test]
    fn the_message_is_the_upstream_text_and_encodes() {
        let tokens = repel_wore_off_message();
        assert_eq!(tokens.last(), Some(&Token::End));
        let text: String = tokens
            .iter()
            .filter_map(|token| match token {
                Token::Char(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(text, "REPEL’s effect wore off…");
        let encoded = engine::text::encode_str(&text).expect("every character has a glyph");
        assert_eq!(
            engine::text::decode(&encoded).unwrap()[..tokens.len() - 1],
            tokens[..tokens.len() - 1]
        );
    }
}
