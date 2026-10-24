//! Binary schema for the normalized species-cry table, the species-to-cry lookup, and the
//! `PlayCryInternal` mode settings.
//!
//! Upstream's `gCryTable` and `gCryTable_Reverse` (`sound/cry_tables.inc`) name the same
//! `DirectSound` sample at each of 388 positions and differ only in the tone type's reverse flag,
//! so the pack keeps one table and takes the reverse flag from the resolved mode.
//!
//! The `audio/cry/table` payload is [`CRY_TABLE_LEN`] length-prefixed sample ids in table order,
//! with nothing after them.

use super::cursor::{check_id_len, Reader, Writer};
use super::error::AudioError;
use super::sample::SampleId;
use super::voicegroup::DirectSoundMode;
use crate::species::SpeciesId;

/// Pack id of the one entry holding the [`CryTable`] payload.
pub const CRY_TABLE_PACK_ID: &str = "audio/cry/table";

/// The number of positions in `gCryTable` (`sound/cry_tables.inc`).
pub const CRY_TABLE_LEN: usize = 388;

/// Every cry plays a `DirectSound` sample, so each referenced id lives in this family.
const DIRECT_SOUND_ID_PREFIX: &str = "audio/sample/direct-sound/";

/// `CRY_VOLUME` (`include/constants/sound.h`), the volume of an unmodified cry.
pub const CRY_VOLUME: i8 = 120;
/// `CRY_PRIORITY_NORMAL` (`include/constants/sound.h`).
pub const CRY_PRIORITY_NORMAL: u8 = 10;
/// `CRY_PRIORITY_AMBIENT` (`include/constants/sound.h`).
pub const CRY_PRIORITY_AMBIENT: u8 = 1;

/// A position in the cry table, as `SpeciesToCryId` returns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CryId(u16);

impl CryId {
    /// Returns the zero-based table position.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// `gSpeciesIdToCryId` (`src/data/pokemon/cry_ids.h`): the cry-table position of each species
/// from `SPECIES_TREECKO` through `SPECIES_CHIMECHO`.
const HOENN_CRY_IDS: [u16; 135] = [
    273, 274, 275, 270, 271, 272, 276, 277, 278, 359, 360, 378, 375, 290, 291, 292, 293, 294, 283,
    284, 285, 286, 287, 288, 301, 302, 303, 266, 267, 374, 373, 269, 280, 279, 310, 311, 377, 381,
    312, 313, 251, 329, 330, 306, 253, 362, 318, 319, 368, 320, 321, 333, 334, 289, 260, 324, 325,
    326, 304, 305, 254, 255, 316, 317, 338, 339, 340, 327, 328, 383, 307, 331, 332, 262, 322, 323,
    308, 309, 363, 336, 337, 263, 264, 258, 256, 361, 252, 298, 299, 300, 314, 315, 376, 382, 380,
    379, 341, 342, 343, 335, 282, 281, 259, 261, 367, 364, 365, 366, 356, 357, 358, 344, 345, 346,
    347, 295, 296, 297, 351, 352, 372, 348, 349, 350, 353, 354, 355, 370, 369, 371, 257, 384, 385,
    386, 387,
];

/// Resolves a species to its cry position: `PlayCryInternal` decrements the species
/// (`sound.c:369-495`), then `SpeciesToCryId` maps it (`pokemon.c:5701-5710`).
///
/// Species up to Celebi map to their own position, the placeholder species between Celebi and
/// Treecko share Unown's cry, and Hoenn species use the remap table. Species zero and species
/// past Chimecho have no cry; upstream indexes outside its table for them.
#[must_use]
pub fn cry_id(species: SpeciesId) -> Option<CryId> {
    let zero_based = species.index().checked_sub(1)?;
    let celebi = SpeciesId::CELEBI.index() - 1;
    let treecko = SpeciesId::TREECKO.index() - 1;
    if zero_based <= celebi {
        return Some(CryId(zero_based));
    }
    if zero_based < treecko {
        return Some(CryId(SpeciesId::UNOWN.index() - 1));
    }
    HOENN_CRY_IDS
        .get(usize::from(zero_based - treecko))
        .copied()
        .map(CryId)
}

/// The `mode` argument of `PlayCry_Script`/`PlayCryInternal` (`include/constants/sound.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryMode {
    Normal,
    Doubles,
    Encounter,
    HighPitch,
    EchoStart,
    Faint,
    EchoEnd,
    Roar1,
    Roar2,
    Growl1,
    Growl2,
    Weak,
    WeakDoubles,
}

/// The playback settings `PlayCryInternal` writes before starting a cry's tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrySettings {
    /// [`DirectSoundMode::Reverse`] when the mode selects `gCryTable_Reverse`, otherwise the
    /// cry tone's ordinary resampled playback.
    pub direct_sound_mode: DirectSoundMode,
    /// `SetPokemonCryPanpot`.
    pub pan: i8,
    /// `SetPokemonCryVolume`: the mode's override, or the caller's volume.
    pub volume: i8,
    /// `SetPokemonCryPitch`.
    pub pitch: u16,
    /// `SetPokemonCryLength`.
    pub length: u8,
    /// `SetPokemonCryRelease`.
    pub release: u8,
    /// `SetPokemonCryChorus`.
    pub chorus: u8,
    /// `SetPokemonCryPriority`.
    pub priority: u8,
}

