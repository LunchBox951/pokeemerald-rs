//! NPC script-text subset (I-3, issue #161): a direct msgbox-style stand-in
//! for the small number of upstream object-event scripts this slice
//! recognizes, keyed by [`assets::ObjectEvent::script`]'s symbolic name --
//! the full script bytecode interpreter (`engine::script`) exists but is not
//! wired into this slice yet -- deferred, still in v1 scope (the issue's own
//! `DoD` wording: "a direct msgbox-style script subset is enough").
//!
//! [`script_text`] returns `None` for every script this table doesn't name
//! (including the literal `"0x0"` no-script sentinel): pressing A while
//! facing that object event still selects it
//! ([`engine::overworld::facing_object_event`] found a real, visible
//! object), but no dialog opens -- the same observable outcome upstream's
//! own `TryStartInteractionScript` produces for a `NULL` script
//! (`GetInteractedObjectEventScript` returns `NULL`, so nothing happens),
//! and, for a script this table simply hasn't been taught yet, an honest
//! "not modelled" rather than a fabricated line.

use engine::text::Token;

use crate::authored_message;
use crate::new_game::DEFAULT_PLAYER_NAME;

/// The token stream to print for `script`'s recognized fresh-save default
/// message, or `None` if this slice doesn't recognize `script` (module
/// docs).
#[must_use]
pub(crate) fn script_text(script: &str) -> Option<Vec<Token>> {
    match script {
        "PlayersHouse_1F_EventScript_Mom" => Some(
            authored_message::parse_message(&mom_text())
                .expect("Mom's compiled-in default message must be a valid authored message"),
        ),
        _ => None,
    }
}

/// `PlayersHouse_1F_Text_IsntItNiceInHere`
/// (`pokeemerald/data/maps/LittlerootTown_BrendansHouse_1F/scripts.inc`),
/// reached by `PlayersHouse_1F_EventScript_Mom`'s
/// (`pokeemerald/data/scripts/players_house.inc`) fresh-save default path:
/// every earlier `goto_if_set`/`goto_if_eq` guard in that script
/// (`FLAG_HAS_MATCH_CALL`, `FLAG_RESCUED_BIRCH`, `VAR_TEMP_1`,
/// `VAR_LITTLEROOT_INTRO_STATE`, `VAR_LITTLEROOT_HOUSES_STATE_MAY`/`_BRENDAN`)
/// reads false/`0` on a brand-new save (see `crate::new_game`'s own
/// fresh-save state), so this is the one line an unmodified new game's Mom
/// actually shows. `{PLAYER}` is substituted for the fixed pre-1.0 default
/// name, mirroring `crate::intro::speech`'s identical convention for the same
/// reason (the naming screen is not modelled yet -- deferred, still in v1
/// scope).
///
/// Byte-identical to upstream's own raw string now (`"...too?$"`, no
/// embedded `\p`): before issue #410, this function appended a synthetic
/// trailing `{P}` here to stand in for `MSGBOX_DEFAULT`'s down-arrow
/// wait-then-close behaviour, which clears the box and pays a post-clear
/// reveal delay upstream's `waitbuttonpress` never does. That wait is now
/// [`crate::overworld::dialog::NpcDialog::with_waitbuttonpress`] (applied by
/// every real dialog this table opens through, `NpcDialog::from_pack`'s own
/// doc comment), so the text itself carries none of it.
fn mom_text() -> String {
    format!("MOM: See, {DEFAULT_PLAYER_NAME}?\nIsn't it nice in here, too?")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_text_recognizes_moms_default_message_only() {
        assert!(script_text("PlayersHouse_1F_EventScript_Mom").is_some());
        assert!(script_text("0x0").is_none());
        assert!(script_text("RivalsHouse_1F_EventScript_RivalMom").is_none());
    }

    #[test]
    fn moms_message_bakes_in_the_fixed_default_name_and_ends_on_the_upstream_raw_string() {
        let tokens = script_text("PlayersHouse_1F_EventScript_Mom").unwrap();
        assert!(tokens.windows(DEFAULT_PLAYER_NAME.len()).any(|w| w
            .iter()
            .zip(DEFAULT_PLAYER_NAME.chars())
            .all(|(t, c)| *t == Token::Char(c))));
        // Ends `?` then `End`, with no `PromptClear` anywhere -- issue #410:
        // the button-press-required-to-close wait is now
        // `NpcDialog::with_waitbuttonpress` (module docs), not a synthetic
        // `{P}` baked into the text, matching upstream's own raw string
        // (`"...too?$"`, no embedded `\p`).
        assert_eq!(tokens[tokens.len() - 2], Token::Char('?'));
        assert_eq!(tokens.last(), Some(&Token::End));
        assert!(
            !tokens.contains(&Token::PromptClear),
            "Mom's message must not carry a synthetic trailing prompt-clear anymore"
        );
    }

    #[test]
    fn moms_message_is_gen3_encodable() {
        let tokens = script_text("PlayersHouse_1F_EventScript_Mom").unwrap();
        engine::text::encode(&tokens)
            .unwrap_or_else(|err| panic!("message not Gen-3 encodable: {err} in {tokens:?}"));
    }
}
