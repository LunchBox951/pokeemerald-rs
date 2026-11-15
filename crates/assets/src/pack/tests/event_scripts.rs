//! Consumer fixtures for the addressed event-script resolver: events and
//! headers from the generated tables resolve through a pack built with
//! `PackWriter`.

use pack_format::{image_entry, raw_entry, PackWriter};

use super::super::{AssetPack, PackError};
use super::shared::write_pack;
use crate::event_scripts::{
    AddressSpan, AddressedEventScripts, CartridgeAddress, EventScriptError, EventScriptParts,
    MapScriptTag, MapTable, MapTableDescriptor, RootRecord, SymbolKind, SymbolRecord, TaggedEntry,
    ENTRY_ID, STANDARD_SCRIPT_COUNT,
};
use crate::map_events::{BgEventKind, CoordEventKind, MapEventsTable};
use crate::map_headers::MapHeaderTable;
use crate::script_refs::{MapScriptTableRef, ScriptRef};
use crate::wild_encounters::MapId;

const SCRIPTS: u32 = 0x0810_0000;
const STD: u32 = 0x0820_0000;
const EMPTY_TABLE: &str = "Fx_Empty_MapScripts";
const MAIN_TABLE: &str = "PetalburgCity_MapScripts";

fn addr(offset: u32) -> CartridgeAddress {
    CartridgeAddress::new(SCRIPTS + offset)
}

fn object_ref() -> ScriptRef {
    let events = MapEventsTable::new()
        .resolve(MapId("MAP_PETALBURG_CITY"))
        .unwrap();
    events.object_events[0].script.unwrap()
}

fn trigger_ref() -> ScriptRef {
    let events = MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .unwrap();
    let CoordEventKind::Trigger { script, .. } = events.coord_events[0].kind else {
        panic!("Route 101's first coordinate event is a trigger");
    };
    script
}

fn sign_ref() -> ScriptRef {
    let events = MapEventsTable::new()
        .resolve(MapId("MAP_PETALBURG_CITY"))
        .unwrap();
    events
        .bg_events
        .iter()
        .find_map(|e| match e.kind {
            BgEventKind::Sign { script, .. } => script,
            _ => None,
        })
        .expect("Petalburg City has a sign")
}

/// Three one-byte scripts, a one-record tagged table, an empty table, and a
/// standard-script table, with ids taken from the refs under test.
fn parts() -> EventScriptParts {
    let mut ids = [object_ref().id(), trigger_ref().id(), sign_ref().id()];
    ids.sort_unstable();
    let mut bytes = vec![0x02; 3];
    bytes.push(MapScriptTag::OnLoad.value());
    bytes.extend((SCRIPTS + 2).to_le_bytes());
    bytes.push(0); // table terminator
    bytes.push(0); // empty table
    let std_bytes: Vec<u8> = (0..STANDARD_SCRIPT_COUNT)
        .flat_map(|i| (SCRIPTS + u32::from(i % 3)).to_le_bytes())
        .collect();
    let symbol = |name: &str, address: u32, kind| SymbolRecord {
        name: name.to_owned(),
        address: CartridgeAddress::new(address),
        kind,
    };
    let mut symbols = vec![
        symbol(MAIN_TABLE, SCRIPTS + 3, SymbolKind::TaggedMapTable),
        symbol(EMPTY_TABLE, SCRIPTS + 9, SymbolKind::TaggedMapTable),
        symbol("gStdScripts", STD, SymbolKind::StandardTable),
    ];
    let mut roots = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        symbols.push(symbol(
            id,
            SCRIPTS + u32::try_from(i).unwrap(),
            SymbolKind::Script,
        ));
        roots.push(RootRecord {
            id: (*id).to_owned(),
            symbol: (*id).to_owned(),
        });
    }
    symbols.sort_by(|a, b| a.name.cmp(&b.name));
    let mut map_tables = vec![
        MapTableDescriptor {
            id: MAIN_TABLE.to_owned(),
            symbol: MAIN_TABLE.to_owned(),
            byte_len: 6,
        },
        MapTableDescriptor {
            id: EMPTY_TABLE.to_owned(),
            symbol: EMPTY_TABLE.to_owned(),
            byte_len: 1,
        },
    ];
    map_tables.sort_by(|a, b| a.id.cmp(&b.id));
    EventScriptParts {
        spans: vec![
            AddressSpan {
                start: CartridgeAddress::new(SCRIPTS),
                bytes,
            },
            AddressSpan {
                start: CartridgeAddress::new(STD),
                bytes: std_bytes,
            },
        ],
        symbols,
        roots,
        map_tables,
        standard_table: "gStdScripts".to_owned(),
    }
}

