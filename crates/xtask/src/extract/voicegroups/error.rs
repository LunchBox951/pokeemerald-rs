//! [`VoiceGroupError`]: every way parsing or resolving a voicegroup's
//! dependency tree can fail. Split out of [`super::parser`] to keep that
//! file under this crate's ~600-line-per-file guideline (`oop-boundaries`).

use std::fmt;

/// An error parsing one voicegroup `.inc` file, `keysplit_tables.inc`, or
/// resolving a voicegroup's key-split/rhythm dependency tree. Every variant
/// that can name a specific source line does; the outer
/// `ExtractError::VoiceGroupFile` wraps a parse-time variant with the
/// offending path (see `super`'s module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VoiceGroupError {
    /// The file had no `voice_group` declaration line at all (every real
    /// line was blank or an `@` comment).
    MissingDeclaration,
    /// A `voice_group` declaration's `starting_note` operand was present
    /// but not a valid `u8`.
    BadStartingNote,
    /// A body line didn't parse as any recognized `voice_*` macro
    /// invocation (missing/extra operands, or a non-numeric numeric
    /// operand). Carries the owning group's label and the raw line.
    MalformedLine { group: String, line: String },
    /// A body line invoked a macro name this parser does not recognize.
    /// Carries the owning group's label and the macro name.
    UnknownMacro { group: String, macro_name: String },
    /// A `voice_keysplit`/`voice_keysplit_all` operand didn't carry the
    /// expected `voicegroup_`/`keysplit_` symbol prefix. Carries the owning
    /// group's label, the raw operand, and the expected prefix.
    MalformedReference {
        group: String,
        reference: String,
        expected_prefix: &'static str,
    },
    /// A `ProgrammableWaveData_<n>` symbol carried the expected prefix but
    /// a suffix that is not a number, so no
    /// `audio/sample/programmable-wave/<nn>` id can be derived from it --
    /// the numeric half of the same fail-closed check
    /// [`Self::MalformedReference`] covers the prefix half of (see
    /// [`super::resolve`]'s "Sample id scheme"). Carries the owning group's
    /// label and the raw symbol.
    MalformedProgrammableWaveIndex { group: String, reference: String },
    /// A `keysplit` declaration line in `keysplit_tables.inc` was missing
    /// its label operand.
    MalformedKeySplitTableHeader,
    /// A `keysplit` declaration's `starting_note` operand was present but
    /// not a valid `u8`.
    BadKeySplitStartingNote,
    /// A `split` line appeared before any `keysplit` declaration.
    SplitOutsideKeySplit,
    /// A `split` line's `index`/`ending_note` operand wasn't a valid `u8`.
    /// Carries the owning keysplit table's label.
    MalformedSplitLine(String),
    /// A `split` line's `ending_note` was smaller than the table's running
    /// note cursor (the previous `split`'s `ending_note`, or the table's
    /// own `starting_note` for the first `split`). Carries the owning
    /// keysplit table's label.
    SplitOutOfOrder(String),
    /// A `keysplit_tables.inc` line invoked a macro name this parser does
    /// not recognize (only `keysplit`/`split` are). Fails closed for the
    /// same reason [`Self::UnknownMacro`] does on the voicegroup side: a
    /// silently-skipped line is a silently-wrong table. Carries the macro
    /// name.
    UnknownKeySplitMacro(String),
    /// Two `keysplit` blocks in `keysplit_tables.inc` declared the same
    /// label.
    DuplicateKeySplitTable(String),
    /// A `keysplit_tables.inc` table's expanded length exceeded
    /// [`super::VOICE_SLOT_COUNT`] (128) — a table entry selects a slot
    /// index in a group of at most that many slots. Carries the table's
    /// label and its expanded length.
    KeySplitTableTooLong { label: String, len: usize },
    /// A `voice_keysplit`/`voice_keysplit_all` reference named a voicegroup
    /// label with no corresponding `.inc` file anywhere under
    /// `sound/voicegroups/`. Carries the referring group's label and the
    /// dangling target label.
    DanglingVoiceGroupReference { referrer: String, target: String },
    /// A `voice_keysplit` reference named a keysplit table label with no
    /// corresponding block in `keysplit_tables.inc`. Carries the referring
    /// group's label and the dangling table label.
    DanglingKeySplitTableReference { referrer: String, target: String },
    /// Resolving a voicegroup's key-split/rhythm reference revisited a
    /// group already in the middle of being resolved -- a cycle. Carries
    /// the full label path, ending with the label that closes the loop.
    Cycle(Vec<String>),
    /// A group referenced *as* a key-split/rhythm child itself contained a
    /// key-split/rhythm slot. Upstream's `ply_note` aborts the note rather
    /// than recursing through a second level of indirection
    /// (`crates/assets/src/audio/voicegroup.rs`'s module docs), so this
    /// pipeline rejects it at extraction instead of silently flattening or
    /// looping. Carries the parent (referencing) label and the nested
    /// child label.
    NestedIndirection { parent: String, child: String },
    /// A voicegroup's `starting_note` bias plus its declared slot count
    /// exceeded [`super::VOICE_SLOT_COUNT`] (128). Carries the label, the
    /// starting note, and the slot count.
    TooManySlots {
        group: String,
        starting_note: u8,
        slot_count: usize,
    },
}

