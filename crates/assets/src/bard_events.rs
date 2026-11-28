//! Bard phoneme templates for the Events Easy Chat word group.
//!
//! Source: `sBardSoundTemplates_Events` in `src/data/bard_music/events.h`, the
//! `BardSoundTemplate` layout in `include/bard_music.h`, and the Hold and End
//! markers in `src/bard_music.c`. Playback, pitch and length calculation, the
//! other word groups and the group dispatch are not part of this table.

use std::error::Error;
use std::fmt;

/// The number of words in the Events group.
pub const EVENTS_WORD_COUNT: usize = 29;

/// The number of template slots per word (`MAX_BARD_SOUNDS_PER_WORD`).
pub const BARD_SOUNDS_PER_WORD: usize = 6;

/// The number of phoneme songs; also the upstream song id of a [`BardSlot::Hold`].
const PHONEME_SONG_COUNT: u8 = 51;

/// The upstream song id of a [`BardSlot::End`] (`PHONEME_ID_NONE`).
const END_SONG_ID: u8 = 0xFF;

/// A failed lookup in the Events Bard templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BardEventsError {
    /// The value is not below [`EVENTS_WORD_COUNT`].
    UnknownEventsWord(u8),
}

impl fmt::Display for BardEventsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEventsWord(index) => {
                write!(f, "unknown events bard word index `{index}`")
            }
        }
    }
}

impl Error for BardEventsError {}

/// A word's index within the Events group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventsWordIndex(u8);

impl EventsWordIndex {
    /// Validates a raw group-local index (not a packed `EC_WORD_*` value).
    ///
    /// # Errors
    ///
    /// Returns [`BardEventsError::UnknownEventsWord`] when `index` is
    /// [`EVENTS_WORD_COUNT`] or more.
    pub const fn new(index: u8) -> Result<Self, BardEventsError> {
        if (index as usize) < EVENTS_WORD_COUNT {
            Ok(Self(index))
        } else {
            Err(BardEventsError::UnknownEventsWord(index))
        }
    }

    /// Returns the raw group index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// What a template slot asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BardSlot {
    /// A phoneme song, as an id relative to the first phoneme song (0 through 50).
    Phoneme(u8),
    /// Hold the previous phoneme (`PREV_BARD_SOUND`).
    Hold,
    /// The word has no further sounds (`NULL_BARD_SOUND`).
    End,
}

impl BardSlot {
    /// Returns the upstream `songId` byte: the phoneme id, 51 for a hold, or 255 for an end.
    #[must_use]
    pub const fn song_id(self) -> u8 {
        match self {
            Self::Phoneme(id) => id,
            Self::Hold => PHONEME_SONG_COUNT,
            Self::End => END_SONG_ID,
        }
    }
}

/// One `BardSoundTemplate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BardSoundTemplate {
    /// The slot's meaning (`songId`).
    pub slot: BardSlot,
    /// The phoneme length adjustment (`lengthAdjustment`); zero for every Events slot.
    pub length_adjustment: i8,
    /// The `unused` field upstream never reads; zero for every Events slot.
    pub unused: u16,
    /// The phoneme volume (`volume`); zero for every Events slot.
    pub volume: i16,
}

/// Lookup over the Events Bard templates.
#[derive(Debug, Clone, Copy)]
pub struct EventsBardTemplates {
    rows: &'static [[BardSoundTemplate; BARD_SOUNDS_PER_WORD]; EVENTS_WORD_COUNT],
}

impl EventsBardTemplates {
    /// Builds the canonical Events template table.
    #[must_use]
    pub const fn new() -> Self {
        Self { rows: &TEMPLATES }
    }

    /// Returns the number of words.
    #[must_use]
    pub const fn len(&self) -> usize {
        EVENTS_WORD_COUNT
    }

    /// Returns `false`; the Events table is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the six slots of a validated word.
    #[must_use]
    pub fn get(
        &self,
        index: EventsWordIndex,
    ) -> &'static [BardSoundTemplate; BARD_SOUNDS_PER_WORD] {
        &self.rows[usize::from(index.index())]
    }

    /// Returns the six slots of the word at the raw group index `index`.
    ///
    /// # Errors
    ///
    /// Returns [`BardEventsError::UnknownEventsWord`] when `index` is
    /// [`EVENTS_WORD_COUNT`] or more.
    pub fn lookup(
        &self,
        index: u8,
    ) -> Result<&'static [BardSoundTemplate; BARD_SOUNDS_PER_WORD], BardEventsError> {
        Ok(self.get(EventsWordIndex::new(index)?))
    }
}

impl Default for EventsBardTemplates {
    fn default() -> Self {
        Self::new()
    }
}

