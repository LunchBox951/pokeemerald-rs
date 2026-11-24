//! Building the opposing [`Trainer`] from the extracted pack tables.
//!
//! Upstream reads the trainer's class from `gTrainerClassNames` and its defeat
//! speech from the text the starting script names (`trainerbattle_*`), so the
//! speech is chosen by a script context, never by the trainer id: one trainer
//! id can carry several speeches. `GetTrainerALoseText` expands the speech's
//! `{PLAYER}` and `{KUN}` control placeholders when the battle asks for it
//! (`src/battle_setup.c:1517`-`:1528`); [`Trainer::from_tables`] does the same
//! up front, so [`Trainer::lose_text`] already holds the printed tokens and
//! `B_TXT_TRAINER1_LOSE_TEXT` only copies them.

use std::fmt;

use assets::pack::PackError;
use assets::{
    AssetPack, TrainerClass, TrainerClassNameTable, TrainerDefeatSpeechContext,
    TrainerDefeatSpeechTable,
};
use engine::save::PlayerGender;
use engine::text::format::{
    expand_placeholders, FormatError, PlaceholderResolver, StaticPlaceholders, PLACEHOLDER_ID_KUN,
    PLACEHOLDER_ID_PLAYER, PLACEHOLDER_ID_RIVAL,
};
use engine::text::{self, TextError, Token};

use super::context::Trainer;

/// Why a [`Trainer`] could not be built from the pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrainerTextError {
    /// The pack could not supply the class-name or defeat-speech table.
    Pack(PackError),
    /// The class name, speech, or player name does not decode or encode.
    Text(TextError),
    /// Placeholder expansion failed.
    Format(FormatError),
    /// The class identity names no row of `gTrainerClassNames`.
    UnknownClass(TrainerClass),
    /// The trainer identity names no `gTrainers` row.
    UnknownTrainer,
}

impl fmt::Display for TrainerTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(error) => write!(f, "trainer text pack table: {error}"),
            Self::Text(error) => write!(f, "trainer text: {error}"),
            Self::Format(error) => write!(f, "trainer text placeholders: {error}"),
            Self::UnknownClass(class) => write!(f, "no class name for {class:?}"),
            Self::UnknownTrainer => write!(f, "the trainer identity names no trainer"),
        }
    }
}

impl std::error::Error for TrainerTextError {}

impl From<PackError> for TrainerTextError {
    fn from(error: PackError) -> Self {
        Self::Pack(error)
    }
}

impl From<TextError> for TrainerTextError {
    fn from(error: TextError) -> Self {
        Self::Text(error)
    }
}

impl From<FormatError> for TrainerTextError {
    fn from(error: FormatError) -> Self {
        Self::Format(error)
    }
}

/// `StringExpandPlaceholders` over the runtime state a defeat speech can read
/// (`src/string_util.c:428`-`:520`): the player's name and gender, the names
/// that need no state, and nothing for the string-variable ids, which hold no
/// text when a trainer's speech is read.
struct SpeechPlaceholders {
    player_name: Vec<Token>,
    gender: PlayerGender,
}

/// `gText_ExpandedPlaceholder_Kun` (`src/strings.c:8`), empty in English.
const KUN: &str = "";
/// `gText_ExpandedPlaceholder_Chan` (`src/strings.c:9`), empty in English.
const CHAN: &str = "";

impl SpeechPlaceholders {
    /// `ExpandPlaceholder_KunChan` (`src/string_util.c:448`-`:454`): `Kun` for
    /// a male player and `Chan` for any other.
    const fn honorific(&self) -> &'static str {
        match self.gender {
            PlayerGender::Male => KUN,
            PlayerGender::Female | PlayerGender::Other(_) => CHAN,
        }
    }
}

impl PlaceholderResolver for SpeechPlaceholders {
    fn resolve(&self, id: u8) -> Option<Vec<Token>> {
        match id {
            PLACEHOLDER_ID_PLAYER => Some(self.player_name.clone()),
            PLACEHOLDER_ID_KUN => Some(without_end(
                text::decode(&text::encode_str(self.honorific()).ok()?).ok()?,
            )),
            PLACEHOLDER_ID_RIVAL => Some(rival_name(self.gender)),
            other => StaticPlaceholders.resolve(other),
        }
    }
}

/// `ExpandPlaceholder_RivalName` (`src/string_util.c:456`-`:462`).
fn rival_name(gender: PlayerGender) -> Vec<Token> {
    let name = if gender == PlayerGender::Male {
        "MAY"
    } else {
        "BRENDAN"
    };
    name.chars().map(Token::Char).collect()
}

fn without_end(mut tokens: Vec<Token>) -> Vec<Token> {
    if tokens.last() == Some(&Token::End) {
        tokens.pop();
    }
    tokens
}

impl Trainer {
    /// Builds the opposing trainer from the pack's two tables.
    ///
    /// `class` is the trainer's `gTrainers` class and `name` its display name.
    /// `speech` is the script context that names the defeat speech, passed
    /// explicitly so a sight trainer's script selects its own speech instead
    /// of it being inferred from the trainer id. `player_name` and `gender`
    /// are the runtime values `{PLAYER}` and `{KUN}` resolve to.
    ///
    /// # Errors
    ///
    /// [`TrainerTextError::UnknownClass`] for a class outside the table, and
    /// the text or format variants for a stream that does not decode or
    /// expand; nothing is replaced by empty text.
    pub fn from_tables(
        classes: &TrainerClassNameTable<'_>,
        speeches: &TrainerDefeatSpeechTable<'_>,
        class: TrainerClass,
        name: &str,
        speech: TrainerDefeatSpeechContext,
        player_name: &str,
        gender: PlayerGender,
    ) -> Result<Self, TrainerTextError> {
        let class_bytes = classes
            .name(class)
            .ok_or(TrainerTextError::UnknownClass(class))?;
        let resolver = SpeechPlaceholders {
            player_name: without_end(text::decode(&text::encode_str(player_name)?)?),
            gender,
        };
        let speech_tokens = text::decode(speeches.speech(speech))?;
        Ok(Self {
            class: without_end(text::decode(class_bytes)?),
            name: name.to_owned(),
            lose_text: without_end(expand_placeholders(&speech_tokens, &resolver)?),
        })
    }

