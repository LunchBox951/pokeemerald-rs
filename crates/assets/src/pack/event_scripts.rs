//! Resolver facade over the addressed event-script payload
//! ([`crate::event_scripts`]) stored at [`ENTRY_ID`].
//!
//! Map metadata names scripts and tables with [`ScriptRef`] and
//! [`MapScriptTableRef`]; these methods turn a reference into validated
//! addressed data. Script identities are [`CartridgeAddress`]es, the
//! addresses the source pointers hold; none is a slice-relative cursor
//! offset (see [`AddressedEventScripts::runtime_offset`] for that, a
//! separate step).
//!
//! Every call decodes and validates the payload again. Callers resolving
//! many references take [`AssetPack::event_scripts`] once and query it.

use crate::event_scripts::{
    AddressedEventScripts, CartridgeAddress, EventScriptError, MapTableRecord, SymbolRecord,
    ENTRY_ID,
};
use crate::script_refs::{MapScriptTableRef, ScriptRef};

use super::{AssetPack, PackError};

fn resolve_failed(source: EventScriptError) -> PackError {
    PackError::EventScriptResolve {
        id: ENTRY_ID.to_owned(),
        source,
    }
}

impl AssetPack {
    /// Decodes and validates the `script/events` entry.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] if the pack has no addressed event
    /// scripts; [`PackError::WrongKind`] if the entry isn't
    /// [`EntryKind::Raw`](super::EntryKind::Raw);
    /// [`PackError::EventScriptDecode`] if the payload fails the schema.
    pub fn event_scripts(&self) -> Result<AddressedEventScripts, PackError> {
        let bytes = self.raw(ENTRY_ID)?;
        AddressedEventScripts::decode(bytes).map_err(|source| PackError::EventScriptDecode {
            id: ENTRY_ID.to_owned(),
            source,
        })
    }

    /// Resolves a map-event script root to the cartridge address it starts at.
    ///
    /// # Errors
    ///
    /// Those of [`Self::event_scripts`]; [`PackError::EventScriptResolve`]
    /// wrapping [`EventScriptError::UnknownRoot`] if no such root exists.
    pub fn resolve_script(&self, script: ScriptRef) -> Result<CartridgeAddress, PackError> {
        self.event_scripts()?
            .root(script.id())
            .map_err(resolve_failed)
    }

    /// Resolves an event field that may hold the NULL script: `None` stays
    /// `None` without reading the pack, and `Some` resolves as
    /// [`Self::resolve_script`].
    ///
    /// # Errors
    ///
    /// As [`Self::resolve_script`], for `Some` only.
    pub fn resolve_optional_script(
        &self,
        script: Option<ScriptRef>,
    ) -> Result<Option<CartridgeAddress>, PackError> {
        script.map(|s| self.resolve_script(s)).transpose()
    }

    /// Resolves a header's map-script table: `None` (no table) stays `None`
    /// without reading the pack.
    ///
    /// # Errors
    ///
    /// Those of [`Self::event_scripts`]; [`PackError::EventScriptResolve`]
    /// wrapping [`EventScriptError::UnknownMapTable`] if the table is absent.
    pub fn resolve_map_script_table(
        &self,
        table: Option<MapScriptTableRef>,
    ) -> Result<Option<MapTableRecord>, PackError> {
        let Some(table) = table else {
            return Ok(None);
        };
        let scripts = self.event_scripts()?;
        scripts
            .map_table(table.id())
            .map(|record| Some(record.clone()))
            .map_err(resolve_failed)
    }

    /// Resolves an assembler symbol by name.
    ///
    /// # Errors
    ///
    /// Those of [`Self::event_scripts`]; [`PackError::EventScriptResolve`]
    /// wrapping [`EventScriptError::MissingSymbol`] if absent.
    pub fn resolve_symbol(&self, name: &str) -> Result<SymbolRecord, PackError> {
        self.event_scripts()?
            .symbol(name)
            .cloned()
            .map_err(resolve_failed)
    }

    /// Resolves `gStdScripts[id]` to the cartridge address it holds.
    ///
    /// # Errors
    ///
    /// Those of [`Self::event_scripts`]; [`PackError::EventScriptResolve`]
    /// wrapping [`EventScriptError::UnknownStandardScript`] past the table.
    pub fn resolve_standard_script(&self, id: u8) -> Result<CartridgeAddress, PackError> {
        self.event_scripts()?
            .standard_script(id)
            .map_err(resolve_failed)
    }
}
