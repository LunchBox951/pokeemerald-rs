//! Locator tests over synthetic ROMs, plus the real-ROM drift check.
//!
//! Every test here but the last builds its own ROM-shaped image with
//! `rom_import::fixture::RomFixture` and its own pack, so the suite needs
//! no copyrighted ROM and no real upstream checkout -- the audio tests
//! build their own synthetic stand-in for the `sound/` files that domain
//! reads. The last test needs both a real ROM and a real checkout and is
//! `#[ignore]`d: it regenerates the profile and asserts the committed file
//! is what a fresh run would write.

use std::path::PathBuf;

use pack_format::{image_entry, PackWriter};

use super::pack_source::PackSource;
use super::plan::ReportLine;
use super::search::{Lz77Search, PointerIndex, RawSearch};
use super::Context;

/// A scratch directory unique to one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-gen-rom-profile-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// Run `lines` through [`super::cross_check`] against a map shaped like
/// GNU ld `-Map` output, which lists linker-global symbols only: the global
/// `gGlobalTable` sits 0x10 bytes past `table`, and nothing is listed at
/// `table` itself because the table is a translation-unit-local `static`.
pub(super) fn assert_survives_ld_map(name: &str, table: u32, lines: &mut [ReportLine]) {
    let dir = scratch(name);
    let map_path = dir.join("pokeemerald.map");
    let map = format!(
        ".rodata         0x{table:016x}       0x20\n \
         *(.rodata .rodata.* .gnu.linkonce.r.*)\n \
         .rodata        0x{table:016x}       0x20 t.o\n \
         {pad}0x{global:016x}                gGlobalTable\n",
        pad = " ".repeat(15),
        global = table + 0x10,
    );
    std::fs::write(&map_path, map).expect("write map");
    let result = super::cross_check(Some(&map_path), lines);
    let _ = std::fs::remove_file(&map_path);
    let _ = std::fs::remove_dir(&dir);
    result.expect("a correctly located static table must survive a real GNU ld map");
}

/// Tile bytes that are distinctive enough to anchor a search.
pub(super) fn distinctive_tiles(tile_count: usize) -> Vec<u8> {
    (0..tile_count * 32)
        .map(|index| {
            let value = u32::try_from(index).expect("small") * 37 + 11;
            u8::try_from(value % 251).expect("modulo 251 fits in u8")
        })
        .collect()
}

/// Run a body against a fixture ROM and a pack built from `entries`.
/// The bag expectation synthetic contexts carry: one root of every family.
pub(super) fn synthetic_bag_ids() -> Vec<String> {
    [
        "bag/image/menu",
        "bag/item-icon/image/potion",
        "bag/palette/bag",
        "bag/item-icon/palette/potion",
        "bag/raw/menu",
    ]
    .map(str::to_owned)
    .to_vec()
}

pub(super) fn with_context<T>(
    name: &str,
    rom: &[u8],
    entries: Vec<pack_format::PackEntry>,
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    let dir = scratch(name);
    let pack_path = dir.join("test.pack");
    let mut writer = PackWriter::new();
    for entry in entries {
        writer.push(entry);
    }
    std::fs::write(&pack_path, writer.finish().expect("pack")).expect("write pack");
    let pack = PackSource::load(&pack_path).expect("load pack");
    let ctx = Context {
        rom,
        pack: &pack,
        raw: RawSearch::new(rom),
        lz77: Lz77Search::new(rom),
        pointers: PointerIndex::build(rom),
        upstream: dir.join("no-checkout"),
        bag_ids: synthetic_bag_ids(),
    };
    let out = body(&ctx);
    let _ = std::fs::remove_file(&pack_path);
    let _ = std::fs::remove_dir(&dir);
    out
}

/// Compress `data` as a literals-only LZ77 stream, which is what the
/// decompressor reads back.
pub(super) fn lz77_literals(data: &[u8]) -> Vec<u8> {
    let len = u32::try_from(data.len()).expect("small");
    let size = len.to_le_bytes();
    let mut out = vec![0x10, size[0], size[1], size[2]];
    for chunk in data.chunks(8) {
        out.push(0);
        out.extend_from_slice(chunk);
    }
    out
}

/// The same bytes as an 8-bit image entry, the kind the importers never emit.
pub(super) fn image_twin(id: &str, payload: &[u8]) -> pack_format::PackEntry {
    let width = u32::try_from(payload.len()).expect("payload fits in u32");
    image_entry(id.to_owned(), width, 1, 8, payload.to_vec()).expect("valid image entry")
}

mod audio;
mod locator;
mod publication;
