use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VoiceGroupError {
    MissingVoiceGroupDeclaration,
    InvalidVoiceGroupStartingNote,
    MalformedVoiceSlot {
        group: String,
        line: String,
    },
    UnknownVoiceMacro {
        group: String,
        macro_name: String,
    },
    MalformedReference {
        group: String,
        reference: String,
        expected_prefix: &'static str,
    },
    MalformedProgrammableWaveIndex {
        group: String,
        reference: String,
    },
    MissingKeySplitLabel,
    InvalidKeySplitStartingNote,
    SplitBeforeKeySplit,
    InvalidSplitOperands {
        table: String,
    },
    SplitOutOfOrder {
        table: String,
    },
    UnknownKeySplitMacro {
        macro_name: String,
    },
    DuplicateKeySplitTable {
        label: String,
    },
    KeySplitTableTooLong {
        label: String,
        expanded_len: usize,
    },
    DanglingVoiceGroupReference {
        referrer: String,
        target: String,
    },
    DanglingKeySplitTableReference {
        referrer: String,
        target: String,
    },
    Cycle(Vec<String>),
    NestedIndirection {
        parent: String,
        child: String,
    },
    TooManySlots {
        group: String,
        starting_note: u8,
        slot_count: usize,
    },
    UnindexedLinkOrderFile(String),
}

impl fmt::Display for VoiceGroupError {
    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive match keeps every voicegroup error message together"
    )]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVoiceGroupDeclaration => {
                write!(f, "no `voice_group` declaration line found")
            }
            Self::InvalidVoiceGroupStartingNote => {
                write!(
                    f,
                    "`voice_group` declaration's starting_note operand is not a valid u8"
                )
            }
            Self::MalformedVoiceSlot { group, line } => {
                write!(f, "voicegroup `{group}`: malformed line: `{line}`")
            }
            Self::UnknownVoiceMacro { group, macro_name } => {
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
            Self::MissingKeySplitLabel => {
                write!(
                    f,
                    "keysplit_tables.inc: `keysplit` line is missing its label"
                )
            }
            Self::InvalidKeySplitStartingNote => write!(
                f,
                "keysplit_tables.inc: `keysplit` declaration's starting_note operand is not a \
                 valid u8"
            ),
            Self::SplitBeforeKeySplit => write!(
                f,
                "keysplit_tables.inc: `split` line appears before any `keysplit` declaration"
            ),
            Self::InvalidSplitOperands { table } => write!(
                f,
                "keysplit table `{table}`: malformed `split` line (index/ending_note not a valid \
                 u8)"
            ),
            Self::SplitOutOfOrder { table } => write!(
                f,
                "keysplit table `{table}`: a `split` line's ending_note is earlier than the \
                 running note cursor"
            ),
            Self::UnknownKeySplitMacro { macro_name } => {
                write!(f, "keysplit_tables.inc: unrecognized macro `{macro_name}`")
            }
            Self::DuplicateKeySplitTable { label } => write!(
                f,
                "keysplit_tables.inc: duplicate `keysplit` label `{label}`"
            ),
            Self::KeySplitTableTooLong {
                label,
                expanded_len,
            } => write!(
                f,
                "keysplit table `{label}`: expanded length {expanded_len} exceeds the maximum of {}",
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
            Self::UnindexedLinkOrderFile(path) => write!(
                f,
                "sound/voice_groups.inc links `sound/voicegroups/{path}`, but no parsed \
                 voicegroup declares that file (directory walk vs. linker order mismatch)"
            ),
        }
    }
}

impl std::error::Error for VoiceGroupError {}
