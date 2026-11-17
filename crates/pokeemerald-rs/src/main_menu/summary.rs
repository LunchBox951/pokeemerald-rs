//! The CONTINUE window's saved-game summary: the PLAYER, TIME and BADGES lines
//! `MainMenu_FormatSavegameText` prints (`pokeemerald/src/main_menu.c:2127-2190`).
//!
//! The `POKéDEX` line is not modelled: the save has no caught count yet.

use engine::save::{PlayTime, PlayerGender, SaveBlock1, SaveBlock2};
use engine::text::format::{convert_int_to_decimal_string_n, StringConvertMode};
use engine::text::Token;
use rendering::{Bgr555, Rgb888};

use super::items::{BADGES_LABEL, PLAYER_LABEL, TIME_LABEL};

/// `FLAG_BADGE01_GET` (`SYSTEM_FLAGS + 0x7`, `include/constants/flags.h:1359`).
const FIRST_BADGE_FLAG: u16 = 0x867;
/// `NUM_BADGES`: `FLAG_BADGE01_GET` through `FLAG_BADGE08_GET`.
const BADGE_COUNT: u16 = 8;

/// Window-local x of the left column's labels and of the right column's
/// labels (`0x6C`), and the edges the values right-align to.
const LEFT_LABEL_X: i32 = 0;
const RIGHT_LABEL_X: i32 = 0x6C;
const NAME_RIGHT_EDGE: i32 = 100;
const VALUE_RIGHT_EDGE: i32 = 0xD0;
/// The two rows the summary prints on (window-local y).
const FIRST_ROW_Y: i32 = 17;
const SECOND_ROW_Y: i32 = 33;

/// The hours field shows three digits and the minutes two.
const HOURS_DIGITS: u8 = 3;
const MINUTES_DIGITS: u8 = 2;

/// Palette 15 entry 1 is the info text's colour, which `Task_DisplayMainMenu`
/// sets from the player's gender (`main_menu.c:767-778`).
const MALE_TEXT: Rgb888 = Bgr555::from_channels(4, 16, 31).to_rgb888();
const FEMALE_TEXT: Rgb888 = Bgr555::from_channels(31, 3, 21).to_rgb888();

/// Where a line starts horizontally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineX {
    /// The string's left edge sits at this window-local x.
    Left(i32),
    /// The string's right edge sits at this window-local x
    /// (`GetStringRightAlignXOffset`).
    RightEdge(i32),
}

/// One string the summary prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SummaryLine {
    pub(super) x: LineX,
    pub(super) y: i32,
    pub(super) tokens: Vec<Token>,
}

/// What the CONTINUE window shows about the loaded save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SaveSummary {
    name: Vec<Token>,
    gender: PlayerGender,
    play_time: PlayTime,
    badges: u8,
}

impl SaveSummary {
    /// Reads the summary from the recovered save blocks.
    pub(crate) fn from_save(block1: &SaveBlock1, block2: &SaveBlock2) -> Self {
        let mut name = engine::text::decode(&block2.player_name).unwrap_or_else(|err| {
            // A checksummed save can carry bytes no glyph names; show no name
            // rather than refuse the whole menu, and say so.
            eprintln!("main menu: the saved player name does not decode ({err:?}); showing none");
            Vec::new()
        });
        name.retain(|token| *token != Token::End);
        Self {
            name,
            gender: block2.player_gender,
            play_time: block2.play_time,
            badges: badge_count(block1),
        }
    }

    /// The info text colour: blue for a male save, pink for anything else, as
    /// the `playerGender == MALE` test chooses (`main_menu.c:769-778`).
    pub(super) const fn text_color(&self) -> Rgb888 {
        match self.gender {
            PlayerGender::Male => MALE_TEXT,
            PlayerGender::Female | PlayerGender::Other(_) => FEMALE_TEXT,
        }
    }

    /// The strings to print, in upstream's order.
    pub(super) fn lines(&self) -> [SummaryLine; 6] {
        let line = |x, y, tokens| SummaryLine { x, y, tokens };
        [
            line(LineX::Left(LEFT_LABEL_X), FIRST_ROW_Y, label(PLAYER_LABEL)),
            line(
                LineX::RightEdge(NAME_RIGHT_EDGE),
                FIRST_ROW_Y,
                self.name.clone(),
            ),
            line(LineX::Left(RIGHT_LABEL_X), FIRST_ROW_Y, label(TIME_LABEL)),
            line(
                LineX::RightEdge(VALUE_RIGHT_EDGE),
                FIRST_ROW_Y,
                time_tokens(self.play_time),
            ),
            line(
                LineX::Left(RIGHT_LABEL_X),
                SECOND_ROW_Y,
                label(BADGES_LABEL),
            ),
            line(
                LineX::RightEdge(VALUE_RIGHT_EDGE),
                SECOND_ROW_Y,
                decimal(i32::from(self.badges), StringConvertMode::LeadingZeros, 1),
            ),
        ]
    }
}

fn label(text: &str) -> Vec<Token> {
    text.chars().map(Token::Char).collect()
}

fn decimal(value: i32, mode: StringConvertMode, width: u8) -> Vec<Token> {
    convert_int_to_decimal_string_n(value, mode, width)
        .expect("every summary field's width is within the converter's 1..=10")
}

/// `H:MM`: the hours left-aligned in three digits, the colon glyph, then the
/// minutes zero-padded to two.
pub(super) fn time_tokens(time: PlayTime) -> Vec<Token> {
    let mut tokens = decimal(
        i32::from(time.hours),
        StringConvertMode::LeftAlign,
        HOURS_DIGITS,
    );
    tokens.push(Token::Char(':'));
    tokens.extend(decimal(
        i32::from(time.minutes),
        StringConvertMode::LeadingZeros,
        MINUTES_DIGITS,
    ));
    tokens
}

/// The earned badges among the eight consecutive badge flags.
pub(super) fn badge_count(block1: &SaveBlock1) -> u8 {
    let earned = (FIRST_BADGE_FLAG..FIRST_BADGE_FLAG + BADGE_COUNT)
        .filter(|&flag| block1.event_data.flag_get(flag).unwrap_or(false))
        .count();
    u8::try_from(earned).expect("at most eight badge flags are counted")
}
