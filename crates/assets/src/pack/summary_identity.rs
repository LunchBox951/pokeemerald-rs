//! Typed summary-screen identity and met-location records (upstream
//! `src/data/text/nature_names.h`, `src/data/region_map/region_map_sections.json`,
//! `src/strings.c`, `src/pokemon.c`).
//!
//! One pack entry holds the four tables the summary screen's memo reads, so
//! their lengths and cross-references validate atomically:
//!
//! - `gNatureNamePointers`: [`NATURE_COUNT`] token streams in nature order.
//! - `gRegionMapEntries`' `name` field: [`REGION_MAP_SECTION_COUNT`] streams
//!   indexed by `MAPSEC_*`, in the order the section list generates the enum.
//!   `MAPSEC_AQUA_HIDEOUT_OLD` keeps its `{AQUA} HIDEOUT` placeholder and
//!   `MAPSEC_DYNAMIC` its empty name; the substitutions
//!   `GetMapNameGeneric` and `GetMapNameHandleAquaHideout` make are applied by
//!   [`SummaryIdentityTable::map_name_generic`] and
//!   [`SummaryIdentityTable::met_location_name`].
//! - `gText_Ferry`, `gText_SecretBase` and `gText_Hideout`: the three strings
//!   those substitutions return.
//! - `sSpeciesToNationalPokedexNum`: [`SPECIES_TO_NATIONAL_COUNT`] National Dex
//!   identities indexed by *species minus one*, `SPECIES_NONE` having no slot.
//!
//! The memo templates (`gText_XNature*`) and the secret-base name `GetMapName`
//! reads from the save are composition and stay with their consumers.
//!
//! The pack id is private so callers cannot request other assets through this
//! type. A hand-built or corrupt pack bypasses extraction, so
//! [`AssetPack::summary_identity`] validates the whole payload on each call.
//!
//! # Canonical encoding
//!
//! Little-endian. Header: magic `SIDN`, `schema_version: u16` (1), then three
//! `u16` record counts (natures, region-map sections, species conversions).
//! Then, in ascending ID order, each nature and each section as `id: u16,
//! byte_len: u32, bytes`; the three special strings (Ferry, Secret Base,
//! Hideout) as `byte_len: u32, bytes`; and each conversion as `species: u16,
//! national: u16`.

use std::fmt;

use super::text_tokens::validate_string;
use super::{AssetPack, PackError};
use crate::pokedex::NationalDexNo;
use crate::species::{SpeciesId, SPECIES_COUNT};

const TABLE_PACK_ID: &str = "summary-identity/table";
const MAGIC: [u8; 4] = *b"SIDN";
const SCHEMA_VERSION: u16 = 1;
const HEADER_BYTES: usize = 12;

/// `NUM_NATURES` (`include/constants/pokemon.h`).
pub const NATURE_COUNT: usize = 25;
/// `MAPSEC_NONE`: the number of region-map sections, and the first value that
/// names none (`MAPSEC_COUNT` is one more).
pub const REGION_MAP_SECTION_COUNT: usize = 213;
/// `MAPSEC_AQUA_HIDEOUT_OLD`.
pub const MAPSEC_AQUA_HIDEOUT_OLD: u16 = 66;
/// `MAPSEC_SECRET_BASE`.
pub const MAPSEC_SECRET_BASE: u16 = 86;
/// `MAPSEC_DYNAMIC`.
pub const MAPSEC_DYNAMIC: u16 = 87;
/// `sSpeciesToNationalPokedexNum[NUM_SPECIES - 1]`: every species but `SPECIES_NONE`.
pub const SPECIES_TO_NATIONAL_COUNT: usize = SPECIES_COUNT - 1;

/// A nature (`NATURE_*`), `0` through [`NATURE_COUNT`] - 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NatureId(u8);

impl NatureId {
    /// The nature, or `None` at or past [`NATURE_COUNT`].
    #[must_use]
    pub fn new(id: u8) -> Option<Self> {
        (usize::from(id) < NATURE_COUNT).then_some(Self(id))
    }

    /// The numeric `NATURE_*` value.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// The three strings `GetMapNameGeneric` and `GetMapNameHandleAquaHideout`
/// substitute for a section's own name, token streams with `EOS` included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecialLocationNames<'a> {
    /// `gText_Ferry`, for `MAPSEC_DYNAMIC`.
    pub ferry: &'a [u8],
    /// `gText_SecretBase`, for `MAPSEC_SECRET_BASE`.
    pub secret_base: &'a [u8],
    /// `gText_Hideout`, for `MAPSEC_AQUA_HIDEOUT_OLD` in a Pokémon's memo.
    pub hideout: &'a [u8],
}

