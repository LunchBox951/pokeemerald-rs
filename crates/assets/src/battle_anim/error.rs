//! Failures from constructing, encoding, or decoding the animation catalog.

use std::fmt;

use super::AnimationSelector;

/// A battle-animation catalog construction or decoding failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleAnimationError {
    /// The payload is incomplete.
    Truncated,
    /// The payload carries unread bytes after the last table. The value is how many.
    TrailingBytes(usize),
    /// The payload declares a schema version this reader does not know.
    UnsupportedVersion(u16),
    /// A command byte is not one of `sScriptCmdTable`'s 48 slots.
    UnknownOpcode(u8),
    /// A symbol is empty, too long, not UTF-8, or not a C identifier.
    InvalidSymbol,
    /// A `createsprite` target flag byte is neither 0 nor 1. The value is the byte.
    InvalidFlag(u8),
    /// An effect-sprite index is beyond `ANIM_TAG_BLUE_RING_2`. The value is that index.
    SpriteIdOutOfRange(u16),
    /// A selector index is beyond its table.
    SelectorOutOfRange {
        /// The table indexed.
        table: super::AnimationTable,
        /// The index beyond it.
        index: u16,
    },
    /// A program has no commands.
    EmptyProgram,
    /// A program has more commands than its `u16` count can hold. The value is the count.
    TooManyCommands(usize),
    /// A program's last command can fall off the end of the script.
    MissingTerminator,
    /// A command carries more arguments than `gBattleAnimArgs` holds. The value is the count.
    ArgCountTooLarge(usize),
    /// A command indexes `gBattleAnimArgs` out of range. The value is the index.
    ArgIndexOutOfRange(u8),
    /// A `createsprite` subpriority byte sets the `ANIMSPRITE_IS_TARGET` bit. The value is the byte.
    SubpriorityOutOfRange(u8),
    /// The catalog holds more programs than a `u16` id can name. The value is the count.
    TooManyPrograms(usize),
    /// Two programs share a label. The value is that label.
    DuplicateSymbol(String),
    /// A jump target or selector names a program the catalog lacks.
    ProgramOutOfRange {
        /// The program id named.
        id: u16,
        /// How many programs the catalog has.
        count: usize,
    },
    /// A selector is assigned two programs.
    DuplicateSelector(AnimationSelector),
    /// A selector has no program: the catalog offers no fallback.
    MissingSelector(AnimationSelector),
}

impl fmt::Display for BattleAnimationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "battle animation payload is truncated"),
            Self::TrailingBytes(count) => {
                write!(f, "battle animation payload has {count} trailing bytes")
            }
            Self::UnsupportedVersion(version) => {
                write!(
                    f,
                    "battle animation schema version {version} is unsupported"
                )
            }
            Self::UnknownOpcode(opcode) => {
                write!(f, "battle animation opcode {opcode:#04x} is not a command")
            }
            Self::InvalidSymbol => write!(f, "battle animation symbol is not a C identifier"),
            Self::InvalidFlag(byte) => {
                write!(f, "createsprite target flag {byte:#04x} is neither 0 nor 1")
            }
            Self::SpriteIdOutOfRange(index) => {
                write!(f, "effect sprite {index} is beyond the last animation tag")
            }
            Self::SelectorOutOfRange { table, index } => {
                write!(f, "selector {index} is beyond the {table} animation table")
            }
            Self::EmptyProgram => write!(f, "battle animation program has no commands"),
            Self::TooManyCommands(count) => {
                write!(f, "battle animation program has {count} commands")
            }
            Self::MissingTerminator => {
                write!(f, "battle animation program does not end in a terminator")
            }
            Self::ArgCountTooLarge(count) => {
                write!(
                    f,
                    "command carries {count} arguments, above the 8 upstream holds"
                )
            }
            Self::ArgIndexOutOfRange(index) => {
                write!(
                    f,
                    "command indexes argument {index}, above the 8 upstream holds"
                )
            }
            Self::SubpriorityOutOfRange(byte) => {
                write!(
                    f,
                    "createsprite subpriority {byte:#04x} sets the target bit"
                )
            }
            Self::TooManyPrograms(count) => {
                write!(f, "battle animation catalog holds {count} programs")
            }
            Self::DuplicateSymbol(symbol) => {
                write!(f, "two battle animation programs are labelled `{symbol}`")
            }
            Self::ProgramOutOfRange { id, count } => {
                write!(f, "program {id} is named but the catalog has {count}")
            }
            Self::DuplicateSelector(selector) => write!(
                f,
                "{} animation {} has two programs",
                selector.table(),
                selector.index()
            ),
            Self::MissingSelector(selector) => write!(
                f,
                "{} animation {} has no program",
                selector.table(),
                selector.index()
            ),
        }
    }
}

impl std::error::Error for BattleAnimationError {}
