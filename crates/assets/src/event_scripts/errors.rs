//! Typed failures of the addressed event-script payload.

use std::error::Error;
use std::fmt;

use super::CartridgeAddress;

/// A typed, deterministic payload failure. Validation runs in phases (framing,
/// spans, symbols, roots, map-table structure, cross-references, standard
/// table), each in wire order, so one malformed payload always reports the
/// same error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventScriptError {
    /// The payload ended inside the named section.
    Truncated {
        /// Section being read.
        section: &'static str,
    },
    /// The magic bytes are wrong.
    BadMagic,
    /// The schema version is not [`VERSION`].
    UnsupportedVersion(u16),
    /// A name is empty, too long, or not UTF-8.
    InvalidString {
        /// Section holding the name.
        section: &'static str,
    },
    /// Bytes remain after the last section.
    TrailingBytes,
    /// A symbol kind byte is undefined.
    UnknownSymbolKind(u8),
    /// A span has no bytes.
    EmptySpan,
    /// Spans are unordered, overlapping, or exceed the 32-bit address space.
    OverlappingSpans,
    /// A section is not strictly sorted.
    NotCanonical {
        /// The unsorted section.
        section: &'static str,
    },
    /// Two symbols share a name.
    DuplicateSymbol(String),
    /// A referenced symbol name does not exist.
    MissingSymbol(String),
    /// Two roots share an id.
    DuplicateRoot(String),
    /// A root names a symbol that is not a script.
    RootNotScript(String),
    /// A script address has no declared root.
    MissingRoot(CartridgeAddress),
    /// A script address has no script symbol.
    MissingScriptSymbol(CartridgeAddress),
    /// Two map tables share an id.
    DuplicateMapTable(String),
    /// A tagged record carries an undefined tag.
    BadTag {
        /// Table id.
        table: String,
        /// The tag byte.
        tag: u8,
    },
    /// A table lacks its terminator, or ends before its declared extent.
    BadTerminator {
        /// Table id.
        table: String,
    },
    /// A table's declared length cuts a record short.
    TruncatedRecord {
        /// Table id.
        table: String,
    },
    /// The standard table does not have [`STANDARD_SCRIPT_COUNT`] slots.
    BadStandardCount(u16),
    /// A standard slot does not name a declared script root.
    InvalidStandardScriptReference {
        /// Slot index.
        id: u8,
    },
    /// An address violates its alignment requirement.
    UnalignedAddress(CartridgeAddress),
    /// An address (or range) is not inside one span.
    OutOfStoreAddress(CartridgeAddress),
    /// A map table's symbol kind, extent, or a pointer into it disagrees.
    InconsistentTableReference {
        /// Table id.
        table: String,
    },
    /// Lookup of an unknown root id.
    UnknownRoot(String),
    /// Lookup of an unknown map-table id.
    UnknownMapTable(String),
    /// Lookup of a standard script slot past the table.
    UnknownStandardScript(u8),
}

impl fmt::Display for EventScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { section } => write!(f, "event-script payload truncated in {section}"),
            Self::BadMagic => f.write_str("event-script payload has bad magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported event-script version {v}"),
            Self::InvalidString { section } => write!(f, "invalid name in {section}"),
            Self::TrailingBytes => f.write_str("trailing bytes after event-script payload"),
            Self::UnknownSymbolKind(k) => write!(f, "unknown symbol kind {k}"),
            Self::EmptySpan => f.write_str("empty address span"),
            Self::OverlappingSpans => f.write_str("address spans are unordered or overlapping"),
            Self::NotCanonical { section } => write!(f, "{section} are not strictly sorted"),
            Self::DuplicateSymbol(n) => write!(f, "duplicate symbol {n}"),
            Self::MissingSymbol(n) => write!(f, "missing symbol {n}"),
            Self::DuplicateRoot(n) => write!(f, "duplicate root {n}"),
            Self::RootNotScript(n) => write!(f, "root {n} does not name a script symbol"),
            Self::MissingRoot(a) => write!(f, "no root for script at {:#010x}", a.0),
            Self::MissingScriptSymbol(a) => write!(f, "no script symbol at {:#010x}", a.0),
            Self::DuplicateMapTable(n) => write!(f, "duplicate map table {n}"),
            Self::BadTag { table, tag } => write!(f, "map table {table} has bad tag {tag}"),
            Self::BadTerminator { table } => write!(f, "map table {table} has a bad terminator"),
            Self::TruncatedRecord { table } => write!(f, "map table {table} ends mid-record"),
            Self::BadStandardCount(c) => write!(f, "standard table has {c} slots"),
            Self::InvalidStandardScriptReference { id } => {
                write!(f, "standard script {id} is not a declared root")
            }
            Self::UnalignedAddress(a) => write!(f, "unaligned address {:#010x}", a.0),
            Self::OutOfStoreAddress(a) => write!(f, "address {:#010x} is outside the store", a.0),
            Self::InconsistentTableReference { table } => {
                write!(f, "map table {table} is inconsistent")
            }
            Self::UnknownRoot(n) => write!(f, "unknown root {n}"),
            Self::UnknownMapTable(n) => write!(f, "unknown map table {n}"),
            Self::UnknownStandardScript(i) => write!(f, "unknown standard script {i}"),
        }
    }
}

impl Error for EventScriptError {}
