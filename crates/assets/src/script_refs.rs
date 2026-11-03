//! Typed references from static map metadata into the addressed event-script
//! pack payload ([`crate::event_scripts`]).
//!
//! A reference names a packed root or table by id; it is not an executable
//! address and not a raw script label. Resolve one through the
//! [`AssetPack`](crate::pack::AssetPack) facade, which returns validated
//! [`CartridgeAddress`](crate::event_scripts::CartridgeAddress)es and table
//! records. Generated tables use the upstream script symbol as the id, so a
//! `script` named `PetalburgCity_EventScript_Boy` in `map.json` is the root
//! `PetalburgCity_EventScript_Boy`, and a header's `mapScripts` pointer
//! `PetalburgCity_MapScripts` is the map table `PetalburgCity_MapScripts`.
//!
//! The upstream NULL script (`"0x0"` in `map.json`) is not a reference: fields
//! that may be NULL are `Option<ScriptRef>` and hold `None`.

/// A reference to a script root of the addressed event-script payload
/// ([`AddressedEventScripts::root`](crate::event_scripts::AddressedEventScripts::root)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptRef(&'static str);

impl ScriptRef {
    /// Names the root with id `id`.
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Returns the root id.
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.0
    }
}

/// A reference to a `MapHeader.mapScripts` table of the addressed event-script
/// payload ([`AddressedEventScripts::map_table`](crate::event_scripts::AddressedEventScripts::map_table)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapScriptTableRef(&'static str);

impl MapScriptTableRef {
    /// Names the map table with id `id`.
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Returns the table id.
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.0
    }
}
