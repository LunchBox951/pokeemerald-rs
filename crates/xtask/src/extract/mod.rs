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
//!   ledger reason. Two of the six PNGs (`emerald_version.png`,
//!   `press_start.png` — the version banner and Press Start/copyright OBJ
//!   sprite sheets) additionally get a `title/palette/<name>` entry decoded
//!   from their own embedded `PLTE` chunk ([`png::IndexedImage::palette`]):
//!   unlike every other title-screen graphic, upstream's build derives
//!   *their* in-game palette directly from the PNG
//!   (`INCGFX_U16(...png", ".gbapal")` in `graphics.c`), not a sibling
//!   `.pal` file, so this is the only way to recover it (see
//!   [`TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS`]).
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
//!   NPC-to-palette table lookup needed, unlike the generic NPC roster).
//!   Issue #161 (NPC object-event rendering) additionally extracts the four
//!   generic `npc_1..4.pal` banks (`OBJ_EVENT_PAL_TAG_NPC_1..4`) every
//!   "standard" NPC that slice renders resolves to
//!   (`pokeemerald-rs::overworld::npc`'s own bounded graphics-id table) —
//!   the remaining ~30 palette files in that directory (per-character
//!   palettes like `truck.pal`/`vigoroth.pal`, and every `*_reflection.pal`)
//!   are still *not* extracted here, documented as a deferred item.
//!
//! - **Map layout grids** (S-4): the Littleroot Town layout family's
//!   `map.bin` / `border.bin` grid files — the town itself plus every
//!   interior it contains (both player houses' floors, Professor Birch's
//!   lab, and its rarely-referenced "with table" variant) — **plus, since
//!   issue #177, `LAYOUT_ROUTE101`**, Littleroot's own north map connection
//!   (`data/maps/LittlerootTown/map.json`'s `connections`) targets it, and
//!   it is the traversal prerequisite for I-4's first wild encounter —
//!   **and, since issue #248, `LAYOUT_OLDALE_TOWN`/`LAYOUT_ROUTE103`**:
//!   Route 101's own north connection targets Oldale Town, and Oldale's own
//!   north connection targets Route 103 in turn, completing the
//!   Littleroot ↔ Route 101 ↔ Oldale Town ↔ Route 103 chain I-5's Route 103
//!   rival battle needs to be reachable on foot (see [`LAYOUTS`]). Resolved
//!   via `data/layouts/layouts.json` ([`layouts_json`]) rather than
//!   hardcoded upstream paths, and extracted as opaque [`pack::PackKind::Raw`]
//!   blobs — same rationale as `metatiles.bin` / `metatile_attributes.bin`
//!   above: these are upstream's own flat binary files, not compiled from
//!   another source. `crates/assets::map_layouts` already ships the typed
//!   decode layer (`MetatileCell`, `LayoutGrid`, `BorderGrid`) that reads
//!   these bytes back out of the pack; this pipeline only needs to get the
//!   bytes *into* the pack. Both new layouts share Littleroot/Route 101's
//!   own `gTileset_General`+`gTileset_Petalburg` pair (already extracted,
//!   above), so no sixth tileset is needed.
//!
//! - **Fonts** (S-4, issue #114): the five upstream Latin glyph sheets —
//!   see [`fonts`]'s module docs for the sheet shape and what is
//!   deliberately not extracted.
//!
//! - **Text window frames** (S-4, issue #114): every file under
//!   `graphics/text_window/` — see [`text_window`]'s module docs for the
//!   manifest and the image/palette pairing validation.
//!
//! - **Main menu background palette** (I-3, issue #216): just
//!   `graphics/interface/main_menu_bg.pal`, as `interface/palette/main_menu_bg`
//!   — the one real per-pixel colour `pokeemerald-rs::main_menu::MainMenuScene`
//!   needs (its content-fill/border/glyph colours are upstream's own
//!   runtime-patched `LoadPalette` literals, not sourced from any file — see
//!   that module's docs). The sibling `main_menu_text.pal` is deliberately
//!   *not* extracted yet: every colour that no-save slice draws from bank 15
//!   is one of those same hardcoded runtime patches, and the file's own
//!   colours (indices `0..=9`) belong to the `CONTINUE`/Mystery Gift/Mystery
//!   Events variants this issue's scope notes explicitly exclude.
//!
//! - **Audio samples** (S-4, issue #183, `#115` child 4): every
//!   `DirectSound` instrument sample and CGB programmable-wave table
//!   `mus_title`'s voicegroup references, transitively through its
//!   key-split sub-groups — see [`audio_samples`]'s module docs for exactly
//!   how that set was traced and for the wire format each entry's payload
//!   carries (`crates/assets::audio::sample::Sample`'s encoding, duplicated
//!   here per this module's crate-decoupling policy — see [`pack`]'s
//!   docs). [`wav`] is the `.wav`-source decoder this needs; see its module
//!   docs for the field-by-field derivation, cited into
//!   `tools/wav2agb`.
//! - **`MUS_TITLE`'s voicegroup dependency tree** (S-4, issue #182, `#115`
//!   child 3): `MUS_TITLE`'s own voicegroup (`sound/voicegroups/title.inc`)
//!   plus every key-split/rhythm child it transitively references (the
//!   drum kit `sound/voicegroups/drumsets/rs.inc` and the five instrument
//!   key-splits under `sound/voicegroups/keysplits/`), normalized to
//!   `crates/assets/src/audio/voicegroup.rs`'s backend-neutral schema and
//!   emitted as `audio/voicegroup/<label>` raw entries — see
//!   [`voicegroups`]'s module docs for the resolver, the 128-slot
//!   normalization, and why the other ~188 `.inc` files under
//!   `sound/voicegroups/` stay `pending`. Voicegroup entries carry only
//!   stable `audio/sample/direct-sound/<name>` /
//!   `audio/sample/programmable-wave/<nn>` ids, matching
//!   [`audio_samples`]'s scheme verbatim; that pass writes the entries
//!   those ids name.
//! - **`MUS_TITLE`'s own MIDI semantics** (S-4, issue #181, `#115` child 2):
//!   `sound/songs/midi/mus_title.mid`, compiled the way
//!   `tools/mid2agb`/`midi.cfg`'s own `-E -R50 -G_title -V090` flags
//!   interpret it, into `crates/assets::audio::song::SongEvent` streams (one
//!   per playable MIDI channel) and emitted as a single
//!   `audio/song/mus_title` raw entry — see [`midi`]'s module docs for the
//!   compiler pipeline and every deliberate scope cut. References
//!   `voicegroup_title`'s `audio/voicegroup/title` id, matching
//!   [`voicegroups`]'s scheme verbatim; that pass writes the entry that id
//!   names. Every other song under `sound/songs/midi/` (530 total) stays
//!   `pending`.
//!
//! Explicitly **not** extracted (deferred to future slices, not silently
//! dropped): metatile-to-tile mapping beyond the raw `metatiles.bin` bytes
//! (no decode/typed access yet — that's a rendering-layer concern once
//! `crates/rendering` needs it), the *full* `object_event_graphics_info`
//! palette-tag indirection (only the four generic `npc_1..4` banks above are
//! extracted; every per-character palette it also names — `truck.pal`,
//! `vigoroth.pal`, the `*_reflection.pal` set — stays unextracted), any tileset outside the
//! five above (every other `data/tilesets/*` directory stays `pending` in
//! the ledger), any map layout outside the Littleroot Town family above
//! (every other `data/layouts/*` directory likewise stays `pending`),
//! every non-Latin font sheet under `graphics/fonts/` (see above), every
//! voicegroup outside `MUS_TITLE`'s own dependency tree (the other ~188
//! `.inc` files under `sound/voicegroups/` stay `pending` — see
//! [`voicegroups`]'s module docs), every sample payload a voicegroup
//! entry's `audio/sample/*` ids reference (`#183`'s job), and every song
//! under `sound/songs/midi/` other than `mus_title.mid` (529 of the 530
//! `.mid` sources stay `pending` — see [`midi`]'s module docs for exactly
//! which of `mus_title.mid`'s own MIDI features are modelled and which are
//! deliberately not, independent of the other songs).
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
//! - `interface/palette/main_menu_bg` — see the main menu bullet above.
//! - `audio/sample/direct-sound/<basename>`, `audio/sample/programmable-wave/<NN>`
//!   — see [`audio_samples`]'s module docs for the exact derivation and why
//!   the two live under separate sub-namespaces.
//! - `audio/voicegroup/<label>` (e.g. `audio/voicegroup/title`,
//!   `audio/voicegroup/rs_drumset`) — `<label>` is the upstream
//!   `voice_group`/`voicegroup_*` symbol name verbatim (see [`voicegroups`]'s
//!   module docs). A [`voicegroups`] entry's own payload additionally
//!   references the `audio/sample/direct-sound/<name>` /
//!   `audio/sample/programmable-wave/<nn>` ids above, mirroring
//!   [`audio_samples`]'s scheme verbatim.
//! - `audio/song/<name>` (currently only `audio/song/mus_title`) —
//!   `<name>` is the upstream `.mid` source's filename stem; see [`midi`]'s
//!   module docs. The entry's payload references its voicegroup by the
//!   `audio/voicegroup/<label>` id above.
//!
//! `<name>` is always a normalized, stable identifier (upstream's own
//! directory/file naming, which is already `snake_case` and stable across
//! decomp revisions) — never a `gTileset_*`-style linker symbol. See
//! [`pack`]'s module docs for why that matters.

