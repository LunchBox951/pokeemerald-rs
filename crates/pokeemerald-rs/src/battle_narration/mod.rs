//! Battle narration: ordered requests for the battle strings a turn prints, and
//! their expansion into [`engine::text::Token`]s.
//!
//! [`Narrator`] turns each [`battle::BattleEvent`] into the
//! [`NarrationRequest`]s upstream prints for it, in print order. Each request
//! owns a [`NarrationContext`] snapshot taken when it is made, as
//! `BtlController_EmitPrintString` copies state before printing
//! (`src/battle_controllers.c:1136`-`:1165`). [`expand`] later resolves one
//! request against a [`assets::pack::BattleMessageTable`] without reading live
//! battle state.
//!
//! Placement: `battle` and `engine` both depend only on `assets`, so the layer
//! that joins a battle event to engine tokens belongs to this crate, which
//! depends on all three; neither crate needs a new edge.
//!
//! Expansion follows `BattleStringExpandPlaceholders`
//! (`src/battle_message.c:2312`-`:2764`): a template placeholder is replaced by
//! its source, the three battle buffers are first run as buffer programs
//! (`ExpandBattleTextBuffPlaceholders`, `:2776`-`:2873`), and the replacement
//! is copied without being expanded again.
//!
//! Supported: single battles against a wild Pokémon or a trainer. A placeholder
//! only link, multi, frontier, or secret-base battles reach is an
//! [`NarrationError::UnsupportedPlaceholder`].

mod context;
mod events;
mod expand;
mod lifecycle;
mod names;
mod narrator;
mod string_ids;

pub use context::{
    BattleBuffer, BattleKind, Battler, BufferOp, NarrationContext, Side, StatusName, Trainer,
};
pub use expand::expand;
pub use names::{AssetNames, NarrationNames};
pub use narrator::{Narrator, Scene};

use std::error::Error;
use std::fmt;

use assets::pack::BattleMessageError;
use engine::text::format::FormatError;
use engine::text::TextError;

/// Which template a request prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateId {
    /// A `gBattleStringsTable` entry by battle-string ID.
    Table(u16),
    /// A `BufferStringBattle` payload by upstream symbol, such as
    /// `sText_AttackerUsedX` for `STRINGID_USEDMOVE`.
    Special(&'static str),
}

/// One message to print: a template and the state it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrationRequest {
    /// The template to expand.
    pub template: TemplateId,
    /// The state snapshot taken when the request was made.
    pub context: NarrationContext,
}

/// Why a request could not be built or expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NarrationError {
    /// The message table rejected a lookup.
    Table(BattleMessageError),
    /// No special template carries this symbol.
    UnknownSpecial(String),
    /// A template or name could not be decoded or encoded.
    Text(TextError),
    /// A number could not be formatted.
    Format(FormatError),
    /// The template reads a context field the request left unset.
    MissingContext(&'static str),
    /// A name table has no entry for an identifier.
    UnknownName(&'static str),
    /// The placeholder selector belongs to a battle type this layer does not model.
    UnsupportedPlaceholder(u8),
    /// A status buffer sat in a buffer other than buffer 1.
    StatusOutsideBuffer1,
    /// The event has no narration mapping.
    UnmappedEvent,
}

impl fmt::Display for NarrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Table(error) => write!(f, "battle message table: {error:?}"),
            Self::UnknownSpecial(symbol) => write!(f, "no special template named {symbol}"),
            Self::Text(error) => write!(f, "narration text: {error}"),
            Self::Format(error) => write!(f, "narration number: {error}"),
            Self::MissingContext(field) => write!(f, "narration context has no {field}"),
            Self::UnknownName(table) => write!(f, "no {table} entry for the identifier"),
            Self::UnsupportedPlaceholder(selector) => {
                write!(f, "placeholder {selector:#04x} is not modelled")
            }
            Self::UnmappedEvent => write!(f, "the battle event has no narration mapping"),
            Self::StatusOutsideBuffer1 => write!(f, "a status buffer is only valid in buffer 1"),
        }
    }
}

impl Error for NarrationError {}

impl From<BattleMessageError> for NarrationError {
    fn from(error: BattleMessageError) -> Self {
        Self::Table(error)
    }
}

impl From<TextError> for NarrationError {
    fn from(error: TextError) -> Self {
        Self::Text(error)
    }
}

impl From<FormatError> for NarrationError {
    fn from(error: FormatError) -> Self {
        Self::Format(error)
    }
}

#[cfg(test)]
mod tests {
    mod coverage;
    mod expansion;
    mod families;
    mod fixtures;
}