const fn slot(slot: BardSlot) -> BardSoundTemplate {
    BardSoundTemplate {
        slot,
        length_adjustment: 0,
        unused: 0,
        volume: 0,
    }
}

const fn phoneme(id: u8) -> BardSoundTemplate {
    slot(BardSlot::Phoneme(id))
}

const HOLD: BardSoundTemplate = slot(BardSlot::Hold);
const END: BardSoundTemplate = slot(BardSlot::End);

static TEMPLATES: [[BardSoundTemplate; BARD_SOUNDS_PER_WORD]; EVENTS_WORD_COUNT] = [
    [phoneme(44), phoneme(12), HOLD, END, END, END],
    [phoneme(12), phoneme(11), HOLD, END, END, END],
    [phoneme(3), phoneme(0), phoneme(24), HOLD, HOLD, END],
    [phoneme(9), phoneme(12), HOLD, END, END, END],
    [phoneme(29), phoneme(11), HOLD, END, END, END],
    [phoneme(9), phoneme(14), HOLD, END, END, END],
    [phoneme(43), HOLD, HOLD, END, END, END],
    [phoneme(39), phoneme(50), HOLD, END, END, END],
    [phoneme(5), HOLD, HOLD, END, END, END],
    [phoneme(21), phoneme(44), phoneme(4), HOLD, END, END],
    [phoneme(26), phoneme(44), HOLD, END, END, END],
    [phoneme(18), phoneme(48), HOLD, END, END, END],
    [phoneme(2), phoneme(44), phoneme(33), phoneme(48), END, END],
    [phoneme(12), phoneme(48), HOLD, END, END, END],
    [phoneme(0), phoneme(44), phoneme(39), END, END, END],
    [phoneme(17), phoneme(9), HOLD, END, END, END],
    [phoneme(12), phoneme(9), phoneme(4), HOLD, END, END],
    [phoneme(10), HOLD, HOLD, END, END, END],
    [phoneme(24), phoneme(12), phoneme(29), HOLD, END, END],
    [phoneme(0), phoneme(48), HOLD, END, END, END],
    [phoneme(3), HOLD, HOLD, END, END, END],
    [phoneme(17), phoneme(44), HOLD, END, END, END],
    [phoneme(48), phoneme(42), END, END, END, END],
    [phoneme(48), phoneme(9), END, END, END, END],
    [phoneme(0), phoneme(48), END, END, END, END],
    [phoneme(39), phoneme(42), END, END, END, END],
    [phoneme(39), HOLD, END, END, END, END],
    [phoneme(18), phoneme(48), phoneme(9), HOLD, END, END],
    [phoneme(42), phoneme(12), phoneme(48), END, END, END],
];

#[cfg(test)]
mod tests {
    use super::*;

    // Row i is Events word i; `<VOWEL>_<BLEND|HELD|SOLO>` names a phoneme.
    const EXPECTED: [[&str; 6]; 29] = [
        ["STRUT_SOLO", "FLEECE_BLEND", "HOLD", "END", "END", "END"],
        ["FLEECE_BLEND", "DRESS_SOLO", "HOLD", "END", "END", "END"],
        [
            "FACE_BLEND",
            "TRAP_BLEND",
            "GOAT_BLEND",
            "HOLD",
            "HOLD",
            "END",
        ],
        ["DRESS_BLEND", "FLEECE_BLEND", "HOLD", "END", "END", "END"],
        ["THOUGHT_SOLO", "DRESS_SOLO", "HOLD", "END", "END", "END"],
        ["DRESS_BLEND", "FLEECE_SOLO", "HOLD", "END", "END", "END"],
        ["STRUT_HELD", "HOLD", "HOLD", "END", "END", "END"],
        ["GOOSE_BLEND", "NURSE_SOLO", "HOLD", "END", "END", "END"],
        ["FACE_SOLO", "HOLD", "HOLD", "END", "END", "END"],
        ["LOT_BLEND", "STRUT_SOLO", "FACE_HELD", "HOLD", "END", "END"],
        ["GOAT_SOLO", "STRUT_SOLO", "HOLD", "END", "END", "END"],
        ["PRICE_BLEND", "NURSE_BLEND", "HOLD", "END", "END", "END"],
        [
            "TRAP_SOLO",
            "STRUT_SOLO",
            "MOUTH_BLEND",
            "NURSE_BLEND",
            "END",
            "END",
        ],
        ["FLEECE_BLEND", "NURSE_BLEND", "HOLD", "END", "END", "END"],
        [
            "TRAP_BLEND",
            "STRUT_SOLO",
            "GOOSE_BLEND",
            "END",
            "END",
            "END",
        ],
        ["KIT_SOLO", "DRESS_BLEND", "HOLD", "END", "END", "END"],
        [
            "FLEECE_BLEND",
            "DRESS_BLEND",
            "FACE_HELD",
            "HOLD",
            "END",
            "END",
        ],
        ["DRESS_HELD", "HOLD", "HOLD", "END", "END", "END"],
        [
            "GOAT_BLEND",
            "FLEECE_BLEND",
            "THOUGHT_SOLO",
            "HOLD",
            "END",
            "END",
        ],
        ["TRAP_BLEND", "NURSE_BLEND", "HOLD", "END", "END", "END"],
        ["FACE_BLEND", "HOLD", "HOLD", "END", "END", "END"],
        ["KIT_SOLO", "STRUT_SOLO", "HOLD", "END", "END", "END"],
        ["NURSE_BLEND", "STRUT_BLEND", "END", "END", "END", "END"],
        ["NURSE_BLEND", "DRESS_BLEND", "END", "END", "END", "END"],
        ["TRAP_BLEND", "NURSE_BLEND", "END", "END", "END", "END"],
        ["GOOSE_BLEND", "STRUT_BLEND", "END", "END", "END", "END"],
        ["GOOSE_BLEND", "HOLD", "END", "END", "END", "END"],
        [
            "PRICE_BLEND",
            "NURSE_BLEND",
            "DRESS_BLEND",
            "HOLD",
            "END",
            "END",
        ],
        [
            "STRUT_BLEND",
            "FLEECE_BLEND",
            "NURSE_BLEND",
            "END",
            "END",
            "END",
        ],
    ];

