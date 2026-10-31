//! `cargo xtask gen-rom-profile` (S-4, F-3, Discussion #71 policy C):
//! derive `rom-import`'s address table from a real ROM, once, and commit
//! the result to `crates/rom-import/src/profiles/bpee_rev0.rs`. The shipped
//! importer never scans a ROM; this developer-only tool is the one place a
//! heuristic runs.
//!
//! Every root is located by searching the ROM for the exact bytes the
//! `cargo xtask extract` pack holds for it (see [`pack_source`]), and is
//! accepted only when exactly one place matches. A tie is broken by a struct
//! back-reference or adjacency to an already-unique root, and the report
//! names which; otherwise it is an error, never a first-match-wins guess.
//! The one exception is [`plan::Resolution::ArbitraryAmongIdentical`], for
//! sheets whose duplicate candidates hold identical bytes, so the choice
//! cannot change the pack entry. The ROM is admitted only after
//! `rom_import`'s profile selection accepts its whole-file SHA-1, and
//! `--map` adds an independent witness ([`map_file`]).
//!
//! No ROM bytes leave this tool: the table carries offsets, lengths,
//! dimensions, counts, and names, plus the whole-file SHA-1 `rom_import`
//! already ships, and no other hash of ROM data. A pack missing the whole
//! fixed asset domain (fonts, tilesets, layouts, interface palettes) is
//! refused.

mod audio;
mod bag;
mod battle_anims;
mod battle_interface;
mod battle_messages;
mod battlers;
mod emit;
mod error;
mod field_messages;
mod fonts;
mod images;
mod inventory;
mod layouts;
mod locate;
mod lz77_blobs;
mod map_file;
mod opening_scene;
mod pack_source;
mod palettes;
mod plan;
mod poke_ball;
mod search;
mod sprites;
mod starter_chooser;
mod text_window;
mod tilesets;
mod title;

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rom_import::{select_profile, Rom};

pub use error::GenRomProfileError;
pub(crate) use map_file::SymbolMap;
use pack_source::PackSource;
use plan::{ProfilePlan, ReportLine, SymbolExpectation};
use search::{Lz77Search, PointerIndex, RawSearch};

/// Default output, relative to the repo root.
pub const PROFILE_RELATIVE_PATH: &str = "crates/rom-import/src/profiles/bpee_rev0.rs";

/// One `gen-rom-profile` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The ROM to derive addresses from.
    pub rom: PathBuf,
    /// Defaults to [`PROFILE_RELATIVE_PATH`] under the repo root.
    pub out: Option<PathBuf>,
    /// A GNU `ld` map to cross-check every address against.
    pub map: Option<PathBuf>,
}

/// What one generator run derived.
#[derive(Debug, Clone)]
pub struct GenReport {
    /// Where the module was written.
    pub out_path: PathBuf,
    /// One line per located root.
    pub lines: Vec<ReportLine>,
    /// Roots the `--map` cross-check confirmed by name.
    pub map_named: usize,
    /// Roots confirmed as some symbol's address, with no name asserted.
    pub map_confirmed: usize,
    /// Roots skipped as interior addresses.
    pub map_skipped: usize,
    /// Roots skipped as translation-unit-local symbols.
    pub map_local: usize,
    /// Whether a map was given.
    pub map_used: bool,
}

impl GenReport {
    /// Roots located.
    #[must_use]
    pub fn root_count(&self) -> usize {
        self.lines.len()
    }

    /// Roots resolved by something other than a unique signature.
    #[must_use]
    pub fn resolved_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.resolution != plan::Resolution::UniqueSignature)
            .count()
    }

    /// Roots picked arbitrarily from identical candidates.
    #[must_use]
    pub fn arbitrary_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.resolution == plan::Resolution::ArbitraryAmongIdentical)
            .count()
    }

    /// `None` without a map.
    #[must_use]
    pub fn map_summary(&self) -> Option<String> {
        self.map_used.then(|| {
            format!(
                "map cross-check: {} symbol name(s) matched, {} address(es) confirmed \
                 unnamed, {} interior address(es) skipped, {} translation-unit-local \
                 symbol(s) skipped",
                self.map_named, self.map_confirmed, self.map_skipped, self.map_local
            )
        })
    }
}