mod audio_samples;
mod error;
mod fonts;
pub mod inflate;
pub mod jasc_pal;
mod layouts_json;
mod midi;
pub mod pack;
pub mod png;
mod text_window;
mod voicegroups;
mod wav;

use std::path::{Path, PathBuf};

pub use error::ExtractError;
use pack::{PackEntry, PackKind, PackWriter};

/// The pack's location, relative to the repository root: a top-level,
/// gitignored directory (mirroring how `pokeemerald/`/`mgba/` are also
/// top-level gitignored reference dirs) rather than something under
/// `target/`, so it survives `cargo clean`.
pub const OUTPUT_RELATIVE_PATH: &str = "assets-pack/pokeemerald.pack";

/// Serializes the ignored tests that touch the one real, developer-local
/// pack at [`OUTPUT_RELATIVE_PATH`]: `extract` rewrites it non-atomically
/// while `record_snapshot`'s real-pack round-trip reads it, and the test
/// harness runs ignored tests in parallel by default. Every ignored test
/// that reads or writes that path must hold this lock for its whole body.
#[cfg(test)]
pub(crate) static REAL_PACK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
///
/// `pub(crate)`, not private: `crate::record_snapshot` (F-3, V-4) reuses it
/// to find the same default pack path this module writes
/// ([`OUTPUT_RELATIVE_PATH`]) rather than re-deriving it.
pub(crate) fn repo_root() -> PathBuf {
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
    fonts::extract_fonts(&upstream, &mut writer)?;
    text_window::extract_text_window(&upstream, &mut writer)?;
    extract_interface_palettes(&upstream, &mut writer)?;
    audio_samples::extract_audio_samples(&upstream, &mut writer)?;
    voicegroups::extract_voicegroups(&upstream, &mut writer)?;
    midi::extract_song(&upstream, &mut writer)?;

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
/// PNG's own `PLTE` chunk — see [`text_window`]) into a
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

/// Title-screen OBJ sprite sheets whose in-game palette upstream's build
/// derives directly from the PNG's own embedded colour table
/// (`INCGFX_U16("graphics/title_screen/<name>.png", ".gbapal")` in
/// `graphics.c`), rather than a sibling `.pal` file the way every other
/// title-screen graphic works — see `sVersionBannerLeftSpriteTemplate` /
/// `sSpritePalette_PressStart` in `title_screen.c`. Extracted as
/// `title/palette/<name>` alongside the normal `title/image/<name>` entry
/// (see [`extract_title_screen`]).
const TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS: [&str; 2] = ["emerald_version", "press_start"];

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
            Some("png") => {
                let bytes = read_file(&path)?;
                let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
                if TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS.contains(&stem.as_str()) {
                    if image.palette.is_empty() {
                        return Err(ExtractError::MissingEmbeddedPalette(path.clone()));
                    }
                    let mut payload = Vec::with_capacity(image.palette.len() * 2);
                    for &[r, g, b] in &image.palette {
                        let color = jasc_pal::Rgb888 { r, g, b };
                        payload.extend_from_slice(&color.to_gba555().to_le_bytes());
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    writer.push(PackEntry {
                        id: format!("title/palette/{stem}"),
                        kind: PackKind::Palette {
                            color_count: image.palette.len() as u16,
                        },
                        payload,
                    });
                }
                writer.push(PackEntry {
                    id: format!("title/image/{stem}"),
                    kind: PackKind::Image {
                        width: image.width,
                        height: image.height,
                        bit_depth: image.bit_depth,
                    },
                    payload: image.pixels,
                });
            }
            Some("pal") => decode_palette_entry(&path, format!("title/palette/{stem}"), writer)?,
            Some("bin") => raw_entry(&path, format!("title/raw/{stem}"), writer)?,
            _ => {}
        }
    }
    Ok(())
}