/// The table a record or count belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryIdentitySection {
    /// `gNatureNamePointers`.
    Nature,
    /// `gRegionMapEntries` names.
    RegionSection,
    /// `gText_Ferry`, `gText_SecretBase`, `gText_Hideout`, in that order.
    Special,
    /// `sSpeciesToNationalPokedexNum`.
    SpeciesConversion,
}

impl SummaryIdentitySection {
    const fn count(self) -> usize {
        match self {
            Self::Nature => NATURE_COUNT,
            Self::RegionSection => REGION_MAP_SECTION_COUNT,
            Self::Special => 3,
            Self::SpeciesConversion => SPECIES_TO_NATIONAL_COUNT,
        }
    }

    /// The ID of the table's first record: species are numbered from one.
    const fn first_id(self) -> u16 {
        match self {
            Self::SpeciesConversion => 1,
            _ => 0,
        }
    }
}

impl fmt::Display for SummaryIdentitySection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Nature => "nature name",
            Self::RegionSection => "region-map section name",
            Self::Special => "special location name",
            Self::SpeciesConversion => "species conversion",
        })
    }
}

/// A structural failure decoding or building a [`SummaryIdentityTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryIdentityError {
    /// The payload does not begin with the `SIDN` magic.
    BadMagic,
    /// The schema version is not 1.
    UnsupportedVersion(u16),
    /// A record count differs from upstream's table length.
    HeaderMismatch {
        /// The table.
        section: SummaryIdentitySection,
        /// Stored record count.
        found: u16,
    },
    /// The payload ends inside a header, record header, or record body.
    Truncated,
    /// Bytes follow the last record.
    TrailingBytes,
    /// A record's ID is outside its table.
    IdOutOfRange {
        /// The table.
        section: SummaryIdentitySection,
        /// Offending ID.
        id: u16,
    },
    /// Two records of one table carry the same ID.
    DuplicateId {
        /// The table.
        section: SummaryIdentitySection,
        /// Offending ID.
        id: u16,
    },
    /// A table slot has no record.
    MissingId {
        /// The table.
        section: SummaryIdentitySection,
        /// First absent ID.
        id: u16,
    },
    /// A name's token stream is not one well-formed `EOS`-terminated string.
    MalformedString {
        /// The table.
        section: SummaryIdentitySection,
        /// Record ID.
        id: u16,
        /// What is wrong.
        detail: &'static str,
    },
    /// A conversion names a National Dex identity upstream has no constant for.
    InvalidNationalDexNo {
        /// The species.
        species: u16,
        /// Stored identity.
        national: u16,
    },
}

impl fmt::Display for SummaryIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "missing `SIDN` magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported schema version {v}"),
            Self::HeaderMismatch { section, found } => write!(
                f,
                "{section} count is {found}, not upstream's {}",
                section.count()
            ),
            Self::Truncated => write!(f, "payload is truncated"),
            Self::TrailingBytes => write!(f, "bytes follow the last record"),
            Self::IdOutOfRange { section, id } => write!(f, "{section} id {id} is out of range"),
            Self::DuplicateId { section, id } => write!(f, "{section} id {id} is duplicated"),
            Self::MissingId { section, id } => write!(f, "{section} id {id} is missing"),
            Self::MalformedString {
                section,
                id,
                detail,
            } => write!(f, "{section} id {id}: {detail}"),
            Self::InvalidNationalDexNo { species, national } => {
                write!(
                    f,
                    "species {species} converts to invalid dex number {national}"
                )
            }
        }
    }
}

impl std::error::Error for SummaryIdentityError {}

/// Validated summary-screen identity and location records borrowing the pack.
///
/// Every table is complete at its upstream length, every name is one
/// well-formed `EOS`-terminated string, and every conversion names a National
/// Dex identity upstream defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryIdentityTable<'a> {
    natures: Vec<&'a [u8]>,
    sections: Vec<&'a [u8]>,
    specials: SpecialLocationNames<'a>,
    conversions: Vec<NationalDexNo>,
}

/// Fill a table of `section.count()` slots from `(id, item)` pairs in any order.
fn fill<T: Copy>(
    section: SummaryIdentitySection,
    items: impl IntoIterator<Item = (u16, T)>,
) -> Result<Vec<T>, SummaryIdentityError> {
    let mut slots: Vec<Option<T>> = vec![None; section.count()];
    for (id, item) in items {
        let slot = id
            .checked_sub(section.first_id())
            .and_then(|slot| slots.get_mut(usize::from(slot)))
            .ok_or(SummaryIdentityError::IdOutOfRange { section, id })?;
        if slot.is_some() {
            return Err(SummaryIdentityError::DuplicateId { section, id });
        }
        *slot = Some(item);
    }
    slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            slot.ok_or_else(|| SummaryIdentityError::MissingId {
                section,
                id: wire_u16(index) + section.first_id(),
            })
        })
        .collect()
}

