//! `cargo xtask gen-rom-profile` (S-4, F-3, Discussion #71 policy C, issue
//! #122): derive `rom-import`'s address table from a real ROM, once, and
//! commit the result.
//!
//! The shipped importer never scans a ROM. It reads
//! `crates/rom-import/src/profiles/bpee_rev0.rs`, a table of offsets,
//! lengths, dimensions, counts, and names. This is the developer-only tool
//! that writes that table, and it is the only place a heuristic ever runs.
//!
//! # Why generate rather than hand-write
//!
//! Several hundred addresses, each of which produces plausible garbage
//! rather than a clean failure when it is wrong. A machine that derives
//! every one of them from evidence, asserts each is unique, and checks that
//! the bytes it found really are the bytes the pack holds, is the only way
//! that table is trustworthy.
//!
//! # What counts as evidence
//!
//! `cargo xtask extract` already turned the upstream checkout into a pack.
//! That pack is the expectation: every root is located by looking for the
//! exact bytes the pack says the asset is, repacked into whatever shape a
//! ROM stores (see [`pack_source`]). A root is only accepted when exactly
//! one place in the ROM holds those bytes. When more than one does, a
//! struct back-reference or an adjacency to an already-unique root breaks
//! the tie, and the report says which and why -- never a first-match-wins
//! guess.
//!
//! Three sheets are byte-identical to another sheet and sit in the ROM
//! twice, and nothing there distinguishes the two symbols. Those get
//! [`plan::Resolution::ArbitraryAmongIdentical`]: the choice really is
//! arbitrary, and the report says so rather than dressing it up as
//! evidence. It is sound only because every candidate holds the same
//! bytes, so the pack entry is identical either way.
//!
//! The ROM itself is admitted only after `rom_import`'s own profile
//! selection accepts its whole-file SHA-1, so the heuristics never run
//! against an image that is merely similar. Passing `--map` adds a second,
//! independent witness: see [`map_file`].
//!
//! # What never leaves this tool
//!
//! ROM bytes. The generated table carries offsets, lengths, dimensions,
//! counts, and names, and nothing else: no payloads, no decompressed data,
//! no hashes of ROM regions beyond the whole-file SHA-1 `rom_import`
//! already ships.

mod audio;
mod emit;
mod error;
mod fonts;
mod images;
mod layouts;
mod locate;
mod map_file;
mod pack_source;
mod palettes;
mod plan;
mod search;
mod sprites;
mod text_window;
mod tilesets;
mod title;

use std::path::{Path, PathBuf};

use rom_import::{select_profile, Rom};

pub use error::GenRomProfileError;
use map_file::SymbolMap;
use pack_source::PackSource;
use plan::{ProfilePlan, ReportLine, SymbolExpectation};
use search::{Lz77Search, PointerIndex, RawSearch};

/// Where the generated profile is committed, relative to the repo root.
pub const PROFILE_RELATIVE_PATH: &str = "crates/rom-import/src/profiles/bpee_rev0.rs";

/// One `gen-rom-profile` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The ROM to derive addresses from.
    pub rom: PathBuf,
    /// Where to write the generated module. Defaults to
    /// [`PROFILE_RELATIVE_PATH`] under the repo root.
    pub out: Option<PathBuf>,
    /// An optional GNU `ld` map to cross-check every address against.
    pub map: Option<PathBuf>,
}

/// What one generator run derived.
#[derive(Debug, Clone)]
pub struct GenReport {
    /// Where the module was written.
    pub out_path: PathBuf,
    /// One line per located root.
    pub lines: Vec<ReportLine>,
    /// How many roots the `--map` cross-check confirmed by exact name.
    pub map_named: usize,
    /// How many roots the cross-check confirmed as some symbol's own
    /// address, without asserting a name.
    pub map_confirmed: usize,
    /// How many roots the cross-check skipped as interior addresses.
    pub map_skipped: usize,
    /// Whether a map was given at all.
    pub map_used: bool,
}

impl GenReport {
    /// How many roots were located.
    #[must_use]
    pub fn root_count(&self) -> usize {
        self.lines.len()
    }

    /// How many roots needed something other than a unique signature.
    #[must_use]
    pub fn resolved_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.resolution != plan::Resolution::UniqueSignature)
            .count()
    }

    /// How many roots were picked arbitrarily from identical candidates.
    #[must_use]
    pub fn arbitrary_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.resolution == plan::Resolution::ArbitraryAmongIdentical)
            .count()
    }
}

