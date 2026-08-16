//! Birch's intro speech: the actual dialogue text (issue #149), transcribed
//! from `pokeemerald/data/text/birch_speech.inc` (the `\p`/`\n`/`\l`-laden
//! strings) and the two speech lines `pokeemerald/src/strings.c` defines
//! outside that file (`gText_ThisIsAPokemon`, line 100).
//!
//! # Authoring convention
//!
//! [`parse_page`] re-expresses upstream's own escape convention as data,
//! not code `(no-verbatim)`: a literal `\n` in a page string is a real Rust
//! newline (-> [`Token::Newline`], upstream `CHAR_NEWLINE`), `{P}` marks a
//! upstream `\p` (-> [`Token::PromptClear`] -- wait for a button press, then
//! clear and start a fresh page) and `{L}` marks a `\l` (->
//! [`Token::PromptScroll`] -- wait for a button press, then scroll up one
//! line). Every other character maps through
//! [`Token::Char`]/[`engine::text::char_to_byte`] unchanged --
//! this module's `every_page_is_gen3_encodable` test pins that every page's characters
//! (including "é" and the curly “ ” quotes `gText_ThisIsAPokemon` uses) have
//! a real Gen-3 encoding.
//!
//! # Player-name substitution
//!
//! Two upstream lines interpolate the player's name and the (English-empty)
//! `{KUN}` honorific: `gText_Birch_SoItsPlayer` ("So it's {PLAYER}{KUN}?")
//! and `gText_Birch_YourePlayer` ("You're {PLAYER}{KUN} who's moving..."),
//! via a runtime `StringExpandPlaceholders` call
//! (`main_menu.c:1612`/`1678`). Since this slice's player name is a fixed
//! pre-1.0 default rather than the naming screen (not modelled yet --
//! deferred, still in v1 scope; see `crate::new_game`'s module docs),
//! [`crate::new_game::DEFAULT_PLAYER_NAME`] is baked into these two pages'
//! text directly at authoring time -- the
//! same observable result `StringExpandPlaceholders` would produce for that
//! fixed name, without needing this module to depend on
//! [`engine::text::format`]'s placeholder-resolver machinery for a
//! single, statically-known substitution.
//!
//! # What's omitted
//!
//! `gText_ThisIsAPokemon`'s `{PAUSE 96}` control code (a 96-frame pause
//! before its trailing `\p`) is not transcribed:
//! [`engine::text::render::Printer`] already treats `PAUSE` as a no-op
//! (see its module docs' "Deliberately out of scope" section), so encoding
//! it would add a token with no observable effect in this engine slice --
//! the trailing `{P}` still requires a button press exactly as upstream's
//! `\p` does either way.

use engine::text::Token;

use crate::new_game::DEFAULT_PLAYER_NAME;

/// How many speech pages [`pages`] returns -- one per upstream
/// `AddTextPrinterForMessage`/`AddTextPrinterWithCallbackForMessage` call in
/// `Task_NewGameBirchSpeech_*` (`main_menu.c:1338-1727`), skipping only the
/// gender-select and naming-screen UI steps between them (see
/// `crate::new_game`'s "No truck sequence" / "Fixed player name/gender"
/// deviations).
pub const NUM_PAGES: usize = 8;

/// Every Birch-speech page, in upstream's display order, ready for
/// [`engine::text::render::Printer`].
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

/// `gText_Birch_Welcome` (`birch_speech.inc:1-7`).
const WELCOME: &str = "Hi! Sorry to keep you waiting!{P}Welcome to the world of POKéMON!{P}My name is BIRCH.{P}But everyone calls me the POKéMON\nPROFESSOR.{P}";

/// `gText_ThisIsAPokemon` (`strings.c:100`) -- module docs on the omitted
/// `{PAUSE 96}`.
const THIS_IS_A_POKEMON: &str = "This is what we call a “POKéMON.”{P}";

/// `gText_Birch_MainSpeech` (`birch_speech.inc:14-29`).
const MAIN_SPEECH: &str = "This world is widely inhabited by\ncreatures known as POKéMON.{P}We humans live alongside POKéMON,\nat times as friendly playmates, and{L}at times as cooperative workmates.{P}And sometimes, we band together\nand battle others like us.{P}But despite our closeness, we don't\nknow everything about POKéMON.{P}In fact, there are many, many\nsecrets surrounding POKéMON.{P}To unravel POKéMON mysteries,\nI've been undertaking research.{L}That's what I do.{P}";

/// `gText_Birch_AndYouAre` (`birch_speech.inc:31-32`) plus a trailing
/// `{P}` this port adds on top of upstream's own raw string (which ends
/// `"?$"` with no embedded `\p`).
///
/// Upstream doesn't need one: `Task_NewGameBirchSpeech_AndYouAre` prints
/// this line, then *auto*-advances (no button wait) the instant printing
/// finishes, straight into the Birch/Lotad platform-fade-and-slide
/// sequence (`Task_NewGameBirchSpeech_StartBirchLotadPlatformFade` through
/// `Task_NewGameBirchSpeech_WaitForPlayerFadeIn`, `main_menu.c:1410-1477`)
/// that keeps this same text on screen for several real seconds before the
/// gender-select page replaces it. This port renders none of that
/// animation (module docs' "No Birch/Lotad/player sprites..." bullet), so
/// without a `{P}` here the page would instead vanish the instant its own
/// short reveal-delay after the last glyph expires -- legible for only a
/// few frames, nothing like upstream's actual pacing. A button-wait is the
/// closest available stand-in for "stays up until something else advances
/// it" `(behavioral-fidelity)`.
const AND_YOU_ARE: &str = "And you are?{P}";

