//! NPC script-text subset (I-3, issue #161): a direct msgbox-style stand-in
//! for the small number of upstream object-event scripts this slice
//! recognizes, keyed by [`assets::ObjectEvent::script`]'s symbolic name --
//! the full script bytecode interpreter (`engine::script`) is not modelled
//! yet (deferred, still in v1 scope) and stays out of this slice's scope
//! (the issue's own `DoD` wording: "a direct msgbox-style script subset is
//! enough").
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

use crate::new_game::DEFAULT_PLAYER_NAME;

/// The token stream to print for `script`'s recognized fresh-save default
/// message, or `None` if this slice doesn't recognize `script` (module
/// docs).
#[must_use]
pub(crate) fn script_text(script: &str) -> Option<Vec<Token>> {
    match script {
        "PlayersHouse_1F_EventScript_Mom" => Some(parse_message(&mom_text())),
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
/// scope); the trailing `{P}` is
/// this port's own addition on top of upstream's raw string (which ends
/// `"too?$"`, no embedded `\p`) -- standing in for `MSGBOX_DEFAULT`'s own
/// down-arrow wait-then-close behaviour (`crate::intro::speech::AND_YOU_ARE`'s
/// doc comment explains the same stand-in for a different upstream
/// mechanism).
fn mom_text() -> String {
    format!("MOM: See, {DEFAULT_PLAYER_NAME}?\nIsn't it nice in here, too?{{P}}")
}

/// Translate one authored message (the same `{P}`/`\n` convention
/// `crate::intro::speech::parse_page` uses, minus `{L}`/`\l`: no message in
/// this table's own scope needs a mid-message scroll) into a decoded
/// [`Token`] stream, terminated with [`Token::End`].
fn parse_message(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => tokens.push(Token::Newline),
            '{' => {
                let marker: String = chars.clone().take(2).collect();
                if marker == "P}" {
                    chars.by_ref().take(2).for_each(drop);
                    tokens.push(Token::PromptClear);
                } else {
                    // No other `{...}` marker appears in this module's
                    // authored messages.
                    tokens.push(Token::Char('{'));
                }
            }
            other => tokens.push(Token::Char(other)),
        }
    }
    tokens.push(Token::End);
    tokens
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
    fn moms_message_bakes_in_the_fixed_default_name_and_waits_before_closing() {
        let tokens = script_text("PlayersHouse_1F_EventScript_Mom").unwrap();
        assert!(tokens.windows(DEFAULT_PLAYER_NAME.len()).any(|w| w
            .iter()
            .zip(DEFAULT_PLAYER_NAME.chars())
            .all(|(t, c)| *t == Token::Char(c))));
        // Ends `?{P}` then `End` -- a button press is required to close it,
        // matching MSGBOX_DEFAULT (module docs).
        assert_eq!(tokens[tokens.len() - 3], Token::Char('?'));
        assert_eq!(tokens[tokens.len() - 2], Token::PromptClear);
        assert_eq!(tokens.last(), Some(&Token::End));
    }

    #[test]
    fn moms_message_is_gen3_encodable() {
        let tokens = script_text("PlayersHouse_1F_EventScript_Mom").unwrap();
        engine::text::encode(&tokens)
            .unwrap_or_else(|err| panic!("message not Gen-3 encodable: {err} in {tokens:?}"));
    }

    #[test]
    fn parse_message_translates_newline_and_the_page_marker() {
        let tokens = parse_message("Hi{P}there\nyou");
        assert_eq!(
            tokens,
            vec![
                Token::Char('H'),
                Token::Char('i'),
                Token::PromptClear,
                Token::Char('t'),
                Token::Char('h'),
                Token::Char('e'),
                Token::Char('r'),
                Token::Char('e'),
                Token::Newline,
                Token::Char('y'),
                Token::Char('o'),
                Token::Char('u'),
                Token::End,
            ]
        );
    }
}
