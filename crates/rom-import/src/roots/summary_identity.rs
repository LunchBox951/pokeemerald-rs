//! The summary screen's identity and met-location tables.

use crate::reader::GbaPtr;

/// The tables behind the summary screen's nature name and met-location memo.
///
/// `gNatureNamePointers` and `sSpeciesToNationalPokedexNum` are arrays of
/// pointers and of `u16`s; `gRegionMapEntries` is an array of
/// `RegionMapLocation` rows whose name pointer sits at a fixed offset (see
/// `domains::summary_identity`); the three special strings are single streams
/// reached by their own address. A profile records all six or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryIdentityRoots {
    /// `gNatureNamePointers`: 25 pointers indexed by nature.
    pub natures: GbaPtr,
    /// `gRegionMapEntries`: 213 `RegionMapLocation` rows indexed by `MAPSEC_*`.
    pub region_map_entries: GbaPtr,
    /// `gText_Ferry`.
    pub ferry: GbaPtr,
    /// `gText_SecretBase`.
    pub secret_base: GbaPtr,
    /// `gText_Hideout`.
    pub hideout: GbaPtr,
    /// `sSpeciesToNationalPokedexNum`: 411 `u16`s indexed by species minus one.
    pub species_to_national: GbaPtr,
}

impl SummaryIdentityRoots {
    /// No summary-identity roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        natures: GbaPtr::AT_BASE,
        region_map_entries: GbaPtr::AT_BASE,
        ferry: GbaPtr::AT_BASE,
        secret_base: GbaPtr::AT_BASE,
        hideout: GbaPtr::AT_BASE,
        species_to_national: GbaPtr::AT_BASE,
    };
}
