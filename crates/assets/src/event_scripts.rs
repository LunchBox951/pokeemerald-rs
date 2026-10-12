//! Addressed event-script pack schema (S-4, issue #2096, parent #1671).
//!
//! `pokeemerald` event scripts reference each other by absolute cartridge
//! pointers (`ScriptJump`, `ScriptCall` and `ScriptReadWord`, `src/script.c`),
//! and `MapHeader.mapScripts` points at packed tables of such pointers
//! (`include/global.fieldmap.h`, `asm/macros/map.inc`). This payload keeps those
//! source addresses as [`CartridgeAddress`]es. [`RuntimeOffset`] is a distinct
//! type: the position of a byte in the concatenation of the payload's
//! [`AddressSpan`]s, reachable only through [`AddressedEventScripts::runtime_offset`].
//!
//! One singleton [`EntryKind::Raw`](pack_format::EntryKind::Raw) pack entry,
//! [`ENTRY_ID`], carries the payload; its shape is validated here rather than
//! in `pack-format`, which leaves `Raw` entries opaque.
//!
//! Wire format (all integers little-endian; strings are non-empty UTF-8 with a
//! `u16` byte length; no padding):
//!
//! 1. `magic[8] = "PKMESCR\0"`, `version: u16`, then `u32` counts of spans,
//!    symbols, roots and map tables.
//! 2. Spans: `start: u32, length: u32, bytes`, strictly ascending and disjoint.
//! 3. Symbols: `name, address: u32, kind: u8`, strictly sorted by name.
//! 4. Roots: `id, symbol`, strictly sorted by id.
//! 5. Map tables: `id, symbol, byte_len: u32`, strictly sorted by id. The
//!    entries are read from the span bytes at the symbol's address.
//! 6. Standard-script table: `symbol, count: u16`.
//!
//! Map-table layouts follow `map_script` / `map_script_2` (`asm/macros/map.inc`):
//! a tagged table is `tag: u8, pointer: u32` records ended by a zero tag; a
//! conditional table (`MAP_SCRIPT_ON_FRAME_TABLE`,
//! `MAP_SCRIPT_ON_WARP_INTO_MAP_TABLE`) is `var: u16, compare: u16, pointer: u32`
//! records ended by a zero `var`. Script bytes stay opaque: operands are not
//! scanned for further references.

mod codec;
mod errors;
mod validate;

use codec::{put_string, Cursor};
pub use errors::EventScriptError;
use validate::{bytes_in, find_symbol, span_len, validate};

/// Pack entry id of the singleton addressed-script payload.
pub const ENTRY_ID: &str = "script/events";
/// Schema version written and accepted by this module.
pub const VERSION: u16 = 1;
/// Number of `gStdScripts` slots (`data/event_scripts.s`).
pub const STANDARD_SCRIPT_COUNT: u16 = 11;

const MAGIC: [u8; 8] = *b"PKMESCR\0";
const TAGGED_RECORD_BYTES: u32 = 5;
const CONDITIONAL_RECORD_BYTES: u32 = 8;
const TAGGED_TERMINATOR_BYTES: u32 = 1;
const CONDITIONAL_TERMINATOR_BYTES: u32 = 2;
const POINTER_ALIGNMENT: u32 = 4;

/// An absolute address in the compiled cartridge, as a source pointer holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CartridgeAddress(u32);

impl CartridgeAddress {
    /// Wraps a source address.
    #[must_use]
    pub const fn new(address: u32) -> Self {
        Self(address)
    }

    /// Returns the source address.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A byte position within the payload's concatenated spans: the slice-relative
/// offset form, never a compiled pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeOffset(u32);

impl RuntimeOffset {
    /// Returns the offset.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A contiguous run of cartridge bytes starting at a source address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressSpan {
    /// Source address of the first byte.
    pub start: CartridgeAddress,
    /// The bytes at `start..start + bytes.len()`.
    pub bytes: Vec<u8>,
}

/// What a [`SymbolRecord`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// Script bytecode.
    Script,
    /// Opaque data (text, tables not modelled here).
    Data,
    /// A tag-to-script `MapHeader.mapScripts` table.
    TaggedMapTable,
    /// A variable, compare, script table.
    ConditionalMapTable,
    /// The `gStdScripts` pointer table.
    StandardTable,
}

