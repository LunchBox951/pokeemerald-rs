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
//! - **Map layout grids** (S-4): the Littleroot Town layout family's
//!   `map.bin` / `border.bin` grid files — the town itself plus every
//!   interior it contains (both player houses' floors, Professor Birch's
//!   lab, and its rarely-referenced "with table" variant; see [`LAYOUTS`]).
//!   Resolved via `data/layouts/layouts.json` ([`layouts_json`]) rather than
//!   hardcoded upstream paths, and extracted as opaque
//!   [`pack::PackKind::Raw`] blobs — same rationale as `metatiles.bin` /
//!   `metatile_attributes.bin` above: these are upstream's own flat binary
//!   files, not compiled from another source. `crates/assets::map_layouts`
//!   already ships the typed decode layer (`MetatileCell`, `LayoutGrid`,
//!   `BorderGrid`) that reads these bytes back out of the pack; this
//!   pipeline only needs to get the bytes *into* the pack.
//!
//! - **Fonts** (S-4, issue #114): the five upstream Latin glyph sheets
//!   (`graphics/fonts/latin_{normal,narrow,short,small,small_narrow}.png`
//!   — see [`FONTS`]), each a 256x512, 16-column x 32-row grid of 512
//!   16x16-pixel glyph cells, decoded via [`png::decode`]'s new bit-depth-2
//!   support (these sheets are 2bpp — 4 colours, `gbagfx`'s
//!   `SetFontPalette` — unlike tilesets'/sprites' 4/8bpp). Per-glyph advance
//!   widths (upstream `gFont*LatinGlyphWidths`) are *not* in the pack —
//!   they're a small, stable table of constants, ported directly as Rust
//!   data in `crates/assets::fonts` instead (see that module's docs).
//!   Japanese/braille/keypad/arrow glyph sheets (the other 12 files under
//!   `graphics/fonts/`) are **not** extracted — v1 is English-only text, so
//!   they stay `pending` in the ledger.
//!
//! - **Text window frames** (S-4, issue #114): every file under
//!   `graphics/text_window/` — all 20 numbered border-frame tile sheets
//!   (`1.png`..`20.png`, upstream `sWindowFrames`/`WINDOW_FRAMES_COUNT`,
//!   `pokeemerald/src/text_window.c`), the default message-box tile sheet
//!   (`message_box.png`, upstream `gMessageBox_Gfx`), and the four extra
//!   textbox colour palettes (`text_pal1.pal`..`text_pal4.pal`, upstream
//!   `sTextWindowPalettes`).
//!   Unlike tilesets/sprites, the frame and message-box PNGs have no sibling
//!   `.pal` file of their own — upstream's own `INCGFX_U16(..., ".gbapal")`
//!   build rule for them reads the palette straight out of each PNG's own
//!   `PLTE` chunk, so this pipeline does too, via [`png::decode_palette`]
//!   (see that function's docs for why it's a separate read path from
//!   [`png::decode`]). This is every file in the directory, so
//!   `graphics/text_window` closes fully `ported` in the ledger (unlike
//!   `graphics/fonts` above).
//!
//! Explicitly **not** extracted (deferred to future slices, not silently
//! dropped): metatile-to-tile mapping beyond the raw `metatiles.bin` bytes
//! (no decode/typed access yet — that's a rendering-layer concern once
//! `crates/rendering` needs it), NPC-specific palette assignment (the
//! `object_event_graphics_info` indirection table), any tileset outside the
//! five above (every other `data/tilesets/*` directory stays `pending` in
//! the ledger), any map layout outside the Littleroot Town family above
//! (every other `data/layouts/*` directory likewise stays `pending`), and
//! every non-Latin font sheet under `graphics/fonts/` (see above).
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
//! - `layout/<name>/map`, `layout/<name>/border` (e.g.
//!   `layout/littleroot_town/map`, `layout/littleroot_town/border`)
//! - `font/<name>/glyphs` (e.g. `font/normal/glyphs` — `<name>` is the
//!   upstream `FONT_*` id, lowercased: `small`, `normal`, `short`, `narrow`,
//!   `small_narrow`; see [`FONTS`])
//! - `text-window/image/<stem>`, `text-window/palette/<stem>` — `<stem>` is
//!   the upstream filename's stem (e.g. `1` .. `20`, `message_box`,
//!   `text_pal1` .. `text_pal4`), mirroring `title/image/<name>` above.
//!   `text-window/palette/1`..`20`/`message_box` come from each PNG's own
//!   `PLTE`; `text-window/palette/text_pal1`..`4` come from the sibling
//!   `.pal` files.
//!
//! `<name>` is always a normalized, stable identifier (upstream's own
//! directory/file naming, which is already `snake_case` and stable across
//! decomp revisions) — never a `gTileset_*`-style linker symbol. See
//! [`pack`]'s module docs for why that matters.

