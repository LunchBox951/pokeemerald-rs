//! Typed failures from parsing and validating the Pokédex tables.

use std::fmt;

use assets::pokedex::{PokedexError, PokedexOrderKind};

/// Which authority a failure concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PokedexSource {
    /// `src/data/pokemon/pokedex_entries.h`.
    Entries,
    /// `src/data/pokemon/pokedex_text.h`.
    Text,
    /// `src/data/pokemon/pokedex_orders.h`.
    Orders,
    /// `include/constants/pokedex.h`.
    Constants,
}

impl PokedexSource {
    /// The checkout-relative path of the authority.
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::Entries => "src/data/pokemon/pokedex_entries.h",
            Self::Text => "src/data/pokemon/pokedex_text.h",
            Self::Orders => "src/data/pokemon/pokedex_orders.h",
            Self::Constants => "include/constants/pokedex.h",
        }
    }
}

/// A Pokédex parse or closure failure. Every fault has its own variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PokedexSourceError {
    /// A token or construct outside the accepted grammar.
    Malformed {
        source: PokedexSource,
        line: usize,
        expected: &'static str,
    },
    /// A numeric literal that is not a plain decimal number.
    MalformedNumber {
        source: PokedexSource,
        line: usize,
        text: String,
    },
    /// A decimal literal outside the range of its field.
    NumberOutOfRange {
        source: PokedexSource,
        line: usize,
        field: String,
        text: String,
    },
    /// A string literal with an escape other than `\n`, `\\`, or `\"`.
    UnsupportedEscape {
        source: PokedexSource,
        line: usize,
        escape: char,
    },
    /// A string literal that never closes.
    UnterminatedString { source: PokedexSource, line: usize },
    /// A block comment that never closes.
    UnterminatedComment { source: PokedexSource, line: usize },
    /// A National Dex constant declared twice.
    DuplicateConstant { name: String },
    /// The enum does not hold exactly `NATIONAL_DEX_NONE` through the last
    /// old-Unown identity.
    ConstantCount { expected: usize, actual: usize },
    /// The enum's first constant is not `NATIONAL_DEX_NONE`.
    FirstConstantNotNone { name: String },
    /// A count alias is absent.
    MissingCountAlias { name: &'static str },
    /// A count alias names an unknown constant or one with the wrong value.
    WrongCountAlias {
        name: &'static str,
        expected: u16,
        actual: Option<u16>,
    },
    /// A symbol used by an entry key or order does not name a constant.
    UnresolvedConstant {
        source: PokedexSource,
        line: usize,
        symbol: String,
    },
    /// Two entry rows share one designated key.
    DuplicateEntry { symbol: String },
    /// A National Dex number up to `NATIONAL_DEX_COUNT` has no entry row.
    MissingEntry { dex_no: u16 },
    /// An entry row sets one member twice.
    DuplicateEntryMember { symbol: String, member: String },
    /// An entry row omits a persisted member.
    MissingEntryMember {
        symbol: String,
        member: &'static str,
    },
    /// An entry names a member outside the eight persisted ones.
    UnknownEntryMember {
        source: PokedexSource,
        line: usize,
        member: String,
    },
    /// Two text definitions share a symbol.
    DuplicateText { symbol: String },
    /// An entry's `description` names no text definition.
    UnresolvedDescription { entry: String, symbol: String },
    /// An order initializer appears twice.
    DuplicateOrder { order: PokedexOrderKind },
    /// An order initializer is absent.
    MissingOrder { order: PokedexOrderKind },
    /// An array in the orders file is not one of the three orders.
    UnknownOrder { line: usize, symbol: String },
    /// An order has the wrong number of members.
    OrderLength {
        order: PokedexOrderKind,
        expected: usize,
        actual: usize,
    },
    /// The assembled payload broke a rule `assets::pokedex` enforces, such as a
    /// repeated order member.
    Payload(PokedexError),
}

#[expect(clippy::too_many_lines, reason = "one arm per typed fault")]
impl fmt::Display for PokedexSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed {
                source,
                line,
                expected,
            } => write!(f, "{}:{line}: expected {expected}", source.path()),
            Self::MalformedNumber { source, line, text } => {
                write!(
                    f,
                    "{}:{line}: `{text}` is not a decimal number",
                    source.path()
                )
            }
            Self::NumberOutOfRange {
                source,
                line,
                field,
                text,
            } => write!(
                f,
                "{}:{line}: `{text}` is out of range for {field}",
                source.path()
            ),
            Self::UnsupportedEscape {
                source,
                line,
                escape,
            } => write!(
                f,
                "{}:{line}: unsupported escape `\\{escape}`",
                source.path()
            ),
            Self::UnterminatedString { source, line } => {
                write!(f, "{}:{line}: unterminated string", source.path())
            }
            Self::UnterminatedComment { source, line } => {
                write!(f, "{}:{line}: unterminated comment", source.path())
            }
            Self::DuplicateConstant { name } => write!(f, "constant `{name}` is declared twice"),
            Self::ConstantCount { expected, actual } => {
                write!(
                    f,
                    "National Dex enum holds {actual} constants, expected {expected}"
                )
            }
            Self::FirstConstantNotNone { name } => {
                write!(
                    f,
                    "National Dex enum starts at `{name}`, not NATIONAL_DEX_NONE"
                )
            }
            Self::MissingCountAlias { name } => write!(f, "count alias `{name}` is missing"),
            Self::WrongCountAlias {
                name,
                expected,
                actual,
            } => match actual {
                Some(actual) => write!(f, "`{name}` resolves to {actual}, expected {expected}"),
                None => write!(f, "`{name}` names no National Dex constant"),
            },
            Self::UnresolvedConstant {
                source,
                line,
                symbol,
            } => write!(
                f,
                "{}:{line}: `{symbol}` names no National Dex constant",
                source.path()
            ),
            Self::DuplicateEntry { symbol } => write!(f, "entry `{symbol}` appears twice"),
            Self::MissingEntry { dex_no } => {
                write!(f, "National Dex number {dex_no} has no entry row")
            }
            Self::DuplicateEntryMember { symbol, member } => {
                write!(f, "entry `{symbol}` sets `{member}` twice")
            }
            Self::MissingEntryMember { symbol, member } => {
                write!(f, "entry `{symbol}` omits `{member}`")
            }
            Self::UnknownEntryMember {
                source,
                line,
                member,
            } => write!(
                f,
                "{}:{line}: unknown entry member `{member}`",
                source.path()
            ),
            Self::DuplicateText { symbol } => write!(f, "text `{symbol}` is defined twice"),
            Self::UnresolvedDescription { entry, symbol } => {
                write!(
                    f,
                    "entry `{entry}` describes itself with undefined text `{symbol}`"
                )
            }
            Self::DuplicateOrder { order } => write!(f, "{order} order is defined twice"),
            Self::MissingOrder { order } => write!(f, "{order} order is missing"),
            Self::UnknownOrder { line, symbol } => write!(
                f,
                "{}:{line}: `{symbol}` is not a Pokédex order",
                PokedexSource::Orders.path()
            ),
            Self::OrderLength {
                order,
                expected,
                actual,
            } => write!(f, "{order} order has {actual} members, expected {expected}"),
            Self::Payload(error) => write!(f, "Pokédex payload rejected: {error}"),
        }
    }
}

impl std::error::Error for PokedexSourceError {}

impl From<PokedexError> for PokedexSourceError {
    fn from(error: PokedexError) -> Self {
        Self::Payload(error)
    }
}
