//! `cargo xtask extract` (S-4, F-3): builds the local, gitignored asset
//! pack from the developer's `pokeemerald/` reference checkout.
//!
//! Implements the owner-decided "policy A" from Discussion #71: extraction
//! reads the local checkout `./init.sh` fetches and writes a deterministic,
//! versioned pack to disk (format: [`pack`]); it never runs automatically
//! (only an explicit `cargo xtask extract` invocation triggers it, wired in
//! `crate::dispatch`), and the pack itself is never committed, never a CI
//! artifact, and never embedded in a binary (`.gitignore` excludes
//! [`OUTPUT_RELATIVE_PATH`] at the repo root).
//!
//! Deliberately std-only (`minimal-deps`): no image-decoding crate is
//! available, so [`png`] and [`inflate`] reimplement just enough of PNG and
//! DEFLATE to read upstream's own graphics sources (see their module docs
//! for the exact subset). [`jasc_pal`] reads the JASC-PAL palette files the
//! same sources ship alongside.
//!
//! # What this pipeline extracts
//!
//! Per the issue's scope (v1-path maps at minimum, plus title-screen
//! graphics, plus player/NPC sprite sheets):
//!
//! - **Five tilesets** — every one Littleroot Town's family of layouts
//!   references (`LAYOUT_LITTLEROOT_TOWN` and its house/lab interiors, per
//!   `pokeemerald/data/layouts/layouts.json`): primary `general` and
//!   `building`; secondary `petalburg`, `brendans_mays_house`, `lab`. For
//!   each, this pipeline extracts the *entire* upstream tileset directory
//!   (`data/tilesets/{primary,secondary}/<name>/`): `tiles.png`, every
//!   `anim/**/*.png` animation frame (present for `general` and
//!   `building` only), all 16 `palettes/NN.pal` files, and
//!   `metatiles.bin` / `metatile_attributes.bin` as opaque
//!   [`pack::PackKind::Raw`] blobs (upstream ships these as flat binary
//!   files directly, not compiled from another source — see
//!   `crates/assets/src/map_layouts.rs`'s docs for the identical
//!   raw-binary-source situation with `map.bin`/`border.bin`). Extracting
//!   every file in these five directories, not just `tiles.png`, is what
//!   lets the ledger entries for them close as fully `ported` rather than
//!   partially covered.
//! - **Title screen** (`graphics/title_screen/`): all 6 PNGs, all 3 `.pal`
//!   files, and the 3 `.bin` files (`pokemon_logo.bin`, `clouds.bin`,
//!   `rayquaza.bin` — tile-arrangement data this pipeline doesn't
//!   interpret) as raw blobs. Again the whole directory, for the same
//!   ledger reason.
//! - **Player/NPC sprites** (`graphics/object_events/pics/people/`): every
//!   PNG in the directory (133 files: Brendan's and May's own animation
//!   sheets, plus the full upstream NPC roster — nurses, gym leaders,
//!   rivals, etc.) — one bounded, self-contained upstream directory,
//!   matching the issue's "player/NPC sprite sheets" wording without
//!   guessing at a narrower cut. Each image entry stands alone (no
//!   embedded colour table, see [`png`]'s docs); the two player characters
//!   additionally get their real in-game palette
//!   (`graphics/object_events/palettes/{brendan,may}.pal`) extracted
//!   alongside, since I-3 (protagonist's room) needs a paletted player
//!   sprite and those two files are an unambiguous, direct match (no
//!   NPC-to-palette table lookup needed, unlike the generic NPC roster,
//!   which is why the other ~130 NPC palettes are *not* extracted here —
//!   documented as a deferred item).
//!
//! Explicitly **not** extracted (deferred to future slices, not silently
//! dropped): metatile-to-tile mapping beyond the raw `metatiles.bin` bytes
//! (no decode/typed access yet — that's a rendering-layer concern once
//! `crates/rendering` needs it), NPC-specific palette assignment (the
//! `object_event_graphics_info` indirection table), and any tileset outside
//! the five above (every other `data/tilesets/*` directory stays `pending`
//! in the ledger).
//!
//! # Asset id scheme
//!
//! - `tileset/<name>/tiles`
//! - `tileset/<name>/anim/<anim-name>/<frame>`
//! - `tileset/<name>/palette/<NN>`
//! - `tileset/<name>/metatiles`, `tileset/<name>/metatile-attributes`
//! - `title/image/<name>`, `title/palette/<name>`, `title/raw/<name>`
//! - `sprite/<relative-path>` (e.g. `sprite/brendan/walking`, `sprite/nurse`)
//! - `sprite/palette/brendan`, `sprite/palette/may`
//!
//! `<name>` is always a normalized, stable identifier (upstream's own
//! directory/file naming, which is already `snake_case` and stable across
//! decomp revisions) — never a `gTileset_*`-style linker symbol. See
//! [`pack`]'s module docs for why that matters.