    fn decode(token: &str) -> BardSlot {
        const VOWELS: [&str; 17] = [
            "TRAP", "FACE", "CLOTH", "DRESS", "FLEECE", "KIT", "PRICE", "LOT", "GOAT", "THOUGHT",
            "CHOICE", "MOUTH", "FOOT", "GOOSE", "STRUT", "CURE", "NURSE",
        ];
        const VARIANTS: [&str; 3] = ["BLEND", "HELD", "SOLO"];
        match token {
            "HOLD" => BardSlot::Hold,
            "END" => BardSlot::End,
            _ => {
                let (vowel, variant) = token.split_once('_').expect("phoneme token");
                let vowel = VOWELS.iter().position(|v| *v == vowel).expect("vowel");
                let variant = VARIANTS
                    .iter()
                    .position(|v| *v == variant)
                    .expect("variant");
                BardSlot::Phoneme(u8::try_from(3 * vowel + variant).expect("id fits"))
            }
        }
    }

    #[test]
    fn every_slot_matches_the_independent_expectation() {
        let templates = EventsBardTemplates::new();
        assert_eq!(templates.len(), 29);
        assert!(!templates.is_empty());
        for (word, expected) in EXPECTED.iter().enumerate() {
            let row = templates.lookup(u8::try_from(word).unwrap()).unwrap();
            assert_eq!(row.len(), 6);
            for (position, token) in expected.iter().enumerate() {
                assert_eq!(
                    row[position],
                    BardSoundTemplate {
                        slot: decode(token),
                        length_adjustment: 0,
                        unused: 0,
                        volume: 0,
                    },
                    "word {word} slot {position} expected {token}"
                );
            }
        }
    }

    #[test]
    fn hold_end_and_phoneme_ids_are_distinct() {
        let templates = EventsBardTemplates::new();
        let appeal = templates.lookup(0).unwrap();
        assert_eq!(appeal[0].slot, BardSlot::Phoneme(44));
        assert_eq!(appeal[2].slot, BardSlot::Hold);
        assert_eq!(appeal[3].slot, BardSlot::End);
        assert_ne!(appeal[2].slot, appeal[3].slot);
        assert_eq!(BardSlot::Phoneme(50).song_id(), 50);
        assert_eq!(BardSlot::Hold.song_id(), 51);
        assert_eq!(BardSlot::End.song_id(), 255);
        assert_ne!(BardSlot::Phoneme(50), BardSlot::Hold);
    }

    #[test]
    fn words_beyond_the_group_are_refused() {
        let templates = EventsBardTemplates::default();
        for index in 29..=u8::MAX {
            assert_eq!(
                templates.lookup(index),
                Err(BardEventsError::UnknownEventsWord(index))
            );
            assert_eq!(
                EventsWordIndex::new(index),
                Err(BardEventsError::UnknownEventsWord(index))
            );
        }
        let last = EventsWordIndex::new(28).unwrap();
        assert_eq!(last.index(), 28);
        assert_eq!(templates.get(last), templates.lookup(28).unwrap());
        assert_eq!(
            BardEventsError::UnknownEventsWord(29).to_string(),
            "unknown events bard word index `29`"
        );
    }
}