impl SymbolKind {
    fn from_wire(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Script,
            1 => Self::Data,
            2 => Self::TaggedMapTable,
            3 => Self::ConditionalMapTable,
            4 => Self::StandardTable,
            _ => return None,
        })
    }

    fn wire(self) -> u8 {
        match self {
            Self::Script => 0,
            Self::Data => 1,
            Self::TaggedMapTable => 2,
            Self::ConditionalMapTable => 3,
            Self::StandardTable => 4,
        }
    }
}

/// A named cartridge address. Several names may share one address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRecord {
    /// Assembler symbol name.
    pub name: String,
    /// Where the symbol lives.
    pub address: CartridgeAddress,
    /// What lives there.
    pub kind: SymbolKind,
}

/// A script entry point, named by a stable id and defined by a script symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootRecord {
    /// Unique root id.
    pub id: String,
    /// Name of the [`SymbolKind::Script`] symbol it starts at.
    pub symbol: String,
}

/// Declares one map table: its extent is the symbol's bytes through the
/// terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapTableDescriptor {
    /// Unique table id.
    pub id: String,
    /// Name of the table symbol (tagged or conditional kind).
    pub symbol: String,
    /// Bytes from the symbol address through the terminator.
    pub byte_len: u32,
}

/// The inputs [`AddressedEventScripts::new`] validates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventScriptParts {
    /// Cartridge byte runs, strictly ascending and disjoint.
    pub spans: Vec<AddressSpan>,
    /// Symbols, strictly sorted by name.
    pub symbols: Vec<SymbolRecord>,
    /// Roots, strictly sorted by id.
    pub roots: Vec<RootRecord>,
    /// Map tables, strictly sorted by id.
    pub map_tables: Vec<MapTableDescriptor>,
    /// Name of the [`SymbolKind::StandardTable`] symbol.
    pub standard_table: String,
}

/// A `MapHeader.mapScripts` tag (`include/constants/map_scripts.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapScriptTag {
    /// `MAP_SCRIPT_ON_LOAD`.
    OnLoad,
    /// `MAP_SCRIPT_ON_FRAME_TABLE`; targets a conditional table.
    OnFrameTable,
    /// `MAP_SCRIPT_ON_TRANSITION`.
    OnTransition,
    /// `MAP_SCRIPT_ON_WARP_INTO_MAP_TABLE`; targets a conditional table.
    OnWarpIntoMapTable,
    /// `MAP_SCRIPT_ON_RESUME`.
    OnResume,
    /// `MAP_SCRIPT_ON_DIVE_WARP`.
    OnDiveWarp,
    /// `MAP_SCRIPT_ON_RETURN_TO_FIELD`.
    OnReturnToField,
}

impl MapScriptTag {
    fn from_wire(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Self::OnLoad,
            2 => Self::OnFrameTable,
            3 => Self::OnTransition,
            4 => Self::OnWarpIntoMapTable,
            5 => Self::OnResume,
            6 => Self::OnDiveWarp,
            7 => Self::OnReturnToField,
            _ => return None,
        })
    }

    /// Returns the upstream tag value.
    #[must_use]
    pub fn value(self) -> u8 {
        match self {
            Self::OnLoad => 1,
            Self::OnFrameTable => 2,
            Self::OnTransition => 3,
            Self::OnWarpIntoMapTable => 4,
            Self::OnResume => 5,
            Self::OnDiveWarp => 6,
            Self::OnReturnToField => 7,
        }
    }

    /// Whether the record's pointer names a conditional table, not a script.
    #[must_use]
    pub fn targets_conditional_table(self) -> bool {
        matches!(self, Self::OnFrameTable | Self::OnWarpIntoMapTable)
    }
}

/// One `map_script tag, pointer` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaggedEntry {
    /// Which hook this record installs.
    pub tag: MapScriptTag,
    /// The script, or conditional table for [`MapScriptTag::targets_conditional_table`].
    pub target: CartridgeAddress,
}

/// One `map_script_2 var, compare, pointer` record. Both operands go through
/// `VarGet` in `MapHeaderCheckScriptTable`, so each is a variable id or an
/// immediate value (zero is a legal `compare`); they are preserved, not evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConditionalEntry {
    /// First operand; never zero (zero ends the table).
    pub var: u16,
    /// Second operand; any value, including zero.
    pub compare: u16,
    /// The script run when both operands evaluate equal.
    pub target: CartridgeAddress,
}