    /// Builds the opposing trainer from the pack; see [`Trainer::from_tables`].
    ///
    /// # Errors
    ///
    /// [`TrainerTextError::Pack`] when the pack cannot supply a table, and
    /// the errors of [`Trainer::from_tables`].
    pub fn from_pack(
        pack: &AssetPack,
        class: TrainerClass,
        name: &str,
        speech: TrainerDefeatSpeechContext,
        player_name: &str,
        gender: PlayerGender,
    ) -> Result<Self, TrainerTextError> {
        Self::from_tables(
            &pack.trainer_class_names()?,
            &pack.trainer_defeat_speeches()?,
            class,
            name,
            speech,
            player_name,
            gender,
        )
    }
}

/// Synthetic pack tables for tests: made-up class names and speeches, so no
/// game text is ever a fixture.
#[cfg(test)]
pub(crate) mod synthetic {
    use assets::{
        TrainerClass, TrainerClassNameTable, TrainerDefeatSpeechContext, TrainerDefeatSpeechTable,
        TRAINER_CLASS_NAME_COUNT,
    };
    use engine::text::format::{
        PLACEHOLDER_ID_KUN, PLACEHOLDER_ID_PLAYER, PLACEHOLDER_ID_RIVAL,
        PLACEHOLDER_ID_STRING_VAR_1,
    };
    use engine::text::{self, Symbol, Token};

    /// The class and speech streams of a made-up pack, owned.
    pub(crate) struct Tables {
        classes: Vec<Vec<u8>>,
        speeches: Vec<Vec<u8>>,
    }

    impl Tables {
        /// Every class `ZQX` and every speech `Zq <context id>`.
        pub(crate) fn new() -> Self {
            let class = text::encode_str("ZQX").unwrap();
            let speeches = TrainerDefeatSpeechContext::ALL
                .iter()
                .map(|context| text::encode_str(&format!("Zq {}", context.id())).unwrap())
                .collect();
            Self {
                classes: vec![class; TRAINER_CLASS_NAME_COUNT],
                speeches,
            }
        }

        /// Replaces `class`'s name.
        pub(crate) fn with_class(mut self, class: TrainerClass, name: &[Token]) -> Self {
            self.classes[usize::from(class.0)] = encode(name);
            self
        }

        /// Replaces `context`'s speech with `source`, read as [`speech`].
        pub(crate) fn with_speech(
            mut self,
            context: TrainerDefeatSpeechContext,
            source: &str,
        ) -> Self {
            self.speeches[usize::from(context.id())] = encode(&speech(source));
            self
        }

        pub(crate) fn classes(&self) -> TrainerClassNameTable<'_> {
            let items = (0..).zip(self.classes.iter().map(Vec::as_slice));
            TrainerClassNameTable::from_entries(items).unwrap()
        }

        pub(crate) fn speeches(&self) -> TrainerDefeatSpeechTable<'_> {
            let items = (0..).zip(self.speeches.iter().map(Vec::as_slice));
            TrainerDefeatSpeechTable::from_entries(items).unwrap()
        }
    }

    /// The tokens of a name, terminated.
    fn encode(name: &[Token]) -> Vec<u8> {
        let mut tokens = name.to_vec();
        tokens.push(Token::End);
        text::encode(&tokens).unwrap()
    }

    /// The class name a rival's row holds: the `PK` and `MN` glyph tokens, a
    /// space, then made-up letters.
    pub(crate) fn glyph_class() -> Vec<Token> {
        let mut tokens = vec![Token::Symbol(Symbol::Pk), Token::Symbol(Symbol::Mn)];
        tokens.extend(" ZQXJVKW".chars().map(Token::Char));
        tokens
    }

    /// A speech from `source`: `{PLAYER}`, `{KUN}`, `{RIVAL}`, and `{STRING_VAR_1}` are
    /// the control placeholders, `\n` a line break, `\p` a page prompt, and
    /// any other character a glyph. The terminator is added.
    pub(crate) fn speech(source: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut chars = source.chars();
        while let Some(c) = chars.next() {
            tokens.push(match c {
                '{' => {
                    let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                    Token::Placeholder(match name.as_str() {
                        "PLAYER" => PLACEHOLDER_ID_PLAYER,
                        "KUN" => PLACEHOLDER_ID_KUN,
                        "RIVAL" => PLACEHOLDER_ID_RIVAL,
                        "STRING_VAR_1" => PLACEHOLDER_ID_STRING_VAR_1,
                        other => panic!("synthetic speech has no placeholder {other}"),
                    })
                }
                '\\' => match chars.next() {
                    Some('p') => Token::PromptClear,
                    other => panic!("only \\p is an escape, not {other:?}"),
                },
                '\n' => Token::Newline,
                glyph => Token::Char(glyph),
            });
        }
        tokens
    }
}
