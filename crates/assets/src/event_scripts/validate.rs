//! Validation of a decoded or constructed script closure.

use std::collections::BTreeSet;

use super::errors::EventScriptError;
use super::{
    AddressSpan, AddressedEventScripts, CartridgeAddress, ConditionalEntry, EventScriptParts,
    MapScriptTag, MapTable, MapTableDescriptor, MapTableRecord, SymbolKind, SymbolRecord,
    TaggedEntry, CONDITIONAL_RECORD_BYTES, CONDITIONAL_TERMINATOR_BYTES, POINTER_ALIGNMENT,
    STANDARD_SCRIPT_COUNT, TAGGED_RECORD_BYTES, TAGGED_TERMINATOR_BYTES,
};

pub(super) fn span_len(span: &AddressSpan) -> u32 {
    u32::try_from(span.bytes.len()).expect("validated spans fit u32")
}

pub(super) fn find_symbol<'a>(
    symbols: &'a [SymbolRecord],
    name: &str,
) -> Result<&'a SymbolRecord, EventScriptError> {
    symbols
        .binary_search_by(|s| s.name.as_str().cmp(name))
        .map(|i| &symbols[i])
        .map_err(|_| EventScriptError::MissingSymbol(name.to_owned()))
}

pub(super) fn bytes_in(
    spans: &[AddressSpan],
    address: CartridgeAddress,
    len: u32,
) -> Result<&[u8], EventScriptError> {
    let out_of_store = EventScriptError::OutOfStoreAddress(address);
    let span = spans
        .iter()
        .find(|s| {
            address.0 >= s.start.0
                && u64::from(address.0) < u64::from(s.start.0) + s.bytes.len() as u64
        })
        .ok_or_else(|| out_of_store.clone())?;
    let from = (address.0 - span.start.0) as usize;
    let to = from
        .checked_add(len as usize)
        .filter(|&t| t <= span.bytes.len());
    to.map(|to| &span.bytes[from..to]).ok_or(out_of_store)
}

pub(super) fn check_sorted<T, K: Ord + ?Sized>(
    items: &[T],
    key: impl Fn(&T) -> &K,
    section: &'static str,
    duplicate: impl Fn(&T) -> EventScriptError,
) -> Result<(), EventScriptError> {
    for pair in items.windows(2) {
        match key(&pair[0]).cmp(key(&pair[1])) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => return Err(duplicate(&pair[1])),
            std::cmp::Ordering::Greater => return Err(EventScriptError::NotCanonical { section }),
        }
    }
    Ok(())
}

pub(super) fn check_name(name: &str, section: &'static str) -> Result<(), EventScriptError> {
    if name.is_empty() || name.len() > usize::from(u16::MAX) {
        return Err(EventScriptError::InvalidString { section });
    }
    Ok(())
}

/// Addresses of script symbols and of declared roots, for target checks.
pub(super) struct ScriptTargets {
    scripts: BTreeSet<u32>,
    roots: BTreeSet<u32>,
}

impl ScriptTargets {
    fn check(&self, target: CartridgeAddress) -> Result<(), EventScriptError> {
        if !self.scripts.contains(&target.0) {
            return Err(EventScriptError::MissingScriptSymbol(target));
        }
        if !self.roots.contains(&target.0) {
            return Err(EventScriptError::MissingRoot(target));
        }
        Ok(())
    }
}

pub(super) fn validate(parts: EventScriptParts) -> Result<AddressedEventScripts, EventScriptError> {
    validate_decoded(parts, STANDARD_SCRIPT_COUNT)
}

/// Runs every phase in wire order; `standard_count` is the wire's slot count
/// and is checked as the first step of the standard-table phase.
pub(super) fn validate_decoded(
    parts: EventScriptParts,
    standard_count: u16,
) -> Result<AddressedEventScripts, EventScriptError> {
    validate_spans(&parts.spans)?;
    validate_symbols(&parts)?;
    let targets = validate_roots(&parts)?;
    let tables = parse_tables(&parts)?;
    check_table_references(&tables, &targets)?;
    if standard_count != STANDARD_SCRIPT_COUNT {
        return Err(EventScriptError::BadStandardCount(standard_count));
    }
    let standard = read_standard_table(&parts, &targets)?;
    Ok(AddressedEventScripts {
        parts,
        tables,
        standard,
    })
}