/// Read-only inputs shared by every locator.
pub struct Context<'a> {
    /// The validated ROM image.
    pub rom: &'a [u8],
    /// The pack holding each asset's expected bytes.
    pub pack: &'a PackSource,
    /// Batch search for uncompressed bytes.
    pub raw: RawSearch<'a>,
    /// Batch search for LZ77 streams.
    pub lz77: Lz77Search<'a>,
    /// Every aligned cartridge pointer in the image.
    pub pointers: PointerIndex,
    /// The `pokeemerald/` reference checkout.
    pub upstream: PathBuf,
    /// The `bag/` pack ids the checkout's inventory expects.
    pub bag_ids: Vec<String>,
}

fn locate_profile(
    ctx: &Context<'_>,
    lines: &mut Vec<ReportLine>,
) -> Result<ProfilePlan, GenRomProfileError> {
    inventory::preflight(ctx.pack, &ctx.bag_ids)?;
    Ok(ProfilePlan {
        tilesets: tilesets::locate(ctx, lines)?,
        title_screen: title::locate(ctx, lines)?,
        sprites: sprites::locate(ctx, lines)?,
        layouts: layouts::locate(ctx, lines)?,
        fonts: fonts::locate(ctx, lines)?,
        text_window: text_window::locate(ctx, lines)?,
        interface: locate_interface_palettes(ctx, lines)?,
        audio: audio::locate(ctx, lines)?,
        battle_messages: battle_messages::locate(ctx, lines)?,
        field_messages: field_messages::locate(ctx, lines)?,
        battle_anims: battle_anims::locate(ctx, lines)?,
        bag: bag::locate(ctx, lines)?,
        starter_chooser: starter_chooser::locate(ctx, lines)?,
        opening_scene: opening_scene::locate(ctx, lines)?,
        battle_interface: battle_interface::locate(ctx, lines)?,
        battlers: battlers::locate(ctx, lines)?,
        poke_ball: poke_ball::locate(ctx, lines)?,
    })
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
    // Asked before `select_profile` and before the search: the ROM is the
    // developer's own dump and cannot be regenerated. `write_module` asks
    // again after creating the output directory, which can change the answer.
    // Same refusal as `ImportRomError::DestinationIsSource`.
    if rom_import::overwrites_rom(&options.rom, &out_path) {
        return Err(GenRomProfileError::OutputIsRom {
            rom_path: options.rom.clone(),
            out_path,
        });
    }

    let profile = select_profile(&rom).map_err(|err| GenRomProfileError::RomUnusable {
        path: options.rom.clone(),
        reason: err.to_string(),
    })?;

    let pack = PackSource::load(&repo_root.join(pack_format::OUTPUT_RELATIVE_PATH))?;
    let upstream = repo_root.join("pokeemerald");
    let bag_ids = crate::extract::scope::bag_ids(&upstream).map_err(|err| {
        GenRomProfileError::UpstreamSource {
            path: upstream.clone(),
            reason: err.to_string(),
        }
    })?;
    let ctx = Context {
        rom: rom.bytes(),
        pack: &pack,
        raw: RawSearch::new(rom.bytes()),
        lz77: Lz77Search::new(rom.bytes()),
        pointers: PointerIndex::build(rom.bytes()),
        upstream,
        bag_ids,
    };

    generate(&ctx, &profile.sha1.to_string(), options, out_path)
}

/// Nothing is written unless locating and the map cross-check both succeed.
fn generate(
    ctx: &Context<'_>,
    sha1: &str,
    options: &Options,
    out_path: PathBuf,
) -> Result<GenReport, GenRomProfileError> {
    let mut lines = Vec::new();
    let plan = locate_profile(ctx, &mut lines)?;

    let map = cross_check(options.map.as_deref(), &mut lines)?;

    let module = emit::module(&plan, sha1, &lines);
    write_module(&options.rom, &out_path, &module)?;

    Ok(GenReport {
        out_path,
        lines,
        map_named: map.named,
        map_confirmed: map.confirmed,
        map_skipped: map.skipped,
        map_local: map.local,
        map_used: map.used,
    })
}