fn checked_string(
    section: SummaryIdentitySection,
    id: u16,
    stream: &[u8],
) -> Result<(), SummaryIdentityError> {
    validate_string(stream).map_err(|detail| SummaryIdentityError::MalformedString {
        section,
        id,
        detail,
    })
}

fn checked_names<'a>(
    section: SummaryIdentitySection,
    names: impl IntoIterator<Item = (u16, &'a [u8])>,
) -> Result<Vec<&'a [u8]>, SummaryIdentityError> {
    let names = fill(section, names)?;
    for (index, stream) in names.iter().enumerate() {
        checked_string(section, wire_u16(index), stream)?;
    }
    Ok(names)
}

/// A table index as the wire's `u16`; every table is far shorter than `u16::MAX`.
fn wire_u16(index: usize) -> u16 {
    u16::try_from(index).unwrap_or(u16::MAX)
}

impl<'a> SummaryIdentityTable<'a> {
    /// Build a table from records in any order.
    ///
    /// Conversion IDs are species numbers, from one; the value is the National
    /// Dex identity.
    ///
    /// # Errors
    ///
    /// Any [`SummaryIdentityError`] naming an out-of-range, duplicate, missing,
    /// or malformed record.
    pub fn from_entries(
        natures: impl IntoIterator<Item = (u16, &'a [u8])>,
        sections: impl IntoIterator<Item = (u16, &'a [u8])>,
        specials: SpecialLocationNames<'a>,
        conversions: impl IntoIterator<Item = (u16, u16)>,
    ) -> Result<Self, SummaryIdentityError> {
        let natures = checked_names(SummaryIdentitySection::Nature, natures)?;
        let sections = checked_names(SummaryIdentitySection::RegionSection, sections)?;
        for (id, stream) in [specials.ferry, specials.secret_base, specials.hideout]
            .into_iter()
            .enumerate()
        {
            checked_string(SummaryIdentitySection::Special, wire_u16(id), stream)?;
        }
        let conversions = fill(SummaryIdentitySection::SpeciesConversion, conversions)?
            .into_iter()
            .enumerate()
            .map(|(slot, national)| {
                NationalDexNo::new(national).map_err(|_| {
                    SummaryIdentityError::InvalidNationalDexNo {
                        species: wire_u16(slot) + 1,
                        national,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            natures,
            sections,
            specials,
            conversions,
        })
    }

    /// Strictly decode the canonical encoding.
    ///
    /// # Errors
    ///
    /// Any [`SummaryIdentityError`]; nothing is repaired.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, SummaryIdentityError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(SummaryIdentityError::BadMagic);
        }
        let version = reader.u16()?;
        if version != SCHEMA_VERSION {
            return Err(SummaryIdentityError::UnsupportedVersion(version));
        }
        for section in [
            SummaryIdentitySection::Nature,
            SummaryIdentitySection::RegionSection,
            SummaryIdentitySection::SpeciesConversion,
        ] {
            let count = reader.u16()?;
            if usize::from(count) != section.count() {
                return Err(SummaryIdentityError::HeaderMismatch {
                    section,
                    found: count,
                });
            }
        }
        let natures = reader.named_records(NATURE_COUNT)?;
        let sections = reader.named_records(REGION_MAP_SECTION_COUNT)?;
        let specials = SpecialLocationNames {
            ferry: reader.stream()?,
            secret_base: reader.stream()?,
            hideout: reader.stream()?,
        };
        let mut conversions = Vec::with_capacity(SPECIES_TO_NATIONAL_COUNT);
        for _ in 0..SPECIES_TO_NATIONAL_COUNT {
            conversions.push((reader.u16()?, reader.u16()?));
        }
        if reader.pos != bytes.len() {
            return Err(SummaryIdentityError::TrailingBytes);
        }
        Self::from_entries(natures, sections, specials, conversions)
    }

    /// The canonical encoding, independent of construction order.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
        for count in [
            NATURE_COUNT,
            REGION_MAP_SECTION_COUNT,
            SPECIES_TO_NATIONAL_COUNT,
        ] {
            out.extend_from_slice(&wire_u16(count).to_le_bytes());
        }
        for names in [&self.natures, &self.sections] {
            for (id, stream) in names.iter().enumerate() {
                out.extend_from_slice(&wire_u16(id).to_le_bytes());
                push_stream(&mut out, stream);
            }
        }
        for stream in [
            self.specials.ferry,
            self.specials.secret_base,
            self.specials.hideout,
        ] {
            push_stream(&mut out, stream);
        }
        for (slot, national) in self.conversions.iter().enumerate() {
            out.extend_from_slice(&(wire_u16(slot) + 1).to_le_bytes());
            out.extend_from_slice(&national.get().to_le_bytes());
        }
        out
    }

    /// `gNatureNamePointers[nature]`, `EOS` included.
    #[must_use]
    pub fn nature_name(&self, nature: NatureId) -> &'a [u8] {
        self.natures[usize::from(nature.index())]
    }

    /// `gRegionMapEntries[section].name`, `EOS` included and possibly bare, or
    /// `None` at or past `MAPSEC_NONE`.
    ///
    /// This is the raw field: `MAPSEC_AQUA_HIDEOUT_OLD` holds `{AQUA} HIDEOUT`
    /// and `MAPSEC_DYNAMIC` holds nothing.
    #[must_use]
    pub fn region_section_name(&self, section: u16) -> Option<&'a [u8]> {
        self.sections.get(usize::from(section)).copied()
    }