/// Everything the locators read.
pub struct Context<'a> {
    /// The validated ROM image.
    pub rom: &'a [u8],
    /// The pack that says what each asset's bytes are.
    pub pack: &'a PackSource,
    /// Batch search for uncompressed bytes.
    pub raw: RawSearch<'a>,
    /// Batch search for LZ77 streams.
    pub lz77: Lz77Search<'a>,
    /// Every aligned cartridge pointer in the image.
    pub pointers: PointerIndex,
    /// The `pokeemerald/` reference checkout.
    pub upstream: PathBuf,
}

/// Derive the profile and write it.
///
/// # Errors
///
/// [`GenRomProfileError::RomUnusable`] if the ROM is not the supported
/// build, [`GenRomProfileError::OutputIsRom`] if the output names the same
/// file as the ROM, [`GenRomProfileError::PackUnreadable`] without a pack
/// to compare against, any locator failure,
/// [`GenRomProfileError::MapMismatch`] if a `--map` cross-check disagrees,
/// or [`GenRomProfileError::WriteFailed`].
pub fn run(options: &Options) -> Result<GenReport, GenRomProfileError> {
    let repo_root = crate::extract::repo_root();
    let rom = Rom::load(&options.rom).map_err(|err| GenRomProfileError::RomUnusable {
        path: options.rom.clone(),
        reason: err.to_string(),
    })?;

    let out_path = options
        .out
        .clone()
        .unwrap_or_else(|| repo_root.join(PROFILE_RELATIVE_PATH));
    // Refused before the search, not at the write: the answer cannot change
    // during a run, and a developer who spelled one file two ways should
    // hear about it before minutes of scanning, not after.
    //
    // Before `select_profile` too, deliberately. The ROM is the one input
    // here that cannot be regenerated -- a developer's own cartridge dump,
    // which this project never ships -- and `Rom::load` has already read
    // the whole of it into memory, so nothing downstream would notice the
    // output landing on it and `write_module` would truncate the cartridge
    // image into a Rust file. Whether the image is the *supported* build is
    // a smaller question than whether this run is about to destroy it, so
    // it is asked second.
    //
    // The shipped importer refuses the same shape for the same reason
    // (`ImportRomError::DestinationIsSource`); this is that refusal on the
    // developer tool, through the helper `rom_import` exposes for it. Hard
    // links and symlink aliases are covered, which comparing the two path
    // strings would miss.
    if rom_import::overwrites_rom(&options.rom, &out_path) {
        return Err(GenRomProfileError::OutputIsRom {
            rom_path: options.rom.clone(),
            out_path,
        });
    }

    // Heuristics only ever run against a ROM whose whole-file hash already
    // matched a shipped profile.
    let profile = select_profile(&rom).map_err(|err| GenRomProfileError::RomUnusable {
        path: options.rom.clone(),
        reason: err.to_string(),
    })?;

    let pack = PackSource::load(&repo_root.join(pack_format::OUTPUT_RELATIVE_PATH))?;
    let ctx = Context {
        rom: rom.bytes(),
        pack: &pack,
        raw: RawSearch::new(rom.bytes()),
        lz77: Lz77Search::new(rom.bytes()),
        pointers: PointerIndex::build(rom.bytes()),
        upstream: repo_root.join("pokeemerald"),
    };

    let mut lines = Vec::new();
    let plan = ProfilePlan {
        tilesets: tilesets::locate(&ctx, &mut lines)?,
        title_screen: title::locate(&ctx, &mut lines)?,
        sprites: sprites::locate(&ctx, &mut lines)?,
        layouts: layouts::locate(&ctx, &mut lines)?,
        fonts: fonts::locate(&ctx, &mut lines)?,
        text_window: text_window::locate(&ctx, &mut lines)?,
        interface: palettes::locate_unique(
            &ctx,
            &pack.ids_with_prefix("interface/palette/"),
            &interface_palette_symbol,
            &mut lines,
        )?,
        audio: audio::locate(&ctx, &mut lines)?,
    };

    let map = cross_check(options.map.as_deref(), &mut lines)?;

    let module = emit::module(&plan, &profile.sha1.to_string(), lines.len());
    write_module(&out_path, &module)?;

    Ok(GenReport {
        out_path,
        lines,
        map_named: map.named,
        map_confirmed: map.confirmed,
        map_skipped: map.skipped,
        map_used: map.used,
    })
}

/// What a linker map should say at an interface palette's address.
///
/// Upstream names these after the file, in `sCamelCasePal` form.
fn interface_palette_symbol(id: &str) -> SymbolExpectation {
    let stem = id.trim_start_matches("interface/palette/");
    SymbolExpectation::Contains(vec![format!("{}Pal", locate::camel_case(stem))])
}