/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] if the pack holds no
/// `interface/palette/*` entries at all or lacks a required fixed
/// interface palette, or any other
/// [`GenRomProfileError`] [`palettes::locate_unique`] raises.
fn locate_interface_palettes(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<plan::PalettePlan>, GenRomProfileError> {
    let ids = ctx.pack.ids_with_prefix("interface/palette/");
    if ids.is_empty() {
        return Err(GenRomProfileError::MissingPackEntry(
            "interface/palette/*".to_owned(),
        ));
    }
    inventory::require_interface_palettes(ctx.pack)?;
    palettes::locate_unique(ctx, &ids, &interface_palette_symbol, report)
}

/// The one interface palette extraction emits is `sMainMenuBgPal`, declared
/// `static` (`main_menu.c:406`), so a linker map cannot list it. An id with no
/// cited declaration keeps the any-symbol check instead of a guessed claim.
fn interface_palette_symbol(id: &str) -> SymbolExpectation {
    match id {
        crate::extract::scope::MAIN_MENU_BG_PALETTE_ID => {
            SymbolExpectation::Local("sMainMenuBgPal".to_owned())
        }
        _ => SymbolExpectation::Unnamed,
    }
}

/// Mark every line of `lines` that `local` names as a translation-unit-local
/// `static`, which a linker map does not list.
///
/// A locator that reports through shared helpers
/// ([`images::locate_images`], [`palettes::locate_unique`]) leaves each root
/// [`SymbolExpectation::Unnamed`]; its domain calls this once on the lines it
/// added, from a table of upstream declarations.
fn mark_local_roots(lines: &mut [ReportLine], local: &dyn Fn(&str) -> Option<&'static str>) {
    for line in lines {
        if let Some(name) = local(&line.id) {
            line.symbol = SymbolExpectation::Local(name.to_owned());
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct MapCheck {
    /// Roots whose expected symbol name the map confirmed.
    named: usize,
    /// Roots the map placed a symbol at, without a name to assert.
    confirmed: usize,
    /// Roots skipped because their address is interior to another symbol.
    skipped: usize,
    /// Roots skipped because their symbol is local to a translation unit.
    local: usize,
    /// Whether a map was given at all.
    used: bool,
}

/// Interior and translation-unit-local roots are skipped (a linker map lists
/// only global symbols). A disagreement is an error, not an annotation.
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
        match line.symbol {
            SymbolExpectation::Interior => {
                check.skipped += 1;
                continue;
            }
            SymbolExpectation::Local(_) => {
                check.local += 1;
                continue;
            }
            _ => {}
        }
        let symbols = map.symbols_at(line.addr);
        let accepted = match &line.symbol {
            SymbolExpectation::Interior | SymbolExpectation::Local(_) => {
                unreachable!("skipped above")
            }
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

fn describe(expectation: &SymbolExpectation) -> String {
    match expectation {
        SymbolExpectation::Interior => "an interior address".to_owned(),
        SymbolExpectation::Local(name) => format!("the translation-unit-local `{name}`"),
        SymbolExpectation::Unnamed => "any symbol".to_owned(),
        SymbolExpectation::Exact(name) => format!("the symbol `{name}`"),
        SymbolExpectation::Contains(fragments) => {
            format!("a symbol containing {}", fragments.join(" + "))
        }
    }
}

/// Repeats [`run`]'s ROM-overwrite guard after creating the parent: an
/// `--out absent/../emerald.gba` beside the ROM resolves to nothing until
/// the parent exists, and `rom_import::overwrites_rom` answers `false` for
/// what it cannot resolve.
fn write_module(rom_path: &Path, path: &Path, module: &str) -> Result<(), GenRomProfileError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| GenRomProfileError::WriteFailed {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
    }
    if rom_import::overwrites_rom(rom_path, path) {
        return Err(GenRomProfileError::OutputIsRom {
            rom_path: rom_path.to_path_buf(),
            out_path: path.to_path_buf(),
        });
    }
    publish(path, module)
}

/// Never a truncating write onto `path`: the rename replaces a name, so a
/// hard link to the ROM that `rom_import::overwrites_rom` cannot see off
/// Unix loses only the alias. A mid-write death also keeps the old profile.
fn publish(path: &Path, module: &str) -> Result<(), GenRomProfileError> {
    publish_with(path, |file, _temp| file.write_all(module.as_bytes()))
}

/// [`publish`] with the write injected for tests; `write` receives the
/// temporary path picked inside this call.
fn publish_with(
    path: &Path,
    write: impl FnOnce(&mut std::fs::File, &Path) -> std::io::Result<()>,
) -> Result<(), GenRomProfileError> {
    let failed = |err: std::io::Error| GenRomProfileError::WriteFailed {
        path: path.to_path_buf(),
        reason: err.to_string(),
    };
    let temp = temp_sibling(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(failed)?;
    // The umask can have taken the owner's read bit, and the rename below
    // would carry that mode onto the published module.
    let restored = restore_owner_access(&file);
    match restored
        .and_then(|()| write(&mut file, &temp))
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::rename(&temp, path))
    {
        Ok(()) => Ok(()),
        Err(err) => {
            // Windows refuses to unlink an open file.
            drop(file);
            Err(failed(remove_after(&temp, err)))
        }
    }
}

/// Give `file`'s owner read and write access if the umask took either, and
/// leave every other mode bit as it is.
///
/// `OpenOptions` asks for `0o666`, and a umask that strips the owner bits
/// (`0444`) leaves a file its owner can no longer read; the publishing rename
/// would install that mode over a readable module. The mode changes only when
/// an owner bit is missing, through the descriptor that created the file.
#[cfg(unix)]
fn restore_owner_access(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    const OWNER: u32 = 0o600;
    let mut permissions = file.metadata()?.permissions();
    let mode = permissions.mode();
    if mode & OWNER != OWNER {
        permissions.set_mode(mode | OWNER);
        file.set_permissions(permissions)?;
    }
    Ok(())
}

/// Off Unix a file has no owner mode bits to restore.
#[cfg(not(unix))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "one signature for every platform; only the unix arm can fail to change a mode"
)]
fn restore_owner_access(_file: &std::fs::File) -> std::io::Result<()> {
    Ok(())
}

/// Folds a cleanup failure into `original`, keeping its `ErrorKind`, because
/// [`temp_sibling`] never reuses a name and nothing would clean it later.
/// `NotFound` means nothing was left to remove.
fn remove_after(path: &Path, original: std::io::Error) -> std::io::Error {
    match std::fs::remove_file(path) {
        Ok(()) => original,
        Err(cleanup_err) if cleanup_err.kind() == std::io::ErrorKind::NotFound => original,
        Err(cleanup_err) => std::io::Error::new(
            original.kind(),
            format!(
                "{original} (additionally, failed to remove partial profile `{}`: {cleanup_err})",
                path.display()
            ),
        ),
    }
}

/// Same directory as `path`, so the rename never crosses filesystems. The
/// clock and `RandomState` salt keep concurrent runs apart where a PID
/// alone would not (separate PID namespaces); [`publish`]'s exclusive
/// create turns a collision into an error.
fn temp_sibling(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let salt = RandomState::new().build_hasher().finish();
    // No basename, so the name fits any component limit the output's does.
    path.with_file_name(format!(
        ".profile.{}.{nanos:x}.{salt:x}.tmp",
        std::process::id()
    ))
}

#[cfg(test)]
mod battle_anim_tests;
#[cfg(test)]
mod battle_message_tests;
#[cfg(test)]
mod field_message_tests;
#[cfg(test)]
mod inventory_tests;
#[cfg(test)]
mod tests;