fn pack_from(tag: &str, entries: Vec<pack_format::PackEntry>) -> AssetPack {
    let mut writer = PackWriter::new();
    for entry in entries {
        writer.push(entry);
    }
    let path = write_pack(tag, &writer.finish().unwrap());
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    pack
}

fn pack(tag: &str) -> AssetPack {
    let payload = AddressedEventScripts::new(parts()).unwrap().encode();
    pack_from(tag, vec![raw_entry(ENTRY_ID.to_owned(), payload)])
}

fn resolve_error(
    pack: &AssetPack,
    f: impl FnOnce(&AssetPack) -> Result<(), PackError>,
) -> EventScriptError {
    match f(pack) {
        Err(PackError::EventScriptResolve { id, source }) => {
            assert_eq!(id, ENTRY_ID);
            source
        }
        other => panic!("expected an EventScriptResolve error, got {other:?}"),
    }
}

#[test]
fn object_coordinate_and_sign_references_resolve_to_cartridge_addresses() {
    let pack = pack("script-events-refs");
    let mut ids = [object_ref().id(), trigger_ref().id(), sign_ref().id()];
    ids.sort_unstable();
    let expected = |r: ScriptRef| {
        let slot = ids.iter().position(|id| *id == r.id()).unwrap();
        addr(u32::try_from(slot).unwrap())
    };
    for r in [object_ref(), trigger_ref(), sign_ref()] {
        assert_eq!(pack.resolve_script(r), Ok(expected(r)));
    }
    // Identities are cartridge addresses, not offsets into the span bytes.
    assert!(pack.resolve_script(object_ref()).unwrap().get() >= SCRIPTS);
}

#[test]
fn null_event_scripts_stay_null_without_reading_the_pack() {
    let events = MapEventsTable::new()
        .resolve(MapId("MAP_PETALBURG_CITY"))
        .unwrap();
    assert_eq!(events.object_events[1].script, None);
    // A pack with no script entry cannot resolve anything, yet null is null.
    let empty = pack_from("script-events-null", vec![]);
    assert_eq!(
        empty.resolve_optional_script(events.object_events[1].script),
        Ok(None)
    );
    assert_eq!(empty.resolve_map_script_table(None), Ok(None));
    assert_eq!(
        empty.resolve_optional_script(Some(object_ref())),
        Err(PackError::UnknownAsset(ENTRY_ID.to_owned()))
    );
}

#[test]
fn optional_script_resolves_when_present() {
    let pack = pack("script-events-optional");
    assert_eq!(
        pack.resolve_optional_script(Some(object_ref())),
        Ok(Some(pack.resolve_script(object_ref()).unwrap()))
    );
}