mod error;
pub mod inflate;
pub mod jasc_pal;
mod layouts_json;
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
    extract_layouts(&upstream, &mut writer)?;
    extract_fonts(&upstream, &mut writer)?;
    extract_text_window(&upstream, &mut writer)?;

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
    push_palette_entry(&colors, id, writer);
    Ok(())
}

/// Serialize already-decoded colours (from either a JASC `.pal` file or a
/// PNG's own `PLTE` chunk — see [`extract_text_window`]) into a
/// [`PackKind::Palette`] entry.
fn push_palette_entry(colors: &[jasc_pal::Rgb888], id: String, writer: &mut PackWriter) {
    let mut payload = Vec::with_capacity(colors.len() * 2);
    for color in colors {
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

/// `(LAYOUT_* id, normalized pack name)` — the Littleroot Town layout
/// family this pipeline extracts: the town itself plus every interior it
/// contains. See the module docs for why this list, not the full
/// `data/layouts/` tree, is in scope.
///
/// Every id here is looked up in `layouts.json` at extract time
/// ([`extract_layouts`]) rather than the directory name being derived
/// mechanically from it, so a typo here surfaces as a clear
/// [`ExtractError::UnknownLayoutInJson`] instead of a silently-wrong path.
const LAYOUTS: [(&str, &str); 7] = [
    ("LAYOUT_LITTLEROOT_TOWN", "littleroot_town"),
    (
        "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        "littleroot_town_brendans_house_1f",
    ),
    (
        "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        "littleroot_town_brendans_house_2f",
    ),
    (
        "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        "littleroot_town_mays_house_1f",
    ),
    (
        "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        "littleroot_town_mays_house_2f",
    ),
    (
        "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        "littleroot_town_professor_birchs_lab",
    ),
    (
        // Upstream's rarely-used variant with an extra table prop; its
        // `map.bin` is one of the handful with trailing padding beyond
        // `width * height * 2` bytes (see
        // `crates/assets::map_layouts`'s module docs) -- included anyway
        // since it's the same directory family and the extraction below
        // doesn't need to treat it specially (the padding is inside the
        // grid `LayoutGrid` already knows to tolerate, not in how this
        // pipeline copies the file).
        "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
        "littleroot_town_professor_birchs_lab_with_table",
    ),
];

/// Every upstream `border.bin` is a fixed 2x2 grid of `u16` cells (see
/// `crates/assets::map_layouts::BORDER_CELLS`, duplicated here rather than
/// shared -- this pipeline's crate never depends on `crates/assets`, and
/// vice versa, matching `crates/assets::pack`'s documented decoupling from
/// this module's own pack format).
const BORDER_BLOCK_BYTES: usize = 2 * 2 * 2;

/// Extract the Littleroot Town layout family's `map.bin` / `border.bin`
/// grid files (see [`LAYOUTS`] and the module docs).
fn extract_layouts(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let json_path = upstream.join("data/layouts/layouts.json");
    let text = read_text(&json_path)?;
    let entries =
        layouts_json::parse(&text).map_err(|e| ExtractError::LayoutsJson(json_path.clone(), e))?;

    for (layout_id, name) in LAYOUTS {
        let entry =
            entries
                .iter()
                .find(|e| e.id == layout_id)
                .ok_or(ExtractError::UnknownLayoutInJson(
                    json_path.clone(),
                    layout_id,
                ))?;

        let map_bytes = read_file(&upstream.join(&entry.blockdata_filepath))?;
        let width = usize::try_from(entry.width).unwrap_or(usize::MAX);
        let height = usize::try_from(entry.height).unwrap_or(usize::MAX);
        let expected_min = width.saturating_mul(height).saturating_mul(2);
        if map_bytes.len() < expected_min {
            return Err(ExtractError::LayoutGridTooShort {
                layout_id,
                expected: expected_min,
                actual: map_bytes.len(),
            });
        }
        writer.push(PackEntry {
            id: format!("layout/{name}/map"),
            kind: PackKind::Raw,
            payload: map_bytes,
        });

        let border_bytes = read_file(&upstream.join(&entry.border_filepath))?;
        if border_bytes.len() != BORDER_BLOCK_BYTES {
            return Err(ExtractError::LayoutBorderWrongSize {
                layout_id,
                actual: border_bytes.len(),
            });
        }
        writer.push(PackEntry {
            id: format!("layout/{name}/border"),
            kind: PackKind::Raw,
            payload: border_bytes,
        });
    }
    Ok(())
}

/// `(upstream `FONT_*` id, lowercased; `graphics/fonts/` filename)` — the
/// five Latin glyph sheets this pipeline extracts. See the module docs for
/// why only these five, not the other 12 files under `graphics/fonts/`
/// (Japanese, braille, arrows, the keypad icon sheet).
const FONTS: [(&str, &str); 5] = [
    ("small", "latin_small.png"),
    ("normal", "latin_normal.png"),
    ("short", "latin_short.png"),
    ("narrow", "latin_narrow.png"),
    ("small_narrow", "latin_small_narrow.png"),
];

/// Filename stems for every PNG required from `graphics/text_window/`.
const TEXT_WINDOW_IMAGE_STEMS: [&str; 21] = [
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "10",
    "11",
    "12",
    "13",
    "14",
    "15",
    "16",
    "17",
    "18",
    "19",
    "20",
    "message_box",
];

/// Filename stems for every standalone palette required from
/// `graphics/text_window/`.
const TEXT_WINDOW_PALETTE_STEMS: [&str; 4] = ["text_pal1", "text_pal2", "text_pal3", "text_pal4"];

/// Every text-window palette occupies one 16-colour GBA palette bank.
const TEXT_WINDOW_PALETTE_COLORS: usize = 16;

/// Extract the five Latin font glyph sheets (see [`FONTS`] and the module
/// docs). Per-glyph advance widths are not extracted here — they're ported
/// as Rust data directly in `crates/assets::fonts`.
fn extract_fonts(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/fonts");
    for (name, filename) in FONTS {
        decode_png_entry(&dir.join(filename), format!("font/{name}/glyphs"), writer)?;
    }
    Ok(())
}

/// Extract `graphics/text_window/`'s full contents: the 20 numbered
/// border-frame tile sheets, the default message-box tile sheet, and the
/// four extra textbox palettes (see the module docs). Every PNG's palette
/// comes from its own embedded `PLTE` chunk ([`png::decode_palette`]) since
/// none of these files has a sibling `.pal` of its own; the four `text_pal*`
/// files are ordinary JASC `.pal` files, decoded the same way tileset/sprite
/// palettes are.
fn extract_text_window(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/text_window");

    validate_text_window_manifest(&dir)?;

    for stem in TEXT_WINDOW_IMAGE_STEMS {
        let path = dir.join(format!("{stem}.png"));
        decode_png_entry(&path, format!("text-window/image/{stem}"), writer)?;
        let bytes = read_file(&path)?;
        let colors = png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        push_text_window_palette_entry(
            &path,
            &colors,
            format!("text-window/palette/{stem}"),
            writer,
        )?;
    }
    for stem in TEXT_WINDOW_PALETTE_STEMS {
        let path = dir.join(format!("{stem}.pal"));
        let text = read_text(&path)?;
        let colors = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.clone(), e))?;
        push_text_window_palette_entry(
            &path,
            &colors,
            format!("text-window/palette/{stem}"),
            writer,
        )?;
    }
    Ok(())
}

