//! Byte-fixture tests for the addressed event-script payload (S-4).
//!
//! The fixture below is authored byte by byte, independent of
//! `AddressedEventScripts::encode`, so the decoder is checked against the
//! documented wire layout and not against its own writer.

use assets::event_scripts::{
    AddressedEventScripts, CartridgeAddress, ConditionalEntry, EventScriptError as E, MapScriptTag,
    MapTable, TaggedEntry, ENTRY_ID,
};
use pack_format::{parse_directory, raw_entry, PackWriter};

const S0: u32 = 0x0810_0001; // deliberately odd: scripts are byte-aligned
const S1: u32 = 0x0810_0002;
const MAIN: u32 = 0x0810_0003;
const MAIN_COND: u32 = 0x0810_0027;
const EMPTY: u32 = 0x0810_0039;
const EMPTY_COND: u32 = 0x0810_003A;
const STD: u32 = 0x0820_0000;

struct Fx {
    magic: Vec<u8>,
    version: u16,
    spans: Vec<(u32, Vec<u8>)>,
    symbols: Vec<(&'static str, u32, u8)>,
    roots: Vec<(&'static str, &'static str)>,
    tables: Vec<(&'static str, &'static str, u32)>,
    std: (&'static str, u16),
    trailing: Vec<u8>,
}

fn le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap()
}

fn fx() -> Fx {
    let mut a = vec![0x02, 0x03]; // S0, S1: opaque script bytes
    for tag in 1u8..=7 {
        a.push(tag);
        a.extend(match tag {
            2 | 4 => le(MAIN_COND),
            3 => le(S1),
            _ => le(S0),
        });
    }
    a.push(0); // MAIN terminator
    assert_eq!(a.len(), 38);
    a.extend([0x01, 0x40, 0x02, 0x40]); // var 0x4001, compare 0x4002
    a.extend(le(S1));
    a.extend([0x03, 0x40, 0x00, 0x00]); // compare 0 is an immediate, not a terminator
    a.extend(le(S0));
    a.extend([0, 0]); // MAIN_COND terminator
    a.push(0); // EMPTY
    a.extend([0, 0]); // EMPTY_COND
    assert_eq!(a.len(), 59);
    let mut b = Vec::new();
    for i in 0..11 {
        b.extend(le(if i % 2 == 0 { S0 } else { S1 }));
    }
    Fx {
        magic: b"PKMESCR\0".to_vec(),
        version: 1,
        spans: vec![(S0, a), (STD, b)],
        symbols: vec![
            ("gStdScripts", STD, 4),
            ("mapEmpty", EMPTY, 2),
            ("mapEmptyCond", EMPTY_COND, 3),
            ("mapMain", MAIN, 2),
            ("mapMainCond", MAIN_COND, 3),
            ("s0", S0, 0),
            ("s0_alias", S0, 0),
            ("s1", S1, 0),
        ],
        roots: vec![("root/a", "s0"), ("root/b", "s0_alias"), ("root/c", "s1")],
        tables: vec![
            ("Empty", "mapEmpty", 1),
            ("EmptyCond", "mapEmptyCond", 2),
            ("Main", "mapMain", 36),
            ("MainCond", "mapMainCond", 18),
        ],
        std: ("gStdScripts", 11),
        trailing: Vec::new(),
    }
}

fn string(out: &mut Vec<u8>, s: &str) {
    out.extend(u16::try_from(s.len()).unwrap().to_le_bytes());
    out.extend(s.as_bytes());
}

impl Fx {
    fn bytes(&self) -> Vec<u8> {
        let mut o = self.magic.clone();
        o.extend(self.version.to_le_bytes());
        for n in [
            self.spans.len(),
            self.symbols.len(),
            self.roots.len(),
            self.tables.len(),
        ] {
            o.extend(le(count(n)));
        }
        for (start, data) in &self.spans {
            o.extend(le(*start));
            o.extend(le(count(data.len())));
            o.extend(data);
        }
        for (name, addr, kind) in &self.symbols {
            string(&mut o, name);
            o.extend(le(*addr));
            o.push(*kind);
        }
        for (id, sym) in &self.roots {
            string(&mut o, id);
            string(&mut o, sym);
        }
        for (id, sym, len) in &self.tables {
            string(&mut o, id);
            string(&mut o, sym);
            o.extend(le(*len));
        }
        string(&mut o, self.std.0);
        o.extend(self.std.1.to_le_bytes());
        o.extend(&self.trailing);
        o
    }