mod error;
pub mod inflate;
pub mod jasc_pal;
pub mod pack;
pub mod png;

use std::path::{Path, PathBuf};

pub use error::ExtractError;
use pack::{PackEntry, PackKind, PackWriter};

/// The pack's location, relative to the repository root: a top-level,
/// gitignored directory (mirroring how `pokeemerald/`/`mgba/` are also
/// top-level gitignored reference dirs) rather than something under
/// `target/`, so it survives `cargo clean`.
pub const OUTPUT_RELATIVE_PATH: &str = "assets-pack/pokeemerald.pack";

/// A summary of a completed extraction, printed by `xtask`'s `main` and
/// useful for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractReport {
    /// Number of assets written to the pack.
    pub entry_count: usize,
    /// The pack file's total size in bytes.
    pub pack_size: u64,
    /// Where the pack was written.
    pub output_path: PathBuf,
}

/// The repository root, computed from this crate's own manifest directory
/// (`crates/xtask`) rather than the process's current directory — robust
/// regardless of where `cargo xtask extract` is invoked from.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/xtask is always two levels under the repo root")
        .to_path_buf()
}

/// Whether a local `pokeemerald/` reference checkout looks present (i.e.
/// `./init.sh` has been run) — a cheap, read-only check with no side
/// effects, so tests can decide whether to exercise the real pipeline
/// without ever triggering it accidentally. Test-only: nothing in the real
/// `extract`/`dispatch` path needs this (they just attempt the real
/// extraction and surface [`ExtractError::MissingUpstreamCheckout`] if it
/// fails), so this is `#[cfg(test)]` rather than always-compiled dead code.
#[cfg(test)]
#[must_use]
pub(crate) fn upstream_present() -> bool {
    repo_root().join("pokeemerald/graphics").is_dir()
}

/// Run the full extraction pipeline: locate the upstream checkout, decode
/// every in-scope asset, and write the pack to
/// `<repo root>/`[`OUTPUT_RELATIVE_PATH`].
///
/// # Errors
///
/// See [`extract_to`].
pub fn run() -> Result<ExtractReport, ExtractError> {
    extract_to(&repo_root().join(OUTPUT_RELATIVE_PATH))
}

/// The full extraction pipeline, parameterized over the output path.
///
/// Split out from [`run`] so tests can point separate, concurrent
/// invocations at their own scratch paths rather than racing each other
/// over the one real pack path `run` uses (`cargo test` runs tests on
/// multiple threads by default).
///
/// # Errors
///
/// [`ExtractError::MissingUpstreamCheckout`] if `./init.sh` has not been
/// run; [`ExtractError::ReadFailed`]/[`Png`](ExtractError::Png)/
/// [`Pal`](ExtractError::Pal) if a specific source file is missing or
/// malformed; [`ExtractError::Pack`] if assembling the final pack fails (an
/// internal-bug case — every id is generated by this module, never
/// user-supplied); [`ExtractError::WriteFailed`] if the pack can't be
/// written to disk.
fn extract_to(output_path: &Path) -> Result<ExtractReport, ExtractError> {
    let upstream = repo_root().join("pokeemerald");
    if !upstream.join("graphics").is_dir() {
        return Err(ExtractError::MissingUpstreamCheckout(upstream));
    }

    let mut writer = PackWriter::new();

    for (subdir, name) in TILESETS {
        extract_tileset(&upstream, subdir, name, &mut writer)?;
    }
    extract_title_screen(&upstream, &mut writer)?;
    extract_sprites(&upstream, &mut writer)?;

    let entry_count = writer.len();
    let bytes = writer.finish()?;
    let pack_size = bytes.len() as u64;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ExtractError::WriteFailed(output_path.to_path_buf(), e.to_string()))?;
    }
    std::fs::write(output_path, &bytes)
        .map_err(|e| ExtractError::WriteFailed(output_path.to_path_buf(), e.to_string()))?;

    Ok(ExtractReport {
        entry_count,
        pack_size,
        output_path: output_path.to_path_buf(),
    })
}

