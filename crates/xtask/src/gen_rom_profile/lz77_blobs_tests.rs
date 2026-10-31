//! Tests for the shared LZ77 raw-blob locator over a synthetic ROM and pack.

use pack_format::{raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::{Encoding, ROM_BASE};

use super::{locate_lz77_blobs, unnamed};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{lz77_literals, with_context};

const FIRST: &str = "demo/raw/first";
const SECOND: &str = "demo/raw/second";

fn blob(seed: u8) -> Vec<u8> {
    (0..40u8)
        .map(|n| n.wrapping_mul(5).wrapping_add(seed))
        .collect()
}

fn ids() -> Vec<String> {
    vec![FIRST.to_owned(), SECOND.to_owned()]
}

fn entries() -> Vec<PackEntry> {
    vec![
        raw_entry(FIRST.into(), blob(1)),
        raw_entry(SECOND.into(), blob(2)),
    ]
}

fn rom() -> Vec<u8> {
    RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&blob(1)))
        .write(0x11_0000, &lz77_literals(&blob(2)))
        .finish()
}

#[test]
fn each_id_is_planned_at_its_stream_with_the_decompressed_length() {
    with_context("lz77-blobs-plan", &rom(), entries(), |ctx| {
        let mut report = Vec::new();
        let plans = locate_lz77_blobs(ctx, &ids(), &unnamed, &mut report).expect("located");

        assert_eq!(plans.len(), 2);
        for (plan, (id, offset)) in plans.iter().zip([(FIRST, 0x10_0000), (SECOND, 0x11_0000)]) {
            assert_eq!(plan.id, id);
            assert_eq!(plan.addr, ROM_BASE + offset);
            assert_eq!(plan.encoding, Encoding::Lz77);
            assert_eq!(plan.len, 40);
        }
        assert_eq!(report.len(), plans.len());
        for (line, plan) in report.iter().zip(&plans) {
            assert_eq!(line.id, plan.id);
            assert_eq!(line.addr, plan.addr);
            assert_eq!(line.len, plan.len);
        }
        assert!(report
            .iter()
            .all(|line| line.resolution == Resolution::UniqueSignature
                && line.symbol == SymbolExpectation::Unnamed
                && line.note.is_none()));
    });
}

#[test]
fn the_symbol_callback_decorates_each_report_line_by_id() {
    with_context("lz77-blobs-symbol", &rom(), entries(), |ctx| {
        let mut report = Vec::new();
        let expect = |id: &str| SymbolExpectation::Exact(format!("g{id}"));
        locate_lz77_blobs(ctx, &ids(), &expect, &mut report).expect("located");

        assert_eq!(
            report[0].symbol,
            SymbolExpectation::Exact(format!("g{FIRST}"))
        );
        assert_eq!(
            report[1].symbol,
            SymbolExpectation::Exact(format!("g{SECOND}"))
        );
    });
}

#[test]
fn no_ids_locate_nothing() {
    with_context("lz77-blobs-empty", &rom(), entries(), |ctx| {
        let mut report = Vec::new();
        let plans = locate_lz77_blobs(ctx, &[], &unnamed, &mut report).expect("located");
        assert!(plans.is_empty());
        assert!(report.is_empty());
    });
}

#[test]
fn a_stream_absent_from_the_rom_is_not_found() {
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&blob(1)))
        .finish();
    with_context("lz77-blobs-absent", &rom, entries(), |ctx| {
        let err = locate_lz77_blobs(ctx, &ids(), &unnamed, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if id == SECOND),
            "{err}"
        );
    });
}

#[test]
fn a_stream_present_twice_is_ambiguous() {
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&blob(1)))
        .write(0x11_0000, &lz77_literals(&blob(2)))
        .write(0x12_0000, &lz77_literals(&blob(2)))
        .finish();
    with_context("lz77-blobs-twice", &rom, entries(), |ctx| {
        let err = locate_lz77_blobs(ctx, &ids(), &unnamed, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::Ambiguous { id, addrs }
                if id == SECOND && addrs.len() == 2),
            "{err}"
        );
    });
}

#[test]
fn an_id_missing_from_the_pack_is_refused() {
    let mut entries = entries();
    entries.retain(|entry| entry.id != SECOND);
    with_context("lz77-blobs-missing", &rom(), entries, |ctx| {
        let err = locate_lz77_blobs(ctx, &ids(), &unnamed, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == SECOND),
            "{err}"
        );
    });
}
