//! Shared fixtures for the synthetic ROM-profile regression tests, which need
//! no real ROM and no upstream checkout. The `#[ignore]`d drift test in
//! `locator` is the exception: it needs both, and an extracted pack.

use std::path::PathBuf;

use pack_format::{image_entry, PackWriter};

use super::error::GenRomProfileError;
use super::pack_source::PackSource;
use super::plan::{ReportLine, SymbolExpectation};
use super::search::{Lz77Search, PointerIndex, RawSearch};
use super::Context;

/// A scratch directory named for the process and `name`; callers keep `name`
/// unique per test and remove the directory themselves.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-gen-rom-profile-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// Requires a translation-unit-local root to pass a map that lists linker
/// globals only, so nothing is listed at `table` itself. The nearby global
/// `gGlobalTable` keeps the map non-empty without naming the root.
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
    result.expect("a local root must not require linker-global evidence");
}

/// Cross-checks a whole domain against a global-only GNU ld map: roots marked
/// [`SymbolExpectation::Local`] or [`SymbolExpectation::Interior`], and ids
/// named in `unlisted`, get no symbol.
///
/// Returns the result so a caller can assert either that a correct ROM/map
/// pair passes or that a missing global symbol is refused.
pub(super) fn cross_check_ld_map(
    name: &str,
    lines: &mut [ReportLine],
    unlisted: &[&str],
) -> Result<super::MapCheck, GenRomProfileError> {
    use std::fmt::Write as _;

    let mut map = String::from(".rodata         0x0000000008000000     0x100000\n");
    for (index, line) in lines.iter().enumerate() {
        if matches!(
            line.symbol,
            SymbolExpectation::Local(_) | SymbolExpectation::Interior
        ) || unlisted.contains(&line.id.as_str())
        {
            continue;
        }
        let symbol = match &line.symbol {
            SymbolExpectation::Exact(name) => name.clone(),
            SymbolExpectation::Contains(fragments) => fragments.concat(),
            _ => format!("gRoot{index}"),
        };
        writeln!(
            map,
            "{}0x{:016x}                {symbol}",
            " ".repeat(15),
            line.addr
        )
        .expect("write to a String");
    }
    let dir = scratch(name);
    let map_path = dir.join("pokeemerald.map");
    std::fs::write(&map_path, map).expect("write map");
    let result = super::cross_check(Some(&map_path), lines);
    let _ = std::fs::remove_file(&map_path);
    let _ = std::fs::remove_dir(&dir);
    result
}

/// Non-repeating 4bpp tile data, so only the planted layout reproduces it.
pub(super) fn distinctive_tiles(tile_count: usize) -> Vec<u8> {
    (0..tile_count * 32)
        .map(|index| {
            let value = u32::try_from(index).expect("small") * 37 + 11;
            u8::try_from(value % 251).expect("modulo 251 fits in u8")
        })
        .collect()
}

/// Synthetic bag ids, independent of the upstream inventory: one root of
/// every bag family.
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

/// Runs `body` against a locator context over `rom` and a pack of `entries`,
/// with synthetic bag ids and an upstream path that holds no checkout.
/// The scratch files are removed after `body` returns, not if it panics.
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

/// Encodes `data` (under 16 MiB) as GBA LZ77 using literals only, no
/// back-references.
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

/// Wraps `payload` in image metadata, to prove a raw-payload id is refused
/// when the pack stores it as an image.
pub(super) fn image_twin(id: &str, payload: &[u8]) -> pack_format::PackEntry {
    let width = u32::try_from(payload.len()).expect("payload fits in u32");
    image_entry(id.to_owned(), width, 1, 8, payload.to_vec()).expect("valid image entry")
}

mod audio;
mod audio_bytecode;
mod locator;
mod publication;