/// Extract the one interface palette the no-save main menu needs (module
/// docs' main-menu bullet): `graphics/interface/main_menu_bg.pal`, as
/// `interface/palette/main_menu_bg`.
fn extract_interface_palettes(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    decode_palette_entry(
        &upstream.join("graphics/interface/main_menu_bg.pal"),
        "interface/palette/main_menu_bg".to_owned(),
        writer,
    )
}

/// Extract every player/NPC sprite sheet plus the player and generic-NPC
/// palettes.
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
    // The four generic NPC palette banks (`OBJ_EVENT_PAL_TAG_NPC_1..4`,
    // issue #161's own NPC object-event rendering slice): every "standard"
    // 16x32 NPC this port renders a sprite for (Mom, the twin, Professor
    // Birch, ...) draws from one of these four, resolved by
    // `pokeemerald-rs::overworld::npc`'s own graphics-id table. The
    // remaining ~30 palette files in this directory (per-character
    // palettes like `truck.pal`/`vigoroth.pal`, and every `*_reflection.pal`)
    // stay unextracted -- no graphics id this port renders resolves to one
    // yet (see that module's own "not drawn" scope list).
    for n in 1..=4 {
        let name = format!("npc_{n}");
        decode_palette_entry(
            &palettes_dir.join(format!("{name}.pal")),
            format!("sprite/palette/{name}"),
            writer,
        )?;
    }
    Ok(())
}