/// `(upstream subdir under data/tilesets/, tileset name)` — the five
/// tilesets Littleroot Town's layout family references. See the module
/// docs for how this list was derived.
const TILESETS: [(&str, &str); 5] = [
    ("primary", "general"),
    ("primary", "building"),
    ("secondary", "petalburg"),
    ("secondary", "brendans_mays_house"),
    ("secondary", "lab"),
];

fn read_file(path: &Path) -> Result<Vec<u8>, ExtractError> {
    std::fs::read(path).map_err(|e| ExtractError::ReadFailed(path.to_path_buf(), e.to_string()))
}

fn read_text(path: &Path) -> Result<String, ExtractError> {
    std::fs::read_to_string(path)
        .map_err(|e| ExtractError::ReadFailed(path.to_path_buf(), e.to_string()))
}

fn decode_png_entry(path: &Path, id: String, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let bytes = read_file(path)?;
    let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.to_path_buf(), e))?;
    writer.push(PackEntry {
        id,
        kind: PackKind::Image {
            width: image.width,
            height: image.height,
            bit_depth: image.bit_depth,
        },
        payload: image.pixels,
    });
    Ok(())
}

fn decode_palette_entry(
    path: &Path,
    id: String,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let text = read_text(path)?;
    let colors = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.to_path_buf(), e))?;
    let mut payload = Vec::with_capacity(colors.len() * 2);
    for color in &colors {
        payload.extend_from_slice(&color.to_gba555().to_le_bytes());
    }
    #[allow(clippy::cast_possible_truncation)]
    writer.push(PackEntry {
        id,
        kind: PackKind::Palette {
            color_count: colors.len() as u16,
        },
        payload,
    });
    Ok(())
}

fn raw_entry(path: &Path, id: String, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let payload = read_file(path)?;
    writer.push(PackEntry {
        id,
        kind: PackKind::Raw,
        payload,
    });
    Ok(())
}

/// Recursively collect every `*.png` under `dir`, sorted by full path —
/// deterministic regardless of the OS's `read_dir` order (see [`pack`]'s
/// determinism docs).
fn collect_pngs_sorted(dir: &Path) -> Result<Vec<PathBuf>, ExtractError> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                walk(&path, out)?;
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, &mut out).map_err(|e| ExtractError::ReadFailed(dir.to_path_buf(), e.to_string()))?;
    out.sort();
    Ok(out)
}

/// Extract one tileset directory's full contents (see the module docs).
fn extract_tileset(
    upstream: &Path,
    subdir: &str,
    name: &str,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let base = upstream.join("data/tilesets").join(subdir).join(name);

    decode_png_entry(
        &base.join("tiles.png"),
        format!("tileset/{name}/tiles"),
        writer,
    )?;

    let anim_dir = base.join("anim");
    if anim_dir.is_dir() {
        for png_path in collect_pngs_sorted(&anim_dir)? {
            // `anim/<anim-name>/<frame>.png` -> id suffix `<anim-name>/<frame>`.
            let rel = png_path
                .strip_prefix(&anim_dir)
                .expect("collect_pngs_sorted only returns paths under anim_dir")
                .with_extension("");
            let rel_id = rel.to_string_lossy().replace('\\', "/");
            decode_png_entry(&png_path, format!("tileset/{name}/anim/{rel_id}"), writer)?;
        }
    }

    for slot in 0..16u8 {
        let pal_path = base.join("palettes").join(format!("{slot:02}.pal"));
        decode_palette_entry(
            &pal_path,
            format!("tileset/{name}/palette/{slot:02}"),
            writer,
        )?;
    }

    raw_entry(
        &base.join("metatiles.bin"),
        format!("tileset/{name}/metatiles"),
        writer,
    )?;
    raw_entry(
        &base.join("metatile_attributes.bin"),
        format!("tileset/{name}/metatile-attributes"),
        writer,
    )?;

    Ok(())
}

