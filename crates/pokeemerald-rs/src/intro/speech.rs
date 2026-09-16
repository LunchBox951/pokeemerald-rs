//! Birch's eight-page introduction dialogue.
//!
//! Pages use the shared authored-message markers for page breaks, scrolling,
//! and pauses. The fixed player name is inserted while the pages are built
//! because the naming screen is not yet part of the introduction scene.
use engine::text::Token;

use crate::authored_message;
use crate::new_game::DEFAULT_PLAYER_NAME;

/// Number of dialogue pages in Birch's introduction.
pub const NUM_PAGES: usize = 8;

/// Returns every dialogue page in display order.
///
/// # Panics
///
/// Panics if an authored marker in this module is malformed.
#[must_use]
pub fn pages() -> [Vec<Token>; NUM_PAGES] {
    [
        parse_page(WELCOME),
        parse_page(THIS_IS_A_POKEMON),
        parse_page(MAIN_SPEECH),
        parse_page(AND_YOU_ARE),
        parse_page(WHATS_YOUR_NAME),
        parse_page(&so_its_player()),
        parse_page(&youre_player()),
        parse_page(ARE_YOU_READY),
    ]
}

const WELCOME: &str = "Hi! Sorry to keep you waiting!{P}Welcome to the world of POKéMON!{P}My name is BIRCH.{P}But everyone calls me the POKéMON\nPROFESSOR.{P}";

// `src/strings.c:gText_ThisIsAPokemon` pauses for 96 frames before its page break.
const THIS_IS_A_POKEMON: &str = "This is what we call a “POKéMON.”{PAUSE 96}{P}";

const MAIN_SPEECH: &str = "This world is widely inhabited by\ncreatures known as POKéMON.{P}We humans live alongside POKéMON,\nat times as friendly playmates, and{L}at times as cooperative workmates.{P}And sometimes, we band together\nand battle others like us.{P}But despite our closeness, we don't\nknow everything about POKéMON.{P}In fact, there are many, many\nsecrets surrounding POKéMON.{P}To unravel POKéMON mysteries,\nI've been undertaking research.{L}That's what I do.{P}";

// `{P}` preserves the hold from `Task_NewGameBirchSpeech_AndYouAre` while its
// Birch and Lotad transition is omitted.
const AND_YOU_ARE: &str = "And you are?{P}";

// `{P}` preserves `Task_NewGameBirchSpeech_WaitPressBeforeNameChoice`'s A/B wait.
const WHATS_YOUR_NAME: &str = "All right.\nWhat's your name?{P}";

// `{P}` stands in for `Task_NewGameBirchSpeech_ProcessNameYesNoMenu`.
fn so_its_player() -> String {
    format!("So it's {DEFAULT_PLAYER_NAME}?{{P}}")
}

fn youre_player() -> String {
    format!(
        "Ah, okay!{{P}}You're {DEFAULT_PLAYER_NAME} who's moving to my\nhometown of LITTLEROOT.{{L}}I get it now!{{P}}"
    )
}

const ARE_YOU_READY: &str = "All right, are you ready?{P}Your very own adventure is about\nto unfold.{P}Take courage, and leap into the\nworld of POKéMON where dreams,{L}adventure, and friendships await!{P}Well, I'll be expecting you later.\nCome see me in my POKéMON LAB.{P}";

fn parse_page(text: &str) -> Vec<Token> {
    authored_message::parse_message(text)
        .unwrap_or_else(|err| panic!("malformed marker in an authored speech page: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::text;

    #[test]
    fn parse_page_translates_newline_and_page_markers() {
        let tokens = parse_page("Hi{P}there\nyou{L}all");
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
                Token::PromptScroll,
                Token::Char('a'),
                Token::Char('l'),
                Token::Char('l'),
                Token::End,
            ]
        );
    }

    #[test]
    #[should_panic(expected = "malformed marker in an authored speech page")]
    fn a_malformed_marker_panics_through_parse_page() {
        parse_page("So it's {PLAYER}?");
    }

    #[test]
    fn pages_returns_eight_terminated_pages() {
        let pages = pages();
        assert_eq!(pages.len(), NUM_PAGES);
        for page in &pages {
            assert_eq!(page.last(), Some(&Token::End));
        }
    }

    #[test]
    fn every_page_is_gen3_encodable() {
        for page in pages() {
            text::encode(&page)
                .unwrap_or_else(|err| panic!("page not Gen-3 encodable: {err} in {page:?}"));
        }
    }

    #[test]
    fn name_pages_bake_in_the_fixed_default_name() {
        let pages = pages();
        let so_its_player = &pages[5];
        let youre_player = &pages[6];
        assert!(so_its_player.windows(DEFAULT_PLAYER_NAME.len()).any(|w| w
            .iter()
            .zip(DEFAULT_PLAYER_NAME.chars())
            .all(|(t, c)| *t == Token::Char(c))));
        assert!(youre_player.windows(DEFAULT_PLAYER_NAME.len()).any(|w| w
            .iter()
            .zip(DEFAULT_PLAYER_NAME.chars())
            .all(|(t, c)| *t == Token::Char(c))));
    }

    #[test]
    fn every_question_page_waits_for_a_press_before_advancing() {
        let pages = pages();
        for page_index in [3, 4, 5] {
            let page = &pages[page_index];
            assert_eq!(
                page[page.len() - 3],
                Token::Char('?'),
                "page {page_index} must end on its question mark"
            );
            assert_eq!(
                page[page.len() - 2],
                Token::PromptClear,
                "page {page_index} must hold for a button press, not auto-advance"
            );
            assert_eq!(page.last(), Some(&Token::End));
        }
    }
}