/// `gText_Birch_WhatsYourName` (`birch_speech.inc:38-40`) plus a trailing
/// `{P}`: unlike [`AND_YOU_ARE`]'s animation-gated pause, this one is a
/// literal upstream button-wait --
/// `Task_NewGameBirchSpeech_WaitPressBeforeNameChoice`
/// (`main_menu.c:1590`), reached once
/// `Task_NewGameBirchSpeech_WaitForWhatsYourNameToPrint` sees printing
/// finish -- which does not advance to the (here, elided) naming screen
/// until `JOY_NEW(A_BUTTON) || JOY_NEW(B_BUTTON)`.
const WHATS_YOUR_NAME: &str = "All right.\nWhat's your name?{P}";

/// `gText_Birch_SoItsPlayer` (`birch_speech.inc:42-43`) with `{PLAYER}{KUN}`
/// substituted for the fixed pre-1.0 default name (module docs), plus a trailing
/// `{P}` this port adds on top of upstream's own raw string (which ends
/// `"?$"` with no embedded `\p`) -- the same stand-in [`AND_YOU_ARE`] uses,
/// for a different upstream mechanism.
///
/// Upstream holds this page on screen for the name confirmation:
/// `Task_NewGameBirchSpeech_SoItsPlayerName` prints it and hands off to
/// `Task_NewGameBirchSpeech_CreateNameYesNo` (`main_menu.c:1609-1623`),
/// which waits for printing to finish and then opens a Yes/No menu;
/// `Task_NewGameBirchSpeech_ProcessNameYesNoMenu` (`main_menu.c:1626`) sits
/// there until the player actually answers -- Yes continues to
/// "Ah, okay!...", No loops back to the naming step. This port renders no
/// Yes/No menu (module docs' elided naming/gender UI), so without a `{P}`
/// the page would auto-advance into the next one a few frames after its
/// last glyph revealed, instead of waiting for the player the way upstream
/// does. A button-wait is the closest available stand-in for that
/// confirmation `(behavioral-fidelity)`.
fn so_its_player() -> String {
    format!("So it's {DEFAULT_PLAYER_NAME}?{{P}}")
}

/// `gText_Birch_YourePlayer` (`birch_speech.inc:45-50`) with
/// `{PLAYER}{KUN}` substituted (module docs).
fn youre_player() -> String {
    format!(
        "Ah, okay!{{P}}You're {DEFAULT_PLAYER_NAME} who's moving to my\nhometown of LITTLEROOT.{{L}}I get it now!{{P}}"
    )
}

/// `gText_Birch_AreYouReady` (`birch_speech.inc:52-61`).
const ARE_YOU_READY: &str = "All right, are you ready?{P}Your very own adventure is about\nto unfold.{P}Take courage, and leap into the\nworld of POKéMON where dreams,{L}adventure, and friendships await!{P}Well, I'll be expecting you later.\nCome see me in my POKéMON LAB.{P}";

/// Translate one authored page (module docs' `{P}`/`{L}`/`\n` convention)
/// into a decoded [`Token`] stream, terminated with [`Token::End`].
fn parse_page(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => tokens.push(Token::Newline),
            '{' => {
                let marker: String = chars.clone().take(2).collect();
                match marker.as_str() {
                    "P}" => {
                        chars.by_ref().take(2).for_each(drop);
                        tokens.push(Token::PromptClear);
                    }
                    "L}" => {
                        chars.by_ref().take(2).for_each(drop);
                        tokens.push(Token::PromptScroll);
                    }
                    // No other `{...}` marker appears in this module's
                    // authored pages; a literal `{` (never used above)
                    // would otherwise fall through here unchanged.
                    _ => tokens.push(Token::Char('{')),
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
    fn pages_returns_eight_terminated_pages() {
        let pages = pages();
        assert_eq!(pages.len(), NUM_PAGES);
        for page in &pages {
            assert_eq!(page.last(), Some(&Token::End));
        }
    }

    #[test]
    fn every_page_is_gen3_encodable() {
        // Every Token::Char must round-trip through the real Gen-3 glyph
        // table (engine::text::encode) -- a stray un-encodable character
        // (e.g. an ASCII quote instead of the curly ones) would otherwise
        // silently render nothing instead of erroring, since
        // Printer::glyph_id_for_token treats an unencodable Char as a
        // skip, not a panic.
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
        // Senior review regression (rounds 3 and 4): these pages used to end
        // straight on `?` with no `PromptClear` before `End`, so each
        // vanished a few frames after its last glyph revealed instead of
        // holding on screen. All three must now end `?{P}` --
        // `Token::PromptClear` immediately before `Token::End` -- see
        // `AND_YOU_ARE`'s, `WHATS_YOUR_NAME`'s and `so_its_player`'s own doc
        // comments for the upstream mechanism each one stands in for (a
        // platform-fade sequence, an explicit button-wait task, and the
        // name-confirmation Yes/No menu respectively).
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