/// The decoded entries of one map table, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapTable {
    /// Tag-to-script records.
    Tagged(Vec<TaggedEntry>),
    /// Variable, compare, script records.
    Conditional(Vec<ConditionalEntry>),
}

/// A validated map table with its identity and source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapTableRecord {
    /// Unique table id.
    pub id: String,
    /// Name of the table symbol.
    pub symbol: String,
    /// Source address of the first record.
    pub address: CartridgeAddress,
    /// Bytes through the terminator.
    pub byte_len: u32,
    /// Decoded records.
    pub table: MapTable,
}

/// A validated addressed event-script closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressedEventScripts {
    parts: EventScriptParts,
    tables: Vec<MapTableRecord>,
    standard: Vec<CartridgeAddress>,
}

impl AddressedEventScripts {
    /// Validates `parts` as a closed script store.
    ///
    /// # Errors
    ///
    /// The first [`EventScriptError`] in phase order; see that type.
    pub fn new(parts: EventScriptParts) -> Result<Self, EventScriptError> {
        validate(parts)
    }

    /// Returns the validated inputs, e.g. to rebuild a modified closure.
    #[must_use]
    pub fn parts(&self) -> &EventScriptParts {
        &self.parts
    }

    /// Looks up a symbol by name.
    ///
    /// # Errors
    ///
    /// [`EventScriptError::MissingSymbol`] when absent.
    pub fn symbol(&self, name: &str) -> Result<&SymbolRecord, EventScriptError> {
        find_symbol(&self.parts.symbols, name)
    }

    /// Returns the source address a root starts at.
    ///
    /// # Errors
    ///
    /// [`EventScriptError::UnknownRoot`] when absent.
    pub fn root(&self, id: &str) -> Result<CartridgeAddress, EventScriptError> {
        let roots = &self.parts.roots;
        let index = roots
            .binary_search_by(|r| r.id.as_str().cmp(id))
            .map_err(|_| EventScriptError::UnknownRoot(id.to_owned()))?;
        Ok(find_symbol(&self.parts.symbols, &roots[index].symbol)?.address)
    }

    /// Looks up a map table by id.
    ///
    /// # Errors
    ///
    /// [`EventScriptError::UnknownMapTable`] when absent.
    pub fn map_table(&self, id: &str) -> Result<&MapTableRecord, EventScriptError> {
        self.tables
            .binary_search_by(|t| t.id.as_str().cmp(id))
            .map(|i| &self.tables[i])
            .map_err(|_| EventScriptError::UnknownMapTable(id.to_owned()))
    }

    /// Returns the source address `gStdScripts[id]` holds.
    ///
    /// # Errors
    ///
    /// [`EventScriptError::UnknownStandardScript`] past the table.
    pub fn standard_script(&self, id: u8) -> Result<CartridgeAddress, EventScriptError> {
        self.standard
            .get(usize::from(id))
            .copied()
            .ok_or(EventScriptError::UnknownStandardScript(id))
    }

    /// Returns the cartridge spans.
    #[must_use]
    pub fn spans(&self) -> &[AddressSpan] {
        &self.parts.spans
    }

    /// Returns the symbols, sorted by name.
    #[must_use]
    pub fn symbols(&self) -> &[SymbolRecord] {
        &self.parts.symbols
    }

    /// Returns the roots, sorted by id.
    #[must_use]
    pub fn roots(&self) -> &[RootRecord] {
        &self.parts.roots
    }

    /// Returns the map tables, sorted by id.
    #[must_use]
    pub fn map_tables(&self) -> &[MapTableRecord] {
        &self.tables
    }

    /// Returns the `len` bytes at `address`, which must lie within one span.
    ///
    /// # Errors
    ///
    /// [`EventScriptError::OutOfStoreAddress`] otherwise.
    pub fn bytes_at(&self, address: CartridgeAddress, len: u32) -> Result<&[u8], EventScriptError> {
        bytes_in(&self.parts.spans, address, len)
    }

