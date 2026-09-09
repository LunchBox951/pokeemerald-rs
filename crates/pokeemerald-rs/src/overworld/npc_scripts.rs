//! Authored messages for the object-event scripts supported by the current
//! overworld slice.
//!
//! The subset contains Mom's fresh-save branch. Unsupported script names
//! return `None` so the interaction flow can handle them separately.

use engine::text::Token;

use crate::authored_message;
use crate::new_game::DEFAULT_PLAYER_NAME;

const MOM_FRESH_SAVE_SCRIPT: &str = "PlayersHouse_1F_EventScript_Mom";

/// Returns the authored message for a supported object-event script.
#[must_use]
pub(crate) fn script_text(script: &str) -> Option<Vec<Token>> {
    match script {
        MOM_FRESH_SAVE_SCRIPT => Some(
            authored_message::parse_message(&mom_text())
                .expect("Mom's compiled-in default message must be a valid authored message"),
        ),
        _ => None,
    }
}

fn mom_text() -> String {
    format!("MOM: See, {DEFAULT_PLAYER_NAME}?\nIsn't it nice in here, too?")
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNSCRIPTED_OBJECT_EVENT: &str = "0x0";
    const UNSUPPORTED_SCRIPT: &str = "RivalsHouse_1F_EventScript_RivalMom";

    #[test]
    fn script_text_recognizes_moms_default_message_only() {
        assert!(script_text(MOM_FRESH_SAVE_SCRIPT).is_some());
        assert!(script_text(UNSCRIPTED_OBJECT_EVENT).is_none());
        assert!(script_text(UNSUPPORTED_SCRIPT).is_none());
    }

    #[test]
    fn moms_message_uses_the_default_name_and_ends_without_a_prompt_clear() {
        let tokens = script_text(MOM_FRESH_SAVE_SCRIPT).unwrap();
        assert!(tokens.windows(DEFAULT_PLAYER_NAME.len()).any(|w| w
            .iter()
            .zip(DEFAULT_PLAYER_NAME.chars())
            .all(|(t, c)| *t == Token::Char(c))));
        assert_eq!(tokens[tokens.len() - 2], Token::Char('?'));
        assert_eq!(tokens.last(), Some(&Token::End));
        assert!(
            !tokens.contains(&Token::PromptClear),
            "Mom's message must not contain a prompt-clear token"
        );
    }

    #[test]
    fn moms_message_is_gen3_encodable() {
        let tokens = script_text(MOM_FRESH_SAVE_SCRIPT).unwrap();
        engine::text::encode(&tokens)
            .unwrap_or_else(|err| panic!("message not Gen-3 encodable: {err} in {tokens:?}"));
    }
}