    fn err(&self) -> E {
        AddressedEventScripts::decode(&self.bytes()).expect_err("fixture must be rejected")
    }
}

fn addr(a: u32) -> CartridgeAddress {
    CartridgeAddress::new(a)
}

#[test]
fn valid_closure_decodes_and_keeps_source_addresses() {
    let bytes = fx().bytes();
    let s = AddressedEventScripts::decode(&bytes).unwrap();
    assert_eq!(s.encode(), bytes);

    assert_eq!(s.root("root/a").unwrap(), addr(S0));
    assert_eq!(s.root("root/b").unwrap(), addr(S0));
    assert_eq!(s.root("root/c").unwrap(), addr(S1));
    assert_eq!(s.root("nope"), Err(E::UnknownRoot("nope".into())));
    assert_eq!(s.symbol("s0_alias").unwrap().address, addr(S0));
    assert_eq!(s.symbol("zzz"), Err(E::MissingSymbol("zzz".into())));
    assert_eq!(s.standard_script(0).unwrap(), addr(S0));
    assert_eq!(s.standard_script(1).unwrap(), addr(S1));
    assert_eq!(s.standard_script(11), Err(E::UnknownStandardScript(11)));

    let MapTable::Tagged(main) = &s.map_table("Main").unwrap().table else {
        panic!("Main is tagged");
    };
    assert_eq!(main.len(), 7);
    assert_eq!(
        main[1],
        TaggedEntry {
            tag: MapScriptTag::OnFrameTable,
            target: addr(MAIN_COND)
        }
    );
    assert_eq!(main[3].tag, MapScriptTag::OnWarpIntoMapTable);
    assert_eq!(main[2].target, addr(S1));
    let MapTable::Conditional(cond) = &s.map_table("MainCond").unwrap().table else {
        panic!("MainCond is conditional");
    };
    assert_eq!(
        cond[0],
        ConditionalEntry {
            var: 0x4001,
            compare: 0x4002,
            target: addr(S1)
        }
    );
    assert_eq!(cond[1].compare, 0);
    assert_eq!(cond[1].target, addr(S0));
    assert_eq!(
        s.map_table("Empty").unwrap().table,
        MapTable::Tagged(vec![])
    );
    assert_eq!(
        s.map_table("EmptyCond").unwrap().table,
        MapTable::Conditional(vec![])
    );
    assert_eq!(s.map_table("zz"), Err(E::UnknownMapTable("zz".into())));

    // Runtime offsets index the concatenated spans, never the source address.
    assert_eq!(s.runtime_offset(addr(S0)).unwrap().get(), 0);
    assert_eq!(s.runtime_offset(addr(MAIN)).unwrap().get(), 2);
    assert_eq!(s.runtime_offset(addr(STD)).unwrap().get(), 59);
    assert_eq!(
        s.runtime_offset(addr(STD + 44)),
        Err(E::OutOfStoreAddress(addr(STD + 44)))
    );
    assert_eq!(s.bytes_at(addr(S0), 2).unwrap(), &[0x02, 0x03]);
    assert!(s.bytes_at(addr(EMPTY_COND), 4).is_err());
}

#[test]
fn round_trips_through_a_pack_entry() {
    let payload = fx().bytes();
    let mut w = PackWriter::new();
    w.push(raw_entry(ENTRY_ID.to_owned(), payload.clone()));
    let pack = w.finish().unwrap();
    let dir = parse_directory(&pack).unwrap();
    let entry = dir.iter().find(|e| e.id == ENTRY_ID).unwrap();
    let start = entry.offset;
    let got = &pack[start..start + entry.length];
    assert_eq!(
        AddressedEventScripts::decode(got).unwrap().encode(),
        payload
    );
}

#[test]
fn constructor_rejects_what_the_decoder_rejects() {
    let s = AddressedEventScripts::decode(&fx().bytes()).unwrap();
    let mut parts = s.parts().clone();
    parts.roots.pop();
    assert_eq!(
        AddressedEventScripts::new(parts),
        Err(E::MissingRoot(addr(S1)))
    );
}

#[test]
fn truncated_payloads_are_typed() {
    let bytes = fx().bytes();
    for cut in [0, 7, 9, 25, 40, bytes.len() - 1] {
        assert!(
            matches!(
                AddressedEventScripts::decode(&bytes[..cut]),
                Err(E::Truncated { .. })
            ),
            "cut {cut}"
        );
    }
}

#[test]
fn framing_errors() {
    let mut f = fx();
    f.magic[0] = b'X';
    assert_eq!(f.err(), E::BadMagic);
    let mut f = fx();
    f.version = 2;
    assert_eq!(f.err(), E::UnsupportedVersion(2));
    let mut f = fx();
    f.trailing = vec![0];
    assert_eq!(f.err(), E::TrailingBytes);
    let mut f = fx();
    f.symbols[7].2 = 9;
    assert_eq!(f.err(), E::UnknownSymbolKind(9));
    let mut f = fx();
    f.roots[0].0 = "";
    assert_eq!(f.err(), E::InvalidString { section: "roots" });
}

#[test]
fn span_errors() {
    let mut f = fx();
    f.spans[1].0 = S0 + 5;
    assert_eq!(f.err(), E::OverlappingSpans);
    let mut f = fx();
    f.spans[1].1.clear();
    assert_eq!(f.err(), E::EmptySpan);
    let mut f = fx();
    f.spans[1].0 = u32::MAX - 3;
    assert_eq!(f.err(), E::OverlappingSpans);
}

#[test]
fn symbol_errors() {
    let mut f = fx();
    f.symbols[7].0 = "s0_alias";
    assert_eq!(f.err(), E::DuplicateSymbol("s0_alias".into()));
    let mut f = fx();
    f.symbols.swap(5, 7);
    assert_eq!(f.err(), E::NotCanonical { section: "symbols" });
    let mut f = fx();
    f.roots[2].1 = "absent";
    assert_eq!(f.err(), E::MissingSymbol("absent".into()));
    let mut f = fx();
    f.symbols[5].1 = 0x0900_0000;
    assert_eq!(f.err(), E::OutOfStoreAddress(addr(0x0900_0000)));
    let mut f = fx();
    f.symbols[0].1 = STD + 2;
    assert_eq!(f.err(), E::UnalignedAddress(addr(STD + 2)));
    let mut f = fx();
    f.symbols[5].1 = STD + 44; // one past the end of the span
    assert_eq!(f.err(), E::OutOfStoreAddress(addr(STD + 44)));
}

#[test]
fn root_errors() {
    let mut f = fx();
    f.roots[1].0 = "root/a";
    assert_eq!(f.err(), E::DuplicateRoot("root/a".into()));
    let mut f = fx();
    f.roots.swap(0, 2);
    assert_eq!(f.err(), E::NotCanonical { section: "roots" });
    let mut f = fx();
    f.roots[0].1 = "mapMain";
    assert_eq!(f.err(), E::RootNotScript("root/a".into()));
    let mut f = fx();
    f.roots.pop();
    assert_eq!(f.err(), E::MissingRoot(addr(S1)));
    let mut f = fx();
    f.spans[0].1[2 + 1..2 + 5].copy_from_slice(&le(MAIN)); // tag 1 -> not a script symbol
    assert_eq!(f.err(), E::MissingScriptSymbol(addr(MAIN)));
}

#[test]
fn map_table_structure_errors() {
    let mut f = fx();
    f.spans[0].1[2] = 8;
    assert_eq!(
        f.err(),
        E::BadTag {
            table: "Main".into(),
            tag: 8
        }
    );
    // Missing terminator: the declared extent ends after the last record.
    let mut f = fx();
    f.tables[2].2 = 35;
    assert_eq!(
        f.err(),
        E::BadTerminator {
            table: "Main".into()
        }
    );
    let mut f = fx();
    f.tables[3].2 = 16;
    assert_eq!(
        f.err(),
        E::BadTerminator {
            table: "MainCond".into()
        }
    );
    // Early terminator: a zero tag with declared bytes left over.
    let mut f = fx();
    f.spans[0].1[2 + 5] = 0;
    assert_eq!(
        f.err(),
        E::BadTerminator {
            table: "Main".into()
        }
    );
    let mut f = fx();
    f.spans[0].1[38..40].copy_from_slice(&[0, 0]);
    assert_eq!(
        f.err(),
        E::BadTerminator {
            table: "MainCond".into()
        }
    );
    // Extent ends inside a record.
    let mut f = fx();
    f.tables[2].2 = 3;
    assert_eq!(
        f.err(),
        E::TruncatedRecord {
            table: "Main".into()
        }
    );
    let mut f = fx();
    f.tables[3].2 = 12;
    assert_eq!(
        f.err(),
        E::TruncatedRecord {
            table: "MainCond".into()
        }
    );
    f.tables[3].2 = 9;
    assert_eq!(
        f.err(),
        E::BadTerminator {
            table: "MainCond".into()
        }
    );
    // Table bytes outside the store.
    let mut f = fx();
    f.tables[2].2 = 1000;
    assert_eq!(
        f.err(),
        E::InconsistentTableReference {
            table: "Main".into()
        }
    );
}

#[test]
fn map_table_declaration_errors() {
    let mut f = fx();
    f.tables[1].0 = "Empty";
    assert_eq!(f.err(), E::DuplicateMapTable("Empty".into()));
    let mut f = fx();
    f.tables.swap(0, 3);
    assert_eq!(
        f.err(),
        E::NotCanonical {
            section: "map tables"
        }
    );
    let mut f = fx();
    f.tables[0].1 = "s0";
    assert_eq!(
        f.err(),
        E::InconsistentTableReference {
            table: "Empty".into()
        }
    );
    let mut f = fx();
    f.tables[0].1 = "missing";
    assert_eq!(f.err(), E::MissingSymbol("missing".into()));
}

#[test]
fn inconsistent_table_references() {
    // OnFrameTable must point at a declared conditional table, not a script.
    let mut f = fx();
    f.spans[0].1[2 + 5 + 1..2 + 5 + 5].copy_from_slice(&le(S0));
    assert_eq!(
        f.err(),
        E::InconsistentTableReference {
            table: "Main".into()
        }
    );
    // ... nor at a tagged table.
    let mut f = fx();
    f.spans[0].1[2 + 5 + 1..2 + 5 + 5].copy_from_slice(&le(MAIN));
    assert_eq!(
        f.err(),
        E::InconsistentTableReference {
            table: "Main".into()
        }
    );
}

#[test]
fn standard_script_errors() {
    let mut f = fx();
    f.spans[1].1[0..4].copy_from_slice(&le(MAIN));
    assert_eq!(f.err(), E::InvalidStandardScriptReference { id: 0 });
    let mut f = fx();
    f.spans[1].1[8..12].copy_from_slice(&le(S1 + 1));
    assert_eq!(f.err(), E::InvalidStandardScriptReference { id: 2 });
    let mut f = fx();
    f.spans[1].1.truncate(40);
    assert_eq!(f.err(), E::OutOfStoreAddress(addr(STD)));
    let mut f = fx();
    f.std.0 = "s0";
    assert_eq!(
        f.err(),
        E::InconsistentTableReference { table: "s0".into() }
    );
    let mut f = fx();
    f.std.0 = "nope";
    assert_eq!(f.err(), E::MissingSymbol("nope".into()));
    let mut f = fx();
    f.std.1 = 10;
    assert_eq!(f.err(), E::BadStandardCount(10));
    f.std.0 = "nope";
    assert_eq!(f.err(), E::BadStandardCount(10));
}

#[test]
fn error_precedence_is_phase_ordered() {
    // Span, symbol, root, table and standard-count faults at once: the span
    // fault is reported.
    let mut f = fx();
    f.spans[1].0 = S0;
    f.symbols[7].0 = "s0_alias";
    f.roots[0].1 = "absent";
    f.tables[2].2 = 35;
    f.std.1 = 10;
    assert_eq!(f.err(), E::OverlappingSpans);
    f.spans[1].0 = STD;
    assert_eq!(f.err(), E::DuplicateSymbol("s0_alias".into()));
    f.symbols[7].0 = "s1";
    assert_eq!(f.err(), E::MissingSymbol("absent".into()));
    f.roots[0].1 = "s0";
    assert_eq!(
        f.err(),
        E::BadTerminator {
            table: "Main".into()
        }
    );
    f.tables[2].2 = 36;
    assert_eq!(f.err(), E::BadStandardCount(10));
}