    /// Maps a cartridge address to its position in the concatenated spans.
    ///
    /// # Errors
    ///
    /// [`EventScriptError::OutOfStoreAddress`] when no span holds it.
    pub fn runtime_offset(
        &self,
        address: CartridgeAddress,
    ) -> Result<RuntimeOffset, EventScriptError> {
        let mut before = 0u32;
        for span in &self.parts.spans {
            let len = span_len(span);
            if address.0 >= span.start.0 && address.0 - span.start.0 < len {
                return Ok(RuntimeOffset(before + (address.0 - span.start.0)));
            }
            before += len;
        }
        Err(EventScriptError::OutOfStoreAddress(address))
    }

    /// Encodes the payload for the [`ENTRY_ID`] raw pack entry.
    ///
    /// # Panics
    ///
    /// Panics if a section length exceeds its wire width; [`Self::new`] rejects those states.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let p = &self.parts;
        let mut out = MAGIC.to_vec();
        out.extend(VERSION.to_le_bytes());
        for count in [
            p.spans.len(),
            p.symbols.len(),
            p.roots.len(),
            p.map_tables.len(),
        ] {
            let count = u32::try_from(count).expect("section count fits u32");
            out.extend(count.to_le_bytes());
        }
        for span in &p.spans {
            out.extend(span.start.0.to_le_bytes());
            out.extend(span_len(span).to_le_bytes());
            out.extend(&span.bytes);
        }
        for s in &p.symbols {
            put_string(&mut out, &s.name);
            out.extend(s.address.0.to_le_bytes());
            out.push(s.kind.wire());
        }
        for r in &p.roots {
            put_string(&mut out, &r.id);
            put_string(&mut out, &r.symbol);
        }
        for t in &self.tables {
            put_string(&mut out, &t.id);
            put_string(&mut out, &t.symbol);
            out.extend(t.byte_len.to_le_bytes());
        }
        put_string(&mut out, &p.standard_table);
        out.extend(STANDARD_SCRIPT_COUNT.to_le_bytes());
        out
    }

    /// Decodes and validates one complete payload.
    ///
    /// # Errors
    ///
    /// Framing failures ([`EventScriptError::Truncated`], `BadMagic`,
    /// `UnsupportedVersion`, `InvalidString`, `UnknownSymbolKind`,
    /// `TrailingBytes`), then the first validation failure of [`Self::new`].
    pub fn decode(bytes: &[u8]) -> Result<Self, EventScriptError> {
        let mut r = Cursor { bytes, pos: 0 };
        if r.take(8, "header")? != MAGIC {
            return Err(EventScriptError::BadMagic);
        }
        let version = r.u16("header")?;
        if version != VERSION {
            return Err(EventScriptError::UnsupportedVersion(version));
        }
        let span_count = r.u32("header")?;
        let symbol_count = r.u32("header")?;
        let root_count = r.u32("header")?;
        let table_count = r.u32("header")?;
        let mut parts = EventScriptParts {
            spans: Vec::new(),
            symbols: Vec::new(),
            roots: Vec::new(),
            map_tables: Vec::new(),
            standard_table: String::new(),
        };
        for _ in 0..span_count {
            let start = CartridgeAddress(r.u32("spans")?);
            let len = r.u32("spans")?;
            let data = r.take(len as usize, "spans")?.to_vec();
            parts.spans.push(AddressSpan { start, bytes: data });
        }
        for _ in 0..symbol_count {
            let name = r.string("symbols")?;
            let address = CartridgeAddress(r.u32("symbols")?);
            let kind = r.u8("symbols")?;
            let kind =
                SymbolKind::from_wire(kind).ok_or(EventScriptError::UnknownSymbolKind(kind))?;
            parts.symbols.push(SymbolRecord {
                name,
                address,
                kind,
            });
        }
        for _ in 0..root_count {
            let id = r.string("roots")?;
            let symbol = r.string("roots")?;
            parts.roots.push(RootRecord { id, symbol });
        }
        for _ in 0..table_count {
            let id = r.string("map tables")?;
            let symbol = r.string("map tables")?;
            let byte_len = r.u32("map tables")?;
            parts.map_tables.push(MapTableDescriptor {
                id,
                symbol,
                byte_len,
            });
        }
        parts.standard_table = r.string("standard table")?;
        let count = r.u16("standard table")?;
        if r.pos != bytes.len() {
            return Err(EventScriptError::TrailingBytes);
        }
        if count != STANDARD_SCRIPT_COUNT {
            return Err(EventScriptError::BadStandardCount(count));
        }
        validate(parts)
    }
}