/// What a `--map` cross-check settled.
#[derive(Debug, Default, Clone, Copy)]
struct MapCheck {
    /// Roots whose expected symbol name the map confirmed.
    named: usize,
    /// Roots the map placed a symbol at, without a name to assert.
    confirmed: usize,
    /// Roots skipped because their address is interior to another symbol.
    skipped: usize,
    /// Whether a map was given at all.
    used: bool,
}

/// Cross-check every root against a linker map, if one was given.
///
/// Three outcomes per root, driven by what the locator recorded:
/// an interior address is skipped, an unnamed root only has to have *some*
/// symbol starting at it, and a named root's symbol has to match. A
/// disagreement is a failure, not an annotation: the whole point of the
/// map is to be a second, independent witness, and a witness that only
/// ever agrees is not one.
fn cross_check(
    map_path: Option<&Path>,
    lines: &mut [ReportLine],
) -> Result<MapCheck, GenRomProfileError> {
    let Some(map_path) = map_path else {
        return Ok(MapCheck::default());
    };
    let map = SymbolMap::load(map_path)?;
    let mut check = MapCheck {
        used: true,
        ..MapCheck::default()
    };
    for line in lines.iter_mut() {
        if line.symbol == SymbolExpectation::Interior {
            check.skipped += 1;
            continue;
        }
        let symbols = map.symbols_at(line.addr);
        let accepted = match &line.symbol {
            SymbolExpectation::Interior => unreachable!("skipped above"),
            SymbolExpectation::Unnamed => !symbols.is_empty(),
            SymbolExpectation::Exact(name) => symbols.iter().any(|found| found == name),
            SymbolExpectation::Contains(fragments) => symbols
                .iter()
                .any(|found| fragments.iter().all(|fragment| found.contains(fragment))),
        };
        if !accepted {
            return Err(GenRomProfileError::MapMismatch {
                id: line.id.clone(),
                generated: line.addr,
                expected: describe(&line.symbol),
                found: symbols.join(", "),
            });
        }
        if matches!(line.symbol, SymbolExpectation::Unnamed) {
            check.confirmed += 1;
        } else {
            check.named += 1;
        }
        let named = symbols.join(", ");
        line.note = Some(match line.note.take() {
            Some(existing) => format!("{existing}; map: {named}"),
            None => format!("map: {named}"),
        });
    }
    Ok(check)
}

/// Render an expectation for an error message.
fn describe(expectation: &SymbolExpectation) -> String {
    match expectation {
        SymbolExpectation::Interior => "an interior address".to_owned(),
        SymbolExpectation::Unnamed => "any symbol".to_owned(),
        SymbolExpectation::Exact(name) => format!("the symbol `{name}`"),
        SymbolExpectation::Contains(fragments) => {
            format!("a symbol containing {}", fragments.join(" + "))
        }
    }
}

/// Write the generated module, creating its directory if needed.
///
/// Through a sibling temporary file and a rename, never a truncating write
/// onto `path`. Two reasons, and the second is why it is not merely tidy:
///
/// 1. A run that dies mid-write leaves the previous profile intact instead
///    of a half-written module that will not compile — the same reason
///    `import_rom` and `engine`'s save writer publish by rename.
/// 2. It replaces a *name*, so it cannot destroy a file reachable under
///    another one. [`run`]'s guard catches an `--out` that resolves to the
///    ROM, but `rom_import::overwrites_rom` cannot see a hard link off
///    Unix (`std` exposes no file identity there). A truncating write
///    through such an alias would destroy the cartridge image under every
///    one of its names; a rename retires only the alias, and the ROM
///    survives under the name `--rom` gave.
///
/// Same directory on both sides, so the rename is atomic and never
/// `EXDEV`.
fn write_module(path: &Path, module: &str) -> Result<(), GenRomProfileError> {
    let failed = |err: std::io::Error| GenRomProfileError::WriteFailed {
        path: path.to_path_buf(),
        reason: err.to_string(),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }

    let temp = temp_sibling(path);
    std::fs::write(&temp, module).map_err(failed)?;
    std::fs::rename(&temp, path).map_err(|err| {
        // The rename is the only step that publishes anything, so a failure
        // here leaves the old profile in place; the temporary file would
        // just be litter beside it.
        let _ = std::fs::remove_file(&temp);
        failed(err)
    })
}

/// A scratch name beside `path`, in the same directory so the rename that
/// follows it stays within one filesystem.
///
/// The process id and a counter keep two concurrent runs (or one run and a
/// leftover from a killed one) off each other's name. Not a security
/// boundary — this is a developer tool writing into a checkout — just
/// enough to keep the tool from tripping over itself.
fn temp_sibling(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut name = std::ffi::OsString::from(".");
    name.push(
        path.file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("profile")),
    );
    name.push(format!(".{}.{sequence}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests;