/// Extract `graphics/title_screen/`'s full contents.
fn extract_title_screen(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/title_screen");

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .and_then(|it| {
            it.map(|entry| entry.map(|e| e.path()))
                .collect::<std::io::Result<_>>()
        })
        .map_err(|e| ExtractError::ReadFailed(dir.clone(), e.to_string()))?;
    entries.sort();

    for path in entries {
        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        match path.extension().and_then(|e| e.to_str()) {
            Some("png") => decode_png_entry(&path, format!("title/image/{stem}"), writer)?,
            Some("pal") => decode_palette_entry(&path, format!("title/palette/{stem}"), writer)?,
            Some("bin") => raw_entry(&path, format!("title/raw/{stem}"), writer)?,
            _ => {}
        }
    }
    Ok(())
}

/// Extract every player/NPC sprite sheet plus the two player palettes.
fn extract_sprites(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let people_dir = upstream.join("graphics/object_events/pics/people");
    for png_path in collect_pngs_sorted(&people_dir)? {
        let rel = png_path
            .strip_prefix(&people_dir)
            .expect("collect_pngs_sorted only returns paths under people_dir")
            .with_extension("");
        let rel_id = rel.to_string_lossy().replace('\\', "/");
        decode_png_entry(&png_path, format!("sprite/{rel_id}"), writer)?;
    }

    let palettes_dir = upstream.join("graphics/object_events/palettes");
    for who in ["brendan", "may"] {
        decode_palette_entry(
            &palettes_dir.join(format!("{who}.pal")),
            format!("sprite/palette/{who}"),
            writer,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{collect_pngs_sorted, extract_to, upstream_present, ExtractError};

    // Real-checkout tests: `pokeemerald/` must be present locally
    // (`./init.sh`) to run these. `cargo test --workspace` in CI never has
    // it, so every test that calls `extract_to`/`super::run()` is
    // `#[ignore]`d — run explicitly with `cargo test -p xtask -- --ignored`
    // after `./init.sh`.
    //
    // Each test below writes to its own scratch path under `std::env::temp_dir()`
    // (never the real `super::run()` output path) so concurrent test
    // threads never race each other over the same file.

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn full_extraction_round_trips_locally() {
        assert!(upstream_present(), "run ./init.sh first");
        let report = extract_to(&scratch_path("full-round-trip"))
            .expect("extraction should succeed against a real checkout");
        assert!(report.entry_count > 0);
        assert!(report.pack_size > 0);
        assert!(report.output_path.is_file());
        let _ = std::fs::remove_file(report.output_path);
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn extraction_is_byte_identical_across_runs() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("determinism");
        let first = std::fs::read(extract_to(&path).unwrap().output_path).unwrap();
        let second = std::fs::read(extract_to(&path).unwrap().output_path).unwrap();
        assert_eq!(first, second);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_upstream_checkout_message_points_to_init_sh() {
        // Pure `Display` check -- no filesystem access -- so it runs
        // everywhere, unlike the `#[ignore]`d tests above.
        let err = ExtractError::MissingUpstreamCheckout(std::path::PathBuf::from("/nowhere"));
        let rendered = err.to_string();
        assert!(rendered.contains("init.sh"));
        assert!(rendered.contains("cargo xtask extract"));
    }

    #[test]
    fn collect_pngs_sorted_rejects_missing_dir() {
        let err = collect_pngs_sorted(std::path::Path::new("/does/not/exist")).unwrap_err();
        assert!(matches!(err, super::ExtractError::ReadFailed(..)));
    }
}