    /// The name `GetMapNameGeneric` produces for `section`, or `None` at or past
    /// `MAPSEC_NONE`, where `GetMapName` fills with spaces instead.
    ///
    /// `MAPSEC_DYNAMIC` reads as the ferry and `MAPSEC_SECRET_BASE` as the
    /// generic secret base; the save-dependent name `GetMapName` would build is
    /// not part of this table.
    #[must_use]
    pub fn map_name_generic(&self, section: u16) -> Option<&'a [u8]> {
        match section {
            MAPSEC_DYNAMIC => Some(self.specials.ferry),
            MAPSEC_SECRET_BASE => Some(self.specials.secret_base),
            _ => self.region_section_name(section),
        }
    }

    /// The name `GetMapNameHandleAquaHideout` produces for a Pokémon's met
    /// location: [`Self::map_name_generic`], except `MAPSEC_AQUA_HIDEOUT_OLD`
    /// reads as the plain hideout.
    #[must_use]
    pub fn met_location_name(&self, section: u16) -> Option<&'a [u8]> {
        if section == MAPSEC_AQUA_HIDEOUT_OLD {
            return Some(self.specials.hideout);
        }
        self.map_name_generic(section)
    }

    /// `SpeciesToNationalPokedexNum`: `NATIONAL_DEX_NONE` for `SPECIES_NONE`,
    /// the table's entry for species `1` through [`SPECIES_TO_NATIONAL_COUNT`],
    /// and `None` for a species past the table, where upstream reads out of
    /// bounds.
    #[must_use]
    pub fn species_to_national(&self, species: SpeciesId) -> Option<NationalDexNo> {
        match usize::from(species.index()).checked_sub(1) {
            None => Some(NationalDexNo::NONE),
            Some(slot) => self.conversions.get(slot).copied(),
        }
    }

    /// Every nature name in nature order.
    pub fn nature_names(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.natures.iter().copied()
    }

    /// Every region-map section name in `MAPSEC_*` order.
    pub fn region_section_names(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.sections.iter().copied()
    }

    /// The three substitution strings.
    #[must_use]
    pub fn special_location_names(&self) -> SpecialLocationNames<'a> {
        self.specials
    }

    /// Every conversion in table order (species minus one), as species and
    /// National Dex identity.
    pub fn species_conversions(&self) -> impl Iterator<Item = (SpeciesId, NationalDexNo)> + '_ {
        self.conversions
            .iter()
            .enumerate()
            .map(|(slot, national)| (SpeciesId(wire_u16(slot) + 1), *national))
    }
}

fn push_stream(out: &mut Vec<u8>, stream: &[u8]) {
    out.extend_from_slice(
        &u32::try_from(stream.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(stream);
}

impl AssetPack {
    /// The validated summary-screen identity and location records.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// non-raw entry; [`PackError::SummaryIdentityDecode`] when the payload is
    /// truncated, malformed, sparse, duplicated, out of range, or holds an
    /// unterminated string or an undefined National Dex identity.
    pub fn summary_identity(&self) -> Result<SummaryIdentityTable<'_>, PackError> {
        let bytes = self.raw(TABLE_PACK_ID)?;
        SummaryIdentityTable::decode(bytes).map_err(|source| PackError::SummaryIdentityDecode {
            id: TABLE_PACK_ID.to_owned(),
            source,
        })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], SummaryIdentityError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(SummaryIdentityError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(SummaryIdentityError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, SummaryIdentityError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn stream(&mut self) -> Result<&'a [u8], SummaryIdentityError> {
        let b = self.take(4)?;
        let len = usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .map_err(|_| SummaryIdentityError::Truncated)?;
        self.take(len)
    }

    /// `count` records of `id: u16` then a stream.
    fn named_records(
        &mut self,
        count: usize,
    ) -> Result<Vec<(u16, &'a [u8])>, SummaryIdentityError> {
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.u16()?;
            records.push((id, self.stream()?));
        }
        Ok(records)
    }
}