impl fmt::Display for VoiceGroupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDeclaration => {
                write!(f, "no `voice_group` declaration line found")
            }
            Self::BadStartingNote => {
                write!(
                    f,
                    "`voice_group` declaration's starting_note operand is not a valid u8"
                )
            }
            Self::MalformedLine { group, line } => {
                write!(f, "voicegroup `{group}`: malformed line: `{line}`")
            }
            Self::UnknownMacro { group, macro_name } => {
                write!(f, "voicegroup `{group}`: unrecognized macro `{macro_name}`")
            }
            Self::MalformedReference {
                group,
                reference,
                expected_prefix,
            } => write!(
                f,
                "voicegroup `{group}`: reference `{reference}` does not start with expected \
                 prefix `{expected_prefix}`"
            ),
            Self::MalformedProgrammableWaveIndex { group, reference } => write!(
                f,
                "voicegroup `{group}`: programmable-wave symbol `{reference}` does not end in a \
                 sample number"
            ),
            Self::MalformedKeySplitTableHeader => {
                write!(
                    f,
                    "keysplit_tables.inc: `keysplit` line is missing its label"
                )
            }
            Self::BadKeySplitStartingNote => write!(
                f,
                "keysplit_tables.inc: `keysplit` declaration's starting_note operand is not a \
                 valid u8"
            ),
            Self::SplitOutsideKeySplit => write!(
                f,
                "keysplit_tables.inc: `split` line appears before any `keysplit` declaration"
            ),
            Self::MalformedSplitLine(label) => write!(
                f,
                "keysplit table `{label}`: malformed `split` line (index/ending_note not a valid \
                 u8)"
            ),
            Self::SplitOutOfOrder(label) => write!(
                f,
                "keysplit table `{label}`: a `split` line's ending_note is earlier than the \
                 running note cursor"
            ),
            Self::UnknownKeySplitMacro(macro_name) => {
                write!(f, "keysplit_tables.inc: unrecognized macro `{macro_name}`")
            }
            Self::DuplicateKeySplitTable(label) => write!(
                f,
                "keysplit_tables.inc: duplicate `keysplit` label `{label}`"
            ),
            Self::KeySplitTableTooLong { label, len } => write!(
                f,
                "keysplit table `{label}`: expanded length {len} exceeds the maximum of {}",
                super::VOICE_SLOT_COUNT
            ),
            Self::DanglingVoiceGroupReference { referrer, target } => write!(
                f,
                "voicegroup `{referrer}` references unknown voicegroup `{target}` (no matching \
                 `voice_group {target}` declaration found under sound/voicegroups/)"
            ),
            Self::DanglingKeySplitTableReference { referrer, target } => write!(
                f,
                "voicegroup `{referrer}` references unknown keysplit table `{target}` (no \
                 matching `keysplit {target}` block found in keysplit_tables.inc)"
            ),
            Self::Cycle(path) => write!(
                f,
                "voicegroup reference cycle detected: {}",
                path.join(" -> ")
            ),
            Self::NestedIndirection { parent, child } => write!(
                f,
                "voicegroup `{parent}` references `{child}` as a key-split/rhythm child, but \
                 `{child}` itself contains a key-split/rhythm slot -- upstream's ply_note aborts \
                 rather than recursing through a second level of indirection"
            ),
            Self::TooManySlots {
                group,
                starting_note,
                slot_count,
            } => write!(
                f,
                "voicegroup `{group}`: starting_note {starting_note} + {slot_count} slots \
                 exceeds the maximum of {}",
                super::VOICE_SLOT_COUNT
            ),
        }
    }
}

impl std::error::Error for VoiceGroupError {}