#[test]
fn header_map_script_table_resolves_to_its_records() {
    let pack = pack("script-events-header");
    let header = MapHeaderTable::new()
        .header(MapId("MAP_PETALBURG_CITY"))
        .unwrap();
    assert_eq!(header.map_scripts, Some(MapScriptTableRef::new(MAIN_TABLE)));
    let record = pack
        .resolve_map_script_table(header.map_scripts)
        .unwrap()
        .unwrap();
    assert_eq!(record.id, MAIN_TABLE);
    assert_eq!(record.address, addr(3));
    assert_eq!(
        record.table,
        MapTable::Tagged(vec![TaggedEntry {
            tag: MapScriptTag::OnLoad,
            target: addr(2),
        }])
    );
    let empty = pack
        .resolve_map_script_table(Some(MapScriptTableRef::new(EMPTY_TABLE)))
        .unwrap()
        .unwrap();
    assert_eq!(empty.table, MapTable::Tagged(vec![]));
}

#[test]
fn symbol_and_standard_script_resolve() {
    let pack = pack("script-events-symbol");
    let symbol = pack.resolve_symbol("gStdScripts").unwrap();
    assert_eq!(symbol.address, CartridgeAddress::new(STD));
    assert_eq!(symbol.kind, SymbolKind::StandardTable);
    assert_eq!(pack.resolve_standard_script(0), Ok(addr(0)));
    assert_eq!(pack.resolve_standard_script(1), Ok(addr(1)));
    assert_eq!(pack.resolve_standard_script(10), Ok(addr(1)));
}

#[test]
fn missing_references_are_typed_resolve_errors() {
    let pack = pack("script-events-missing");
    let err = resolve_error(&pack, |p| {
        p.resolve_script(ScriptRef::new("Nope")).map(drop)
    });
    assert_eq!(err, EventScriptError::UnknownRoot("Nope".to_owned()));
    let err = resolve_error(&pack, |p| {
        p.resolve_map_script_table(Some(MapScriptTableRef::new("Nope_MapScripts")))
            .map(drop)
    });
    assert_eq!(
        err,
        EventScriptError::UnknownMapTable("Nope_MapScripts".to_owned())
    );
    let err = resolve_error(&pack, |p| p.resolve_symbol("nope").map(drop));
    assert_eq!(err, EventScriptError::MissingSymbol("nope".to_owned()));
    let err = resolve_error(&pack, |p| p.resolve_standard_script(11).map(drop));
    assert_eq!(err, EventScriptError::UnknownStandardScript(11));
}

#[test]
fn a_reference_in_the_wrong_namespace_does_not_resolve() {
    let pack = pack("script-events-mismatch");
    // A table id is not a script root, and a script root is not a table.
    let err = resolve_error(&pack, |p| {
        p.resolve_script(ScriptRef::new(MAIN_TABLE)).map(drop)
    });
    assert_eq!(err, EventScriptError::UnknownRoot(MAIN_TABLE.to_owned()));
    let err = resolve_error(&pack, |p| {
        p.resolve_map_script_table(Some(MapScriptTableRef::new(object_ref().id())))
            .map(drop)
    });
    assert_eq!(
        err,
        EventScriptError::UnknownMapTable(object_ref().id().to_owned())
    );
}

#[test]
fn a_missing_wrong_kind_or_malformed_entry_is_a_pack_error() {
    let missing = pack_from("script-events-absent", vec![]);
    assert_eq!(
        missing.event_scripts(),
        Err(PackError::UnknownAsset(ENTRY_ID.to_owned()))
    );

    let image = image_entry(ENTRY_ID.to_owned(), 1, 1, 8, vec![0]);
    let wrong = pack_from("script-events-kind", vec![image.unwrap()]);
    assert!(matches!(
        wrong.resolve_symbol("x"),
        Err(PackError::WrongKind {
            expected: "raw blob",
            actual: "image",
            ..
        })
    ));

    let mut bytes = AddressedEventScripts::new(parts()).unwrap().encode();
    bytes.pop();
    let bad = pack_from(
        "script-events-bad",
        vec![raw_entry(ENTRY_ID.to_owned(), bytes)],
    );
    assert!(matches!(
        bad.event_scripts(),
        Err(PackError::EventScriptDecode {
            source: EventScriptError::Truncated { .. },
            ..
        })
    ));
}