impl CrySettings {
    /// Whether the cry plays the reversed table (`CRY_MODE_ECHO_START`, `CRY_MODE_GROWL_1`).
    #[must_use]
    pub fn is_reverse(&self) -> bool {
        self.direct_sound_mode == DirectSoundMode::Reverse
    }
}

impl CryMode {
    /// The settings `PlayCryInternal` selects for this mode, given the caller's `pan`, `volume`, and
    /// `priority`.
    #[must_use]
    pub fn settings(self, pan: i8, volume: i8, priority: u8) -> CrySettings {
        let mut settings = CrySettings {
            direct_sound_mode: DirectSoundMode::Resampled,
            pan,
            volume,
            pitch: 15360,
            length: 140,
            release: 0,
            chorus: 0,
            priority,
        };
        match self {
            Self::Normal => {}
            Self::Doubles => {
                settings.length = 20;
                settings.release = 225;
            }
            Self::Encounter => {
                settings.release = 225;
                settings.pitch = 15600;
                settings.chorus = 20;
                settings.volume = 90;
            }
            Self::HighPitch => {
                settings.length = 50;
                settings.release = 200;
                settings.pitch = 15800;
                settings.chorus = 20;
                settings.volume = 90;
            }
            Self::EchoStart => {
                settings.length = 25;
                settings.direct_sound_mode = DirectSoundMode::Reverse;
                settings.release = 100;
                settings.pitch = 15600;
                settings.chorus = 192;
                settings.volume = 90;
            }
            Self::Faint => {
                settings.release = 200;
                settings.pitch = 14440;
            }
            Self::EchoEnd => {
                settings.release = 220;
                settings.pitch = 15555;
                settings.chorus = 192;
                settings.volume = 70;
            }
            Self::Roar1 => {
                settings.length = 10;
                settings.release = 100;
                settings.pitch = 14848;
            }
            Self::Roar2 => {
                settings.length = 60;
                settings.release = 225;
                settings.pitch = 15616;
            }
            Self::Growl1 => {
                settings.length = 15;
                settings.direct_sound_mode = DirectSoundMode::Reverse;
                settings.release = 125;
                settings.pitch = 15200;
            }
            Self::Growl2 => {
                settings.length = 100;
                settings.release = 225;
                settings.pitch = 15200;
            }
            Self::WeakDoubles => {
                settings.length = 20;
                settings.release = 225;
                settings.pitch = 15000;
            }
            Self::Weak => settings.pitch = 15000,
        }
        settings
    }
}

/// What `PlayCryInternal` starts for one species: its table sample under the mode's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedCry<'a> {
    pub sample: &'a SampleId,
    pub settings: CrySettings,
}

/// The normalized `gCryTable`: one [`SampleId`] per cry position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryTable {
    samples: Vec<SampleId>,
}

impl CryTable {
    /// Builds a table from [`CRY_TABLE_LEN`] sample ids.
    ///
    /// # Errors
    ///
    /// [`AudioError::CryTableLength`] if `samples` does not hold exactly [`CRY_TABLE_LEN`] ids;
    /// [`AudioError::CrySampleNotDirectSound`] if an id is outside the `DirectSound` sample
    /// family; [`AudioError::IdTooLong`] if an id exceeds the `u16` wire prefix.
    pub fn new(samples: Vec<SampleId>) -> Result<Self, AudioError> {
        if samples.len() != CRY_TABLE_LEN {
            return Err(AudioError::CryTableLength(samples.len()));
        }
        for (position, sample) in samples.iter().enumerate() {
            check_id_len(&sample.0)?;
            if !sample.0.starts_with(DIRECT_SOUND_ID_PREFIX) {
                return Err(AudioError::CrySampleNotDirectSound { position });
            }
        }
        Ok(Self { samples })
    }

    /// The sample ids in table order.
    #[must_use]
    pub fn samples(&self) -> &[SampleId] {
        &self.samples
    }

    /// The sample at a cry position.
    #[must_use]
    pub fn sample(&self, cry: CryId) -> Option<&SampleId> {
        self.samples.get(usize::from(cry.index()))
    }

    /// Resolves `species` and `mode` the way `PlayCryInternal` does, or `None` when the species
    /// has no cry.
    #[must_use]
    pub fn resolve(
        &self,
        species: SpeciesId,
        mode: CryMode,
        pan: i8,
        volume: i8,
        priority: u8,
    ) -> Option<ResolvedCry<'_>> {
        Some(ResolvedCry {
            sample: self.sample(cry_id(species)?)?,
            settings: mode.settings(pan, volume, priority),
        })
    }

    /// Serializes the table into its `audio/cry/table` payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        for sample in &self.samples {
            writer.string(&sample.0);
        }
        writer.into_bytes()
    }

    /// Decodes and validates an `audio/cry/table` payload.
    ///
    /// # Errors
    ///
    /// [`AudioError::Truncated`] or [`AudioError::InvalidString`] for an incomplete or
    /// non-UTF-8 payload; [`AudioError::TrailingBytes`] for bytes after the last id; the
    /// [`Self::new`] errors for an id outside the `DirectSound` sample family.
    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let mut reader = Reader::new(bytes);
        let mut samples = Vec::with_capacity(CRY_TABLE_LEN);
        for _ in 0..CRY_TABLE_LEN {
            samples.push(SampleId(reader.string()?));
        }
        reader.expect_eof()?;
        Self::new(samples)
    }
}

#[cfg(test)]
mod tests;