pub(super) fn validate_spans(spans: &[AddressSpan]) -> Result<(), EventScriptError> {
    let mut next_free = 0u64;
    let mut total = 0u64;
    for span in spans {
        if span.bytes.is_empty() {
            return Err(EventScriptError::EmptySpan);
        }
        let len = span.bytes.len() as u64;
        let start = u64::from(span.start.0);
        total += len;
        if start < next_free || start + len > 1 << 32 || total > u64::from(u32::MAX) {
            return Err(EventScriptError::OverlappingSpans);
        }
        next_free = start + len;
    }
    Ok(())
}

pub(super) fn validate_symbols(parts: &EventScriptParts) -> Result<(), EventScriptError> {
    for s in &parts.symbols {
        check_name(&s.name, "symbols")?;
    }
    check_sorted(
        &parts.symbols,
        |s| s.name.as_str(),
        "symbols",
        |s| EventScriptError::DuplicateSymbol(s.name.clone()),
    )?;
    for s in &parts.symbols {
        bytes_in(&parts.spans, s.address, 1)?;
        if s.kind == SymbolKind::StandardTable && s.address.0 % POINTER_ALIGNMENT != 0 {
            return Err(EventScriptError::UnalignedAddress(s.address));
        }
    }
    Ok(())
}

pub(super) fn validate_roots(parts: &EventScriptParts) -> Result<ScriptTargets, EventScriptError> {
    for r in &parts.roots {
        check_name(&r.id, "roots")?;
    }
    check_sorted(
        &parts.roots,
        |r| r.id.as_str(),
        "roots",
        |r| EventScriptError::DuplicateRoot(r.id.clone()),
    )?;
    let mut roots = BTreeSet::new();
    for r in &parts.roots {
        let symbol = find_symbol(&parts.symbols, &r.symbol)?;
        if symbol.kind != SymbolKind::Script {
            return Err(EventScriptError::RootNotScript(r.id.clone()));
        }
        roots.insert(symbol.address.0);
    }
    let scripts = parts
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Script)
        .map(|s| s.address.0)
        .collect();
    Ok(ScriptTargets { scripts, roots })
}

pub(super) fn parse_tables(
    parts: &EventScriptParts,
) -> Result<Vec<MapTableRecord>, EventScriptError> {
    for t in &parts.map_tables {
        check_name(&t.id, "map tables")?;
    }
    check_sorted(
        &parts.map_tables,
        |t| t.id.as_str(),
        "map tables",
        |t| EventScriptError::DuplicateMapTable(t.id.clone()),
    )?;
    let mut tables = Vec::with_capacity(parts.map_tables.len());
    for d in &parts.map_tables {
        let symbol = find_symbol(&parts.symbols, &d.symbol)?;
        let table = match symbol.kind {
            SymbolKind::TaggedMapTable | SymbolKind::ConditionalMapTable => {
                parse_table(&parts.spans, d, symbol)?
            }
            _ => {
                return Err(EventScriptError::InconsistentTableReference {
                    table: d.id.clone(),
                })
            }
        };
        tables.push(MapTableRecord {
            id: d.id.clone(),
            symbol: d.symbol.clone(),
            address: symbol.address,
            byte_len: d.byte_len,
            table,
        });
    }
    Ok(tables)
}