/// `(LAYOUT_* id, normalized pack name)` — the Littleroot Town layout
/// family this pipeline extracts (the town itself plus every interior it
/// contains), **plus `LAYOUT_ROUTE101`** (issue #177) **and, since issue
/// #248, `LAYOUT_OLDALE_TOWN`/`LAYOUT_ROUTE103`**: Littleroot's own north
/// map connection targets Route 101, Route 101's own north connection
/// targets Oldale Town, and Oldale's own north connection targets Route
/// 103 in turn — walking that whole chain is the traversal prerequisite
/// for I-4's first wild encounter and I-5's Route 103 rival battle. See
/// the module docs for why this list, not the full `data/layouts/` tree,
/// is in scope.
///
/// Every id here is looked up in `layouts.json` at extract time
/// ([`extract_layouts`]) rather than the directory name being derived
/// mechanically from it, so a typo here surfaces as a clear
/// [`ExtractError::UnknownLayoutInJson`] instead of a silently-wrong path.
const LAYOUTS: [(&str, &str); 10] = [
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
    // Not part of the Littleroot Town directory family (its own top-level
    // `data/layouts/Route101/`), bundled anyway: Littleroot's north
    // connection (`data/maps/LittlerootTown/map.json:15-21`) targets it, and
    // Route101's own south connection (`data/maps/Route101/map.json`)
    // targets Littleroot right back -- the one connection pair the early
    // playable slice needs to cross.
    ("LAYOUT_ROUTE101", "route101"),
    // Not part of the Littleroot Town directory family either (its own
    // top-level `data/layouts/OldaleTown/` and `data/layouts/Route103/`),
    // bundled for issue #248 (I-5): Route 101's own north connection
    // (`data/maps/Route101/map.json`) targets Oldale Town, and Oldale's own
    // north connection (`data/maps/OldaleTown/map.json`) targets Route 103
    // in turn -- the second and third links of the same walking chain
    // `LAYOUT_ROUTE101` above is the first link of.
    ("LAYOUT_OLDALE_TOWN", "oldale_town"),
    ("LAYOUT_ROUTE103", "route103"),
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

#[cfg(test)]
mod tests {
    use super::{collect_pngs_sorted, extract_to, upstream_present, ExtractError, LAYOUTS};

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

    /// Tripwire for every *bounded* table elsewhere in the workspace whose
    /// scope is "whatever [`LAYOUTS`] bundles" -- those tables can only be
    /// derived from this list, and `xtask` is std-only (no `assets`
    /// dependency) so they cannot import it. Adding a layout here must
    /// therefore fail *loudly*, right next to the change, with the list of
    /// what else has to grow with it:
    ///
    /// - `crates/assets/src/object_event_flags.rs`'s `OBJECT_EVENT_FLAGS`
    ///   (every `ObjectEvent::flag` name reachable from a bundled map must
    ///   resolve) and its own `BUNDLED_LAYOUTS` mirror, which derives the
    ///   reachable map set from this one.
    /// - `crates/pokeemerald-rs/src/overworld/npc.rs`'s
    ///   `resolve_sprite_source` (its reachable-graphics-id test asserts the
    ///   exact drawn/not-drawn partition over the same maps).
    /// - `crates/pokeemerald-rs/src/overworld/mod.rs`'s
    ///   `resolve_tileset_pack_name` (a new layout may name a sixth
    ///   tileset).
    #[test]
    fn the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it() {
        let ids: Vec<&str> = LAYOUTS.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            ids,
            [
                "LAYOUT_LITTLEROOT_TOWN",
                "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
                "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
                "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
                "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
                "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
                "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
                "LAYOUT_ROUTE101",
                "LAYOUT_OLDALE_TOWN",
                "LAYOUT_ROUTE103",
            ],
            "this list feeds bounded tables in other crates -- see this \
             test's own doc comment for what must be extended alongside it"
        );
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
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn embedded_palette_sheets_get_a_title_palette_entry() {
        // The version banner / press-start sprite sheets have no sibling
        // `.pal` file (module docs) -- confirm `title/palette/emerald_version`
        // and `title/palette/press_start` made it into the pack, the same
        // crude substring search `layout_grids_are_extracted` uses (no pack
        // reader lives in this crate).
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("embedded-palettes");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for name in super::TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS {
            let id = format!("title/palette/{name}");
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
    fn main_menu_bg_palette_gets_an_interface_palette_entry() {
        // Same crude substring search `embedded_palette_sheets_get_a_title_palette_entry`
        // uses (no pack reader lives in this crate) -- confirms issue #216's
        // new `extract_interface_palettes` pass actually reached the pack.
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("interface-palette");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        let id = "interface/palette/main_menu_bg";
        assert!(
            bytes
                .windows(id.len())
                .any(|window| window == id.as_bytes()),
            "missing pack entry id `{id}`"
        );
        let _ = std::fs::remove_file(report.output_path);
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
        // Every entry is either the Littleroot Town directory family or one
        // of the three connection targets outside that family this
        // pipeline bundles: `LAYOUT_ROUTE101` (issue #177),
        // `LAYOUT_OLDALE_TOWN`/`LAYOUT_ROUTE103` (issue #248) (module docs).
        for id in &ids {
            assert!(
                id.starts_with("LAYOUT_LITTLEROOT_TOWN")
                    || *id == "LAYOUT_ROUTE101"
                    || *id == "LAYOUT_OLDALE_TOWN"
                    || *id == "LAYOUT_ROUTE103"
            );
        }
        for name in &names {
            assert!(
                name.starts_with("littleroot_town")
                    || *name == "route101"
                    || *name == "oldale_town"
                    || *name == "route103"
            );
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
}