fn push_text_window_palette_entry(
    path: &Path,
    colors: &[jasc_pal::Rgb888],
    id: String,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    if colors.len() != TEXT_WINDOW_PALETTE_COLORS {
        return Err(ExtractError::TextWindowPaletteWrongColorCount(
            path.to_path_buf(),
            colors.len(),
        ));
    }
    push_palette_entry(colors, id, writer);
    Ok(())
}

fn validate_text_window_manifest(dir: &Path) -> Result<(), ExtractError> {
    for (stems, extension) in [
        (TEXT_WINDOW_IMAGE_STEMS.as_slice(), "png"),
        (TEXT_WINDOW_PALETTE_STEMS.as_slice(), "pal"),
    ] {
        for stem in stems {
            let path = dir.join(format!("{stem}.{extension}"));
            if !path.is_file() {
                return Err(ExtractError::MissingTextWindowAsset(path));
            }
        }
    }

    let entries = std::fs::read_dir(dir)
        .map_err(|e| ExtractError::ReadFailed(dir.to_path_buf(), e.to_string()))?;
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExtractError::ReadFailed(dir.to_path_buf(), e.to_string()))?;
    paths.sort();
    for path in paths {
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        let extension = path.extension().and_then(|extension| extension.to_str());
        let is_expected = match extension {
            Some("png") => stem.is_some_and(|stem| TEXT_WINDOW_IMAGE_STEMS.contains(&stem)),
            Some("pal") => stem.is_some_and(|stem| TEXT_WINDOW_PALETTE_STEMS.contains(&stem)),
            _ => false,
        };
        if !is_expected {
            return Err(ExtractError::UnexpectedTextWindowAsset(path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        collect_pngs_sorted, extract_to, push_text_window_palette_entry, upstream_present,
        validate_text_window_manifest, ExtractError, FONTS, LAYOUTS, TEXT_WINDOW_IMAGE_STEMS,
        TEXT_WINDOW_PALETTE_COLORS, TEXT_WINDOW_PALETTE_STEMS,
    };

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

    #[test]
    fn layouts_list_has_no_duplicate_ids_or_names() {
        // Pure data check -- no filesystem access -- so it runs everywhere.
        let ids: Vec<_> = LAYOUTS.iter().map(|(id, _)| *id).collect();
        let names: Vec<_> = LAYOUTS.iter().map(|(_, name)| *name).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        let unique_names: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(ids.len(), unique_ids.len(), "duplicate LAYOUT_* id");
        assert_eq!(names.len(), unique_names.len(), "duplicate pack name");
        for id in &ids {
            assert!(id.starts_with("LAYOUT_LITTLEROOT_TOWN"));
        }
        for name in &names {
            assert!(name.starts_with("littleroot_town"));
            // Pack ids are ASCII lowercase + digits + underscores + `/` only
            // (see `crate::extract`'s "Asset id scheme" docs).
            assert!(name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn layout_grids_are_extracted() {
        // No pack reader lives in this crate (`crates/assets` owns that,
        // and this crate deliberately never depends on it -- see the
        // module docs), so this just confirms every expected `layout/*`
        // id's bytes made it into the pack's directory, via a crude
        // substring search over the raw file (every id is stored verbatim,
        // UTF-8, in the directory region -- see `pack`'s format docs).
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("layouts");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for (_, name) in LAYOUTS {
            for suffix in ["map", "border"] {
                let id = format!("layout/{name}/{suffix}");
                assert!(
                    bytes
                        .windows(id.len())
                        .any(|window| window == id.as_bytes()),
                    "missing pack entry id `{id}`"
                );
            }
        }
        let _ = std::fs::remove_file(report.output_path);
    }

    #[test]
    fn fonts_list_has_no_duplicate_names_or_filenames() {
        // Pure data check -- no filesystem access -- so it runs everywhere.
        let names: Vec<_> = FONTS.iter().map(|(name, _)| *name).collect();
        let filenames: Vec<_> = FONTS.iter().map(|(_, filename)| *filename).collect();
        let unique_names: std::collections::HashSet<_> = names.iter().collect();
        let unique_filenames: std::collections::HashSet<_> = filenames.iter().collect();
        assert_eq!(names.len(), unique_names.len(), "duplicate font name");
        assert_eq!(
            filenames.len(),
            unique_filenames.len(),
            "duplicate filename"
        );
        for name in &names {
            // Pack ids are ASCII lowercase + digits + underscores + `/` only
            // (see `crate::extract`'s "Asset id scheme" docs).
            assert!(name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        }
        for filename in &filenames {
            assert!(filename.starts_with("latin_"));
            assert!(std::path::Path::new(filename)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png")));
        }
    }

    #[test]
    fn text_window_manifest_rejects_each_missing_required_file() {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-text-window-manifest-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let required: Vec<_> = TEXT_WINDOW_IMAGE_STEMS
            .iter()
            .map(|stem| format!("{stem}.png"))
            .chain(
                TEXT_WINDOW_PALETTE_STEMS
                    .iter()
                    .map(|stem| format!("{stem}.pal")),
            )
            .collect();
        for filename in &required {
            std::fs::write(dir.join(filename), []).unwrap();
        }

        for filename in &required {
            let missing = dir.join(filename);
            std::fs::remove_file(&missing).unwrap();
            let err = validate_text_window_manifest(&dir).unwrap_err();
            assert!(
                matches!(err, ExtractError::MissingTextWindowAsset(path) if path == missing),
                "wrong error for missing `{filename}`"
            );
            std::fs::write(&missing, []).unwrap();
        }

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn text_window_manifest_rejects_unexpected_assets() {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-text-window-unexpected-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        for stem in TEXT_WINDOW_IMAGE_STEMS {
            std::fs::write(dir.join(format!("{stem}.png")), []).unwrap();
        }
        for stem in TEXT_WINDOW_PALETTE_STEMS {
            std::fs::write(dir.join(format!("{stem}.pal")), []).unwrap();
        }

        for filename in ["new_frame.png", "text_pal5.pal"] {
            let unexpected = dir.join(filename);
            std::fs::write(&unexpected, []).unwrap();
            let err = validate_text_window_manifest(&dir).unwrap_err();
            assert!(
                matches!(err, ExtractError::UnexpectedTextWindowAsset(path) if path == unexpected),
                "wrong error for unexpected text-window asset `{filename}`"
            );
            std::fs::remove_file(unexpected).unwrap();
        }

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn text_window_palettes_require_exactly_sixteen_colors() {
        let path = std::path::Path::new("graphics/text_window/example.png");
        let color = super::jasc_pal::Rgb888 { r: 0, g: 0, b: 0 };

        for actual in [0, 1, 15, 17] {
            let colors = vec![color; actual];
            let mut writer = super::pack::PackWriter::new();
            let err = push_text_window_palette_entry(
                path,
                &colors,
                "text-window/palette/example".to_owned(),
                &mut writer,
            )
            .unwrap_err();
            assert!(
                matches!(
                    err,
                    ExtractError::TextWindowPaletteWrongColorCount(error_path, count)
                        if error_path == path && count == actual
                ),
                "wrong error for {actual}-colour text-window palette"
            );
            assert_eq!(writer.len(), 0, "invalid palette must not be serialized");
        }

        let colors = vec![color; TEXT_WINDOW_PALETTE_COLORS];
        let mut writer = super::pack::PackWriter::new();
        push_text_window_palette_entry(
            path,
            &colors,
            "text-window/palette/example".to_owned(),
            &mut writer,
        )
        .unwrap();
        assert_eq!(writer.len(), 1);
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn font_glyph_sheets_are_extracted() {
        // Same crude substring-search strategy as `layout_grids_are_extracted`
        // above (no pack reader lives in this crate -- see its comment).
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("fonts");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for (name, _) in FONTS {
            let id = format!("font/{name}/glyphs");
            assert!(
                bytes
                    .windows(id.len())
                    .any(|window| window == id.as_bytes()),
                "missing pack entry id `{id}`"
            );
        }
        let _ = std::fs::remove_file(report.output_path);
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn text_window_frames_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("text-window");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();

        let mut expected_ids: Vec<String> = Vec::new();
        for n in 1..=20 {
            expected_ids.push(format!("text-window/image/{n}"));
            expected_ids.push(format!("text-window/palette/{n}"));
        }
        expected_ids.push("text-window/image/message_box".to_owned());
        expected_ids.push("text-window/palette/message_box".to_owned());
        for n in 1..=4 {
            expected_ids.push(format!("text-window/palette/text_pal{n}"));
        }

        for id in expected_ids {
            assert!(
                bytes
                    .windows(id.len())
                    .any(|window| window == id.as_bytes()),
                "missing pack entry id `{id}`"
            );
        }
        let _ = std::fs::remove_file(report.output_path);
    }
}