pub(super) fn check_table_references(
    tables: &[MapTableRecord],
    targets: &ScriptTargets,
) -> Result<(), EventScriptError> {
    let conditional: BTreeSet<u32> = tables
        .iter()
        .filter(|t| matches!(t.table, MapTable::Conditional(_)))
        .map(|t| t.address.0)
        .collect();
    for t in tables {
        match &t.table {
            MapTable::Tagged(entries) => {
                for e in entries {
                    if !e.tag.targets_conditional_table() {
                        targets.check(e.target)?;
                    } else if !conditional.contains(&e.target.0) {
                        return Err(EventScriptError::InconsistentTableReference {
                            table: t.id.clone(),
                        });
                    }
                }
            }
            MapTable::Conditional(entries) => {
                for e in entries {
                    targets.check(e.target)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn read_standard_table(
    parts: &EventScriptParts,
    targets: &ScriptTargets,
) -> Result<Vec<CartridgeAddress>, EventScriptError> {
    let symbol = find_symbol(&parts.symbols, &parts.standard_table)?;
    if symbol.kind != SymbolKind::StandardTable {
        return Err(EventScriptError::InconsistentTableReference {
            table: parts.standard_table.clone(),
        });
    }
    let raw = bytes_in(
        &parts.spans,
        symbol.address,
        u32::from(STANDARD_SCRIPT_COUNT) * POINTER_ALIGNMENT,
    )?;
    let mut standard = Vec::new();
    for (index, word) in raw.chunks_exact(4).enumerate() {
        let target = CartridgeAddress(u32::from_le_bytes([word[0], word[1], word[2], word[3]]));
        if targets.check(target).is_err() {
            let id = u8::try_from(index).expect("standard slot count fits u8");
            return Err(EventScriptError::InvalidStandardScriptReference { id });
        }
        standard.push(target);
    }
    Ok(standard)
}

pub(super) fn parse_table(
    spans: &[AddressSpan],
    d: &MapTableDescriptor,
    symbol: &SymbolRecord,
) -> Result<MapTable, EventScriptError> {
    let conditional = symbol.kind == SymbolKind::ConditionalMapTable;
    let (record, terminator) = if conditional {
        (CONDITIONAL_RECORD_BYTES, CONDITIONAL_TERMINATOR_BYTES)
    } else {
        (TAGGED_RECORD_BYTES, TAGGED_TERMINATOR_BYTES)
    };
    let table = d.id.clone();
    let bytes = bytes_in(spans, symbol.address, d.byte_len).map_err(|_| {
        EventScriptError::InconsistentTableReference {
            table: table.clone(),
        }
    })?;
    let mut tagged = Vec::new();
    let mut cond = Vec::new();
    let mut pos = 0usize;
    loop {
        let rest = &bytes[pos..];
        // Terminator is a zero tag byte (tagged) or zero var halfword (conditional).
        let ended = if conditional {
            rest.len() >= 2 && rest[0] == 0 && rest[1] == 0
        } else {
            rest.first() == Some(&0)
        };
        if ended {
            if rest.len() != terminator as usize {
                return Err(EventScriptError::BadTerminator { table });
            }
            break;
        }
        if rest.is_empty() {
            return Err(EventScriptError::BadTerminator { table });
        }
        if rest.len() < record as usize {
            // A conditional table cut inside its would-be terminator is a bad terminator.
            return Err(if conditional && rest.len() == 1 {
                EventScriptError::BadTerminator { table }
            } else {
                EventScriptError::TruncatedRecord { table }
            });
        }
        let ptr = |at: usize| {
            CartridgeAddress(u32::from_le_bytes([
                rest[at],
                rest[at + 1],
                rest[at + 2],
                rest[at + 3],
            ]))
        };
        if conditional {
            cond.push(ConditionalEntry {
                var: u16::from_le_bytes([rest[0], rest[1]]),
                compare: u16::from_le_bytes([rest[2], rest[3]]),
                target: ptr(4),
            });
        } else {
            let tag = MapScriptTag::from_wire(rest[0]).ok_or_else(|| EventScriptError::BadTag {
                table: table.clone(),
                tag: rest[0],
            })?;
            tagged.push(TaggedEntry {
                tag,
                target: ptr(1),
            });
        }
        pos += record as usize;
    }
    Ok(if conditional {
        MapTable::Conditional(cond)
    } else {
        MapTable::Tagged(tagged)
    })
}
