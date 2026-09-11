//! Builds the developer asset pack from a local `pokeemerald/` checkout.
//!
//! [`run`] publishes the deterministic pack at the repository's
//! [`OUTPUT_RELATIVE_PATH`]. The runtime pack resolver is intentionally not
//! used because this command owns the checkout-local build product.
//!
//! The facade owns pipeline order, checkout and output paths, common pack-entry
//! builders, and manifests for tilesets, layouts, title graphics, sprites, and
//! interface palettes. Specialized child modules own source decoding and the
//! remaining asset manifests; `error` defines the public failure contracts.
//!
//! # Extraction scope
//!
//! The pack contains:
//!
//! - every tile image and animation, all 16 palettes, and both raw metatile
//!   tables for the primary `general` and `building` tilesets and the secondary
//!   `petalburg`, `brendans_mays_house`, and `lab` tilesets;
//! - every title-screen PNG, palette, and raw tilemap, with embedded PNG
//!   palettes for `emerald_version` and `press_start` and the upstream colour
//!   limit applied to `pokemon_logo`;
//! - every player and NPC PNG, the `brendan` and `may` palettes, and the four
//!   generic `npc_1` through `npc_4` palettes;
//! - the map and border grids named by `LAYOUTS`;
//! - the five Latin font sheets, every text-window image and palette, and the
//!   main-menu background palette; and
//! - `mus_title`, its transitive voicegroup tree, and every sample that tree
//!   references.
//!
//! # Asset id scheme
//!
//! - `tileset/<name>/{tiles,metatiles,metatile-attributes}`
//! - `tileset/<name>/anim/<animation>/<frame>` and
//!   `tileset/<name>/palette/<NN>`
//! - `title/{image,palette,raw}/<name>`
//! - `sprite/<relative-path>` and `sprite/palette/<name>`
//! - `layout/<name>/{map,border}`
//! - `font/<name>/glyphs`
//! - `text-window/{image,palette}/<stem>`
//! - `interface/palette/main_menu_bg`
//! - `audio/sample/{direct-sound,programmable-wave}/<name>`
//! - `audio/voicegroup/<label>` and `audio/song/<name>`
//!
//! Names preserve normalized upstream directory, file, or symbol names; they
//! never use linker-style `gTileset_*` identifiers.

mod audio_samples;
mod error;
mod fonts;
pub mod inflate;
pub mod jasc_pal;
mod layouts_json;
mod midi;
pub mod png;
mod text_window;
pub(crate) mod voicegroups;
mod wav;

use std::path::{Path, PathBuf};

pub use error::ExtractError;
pub use pack_format::OUTPUT_RELATIVE_PATH;
use pack_format::{EntryShapeError, PackEntry, PackWriter};

/// Serializes tests that read or replace the checkout-local pack.
#[cfg(test)]
pub(crate) static REAL_PACK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Summary of a published asset pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractReport {
    /// Number of pack entries.
    pub entry_count: usize,
    /// Serialized pack size in bytes.
    pub pack_size: u64,
    /// Published pack path.
    pub output_path: PathBuf,
}

/// Returns the repository root independently of the process working directory.
pub(crate) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/xtask is always two levels under the repo root")
        .to_path_buf()
}

/// Returns whether the local reference checkout is available for tests.
#[cfg(test)]
#[must_use]
pub(crate) fn upstream_present() -> bool {
    repo_root().join("pokeemerald/graphics").is_dir()
}

/// Extracts and publishes the checkout-local asset pack.
///
/// # Errors
///
/// Returns [`ExtractError`] when the checkout is missing, a source is invalid
/// or unreadable, or the pack cannot be built or published.
pub fn run() -> Result<ExtractReport, ExtractError> {
    extract_to(&repo_root().join(OUTPUT_RELATIVE_PATH))
}

fn extract_to(output_path: &Path) -> Result<ExtractReport, ExtractError> {
    let upstream = repo_root().join("pokeemerald");
    if !upstream.join("graphics").is_dir() {
        return Err(ExtractError::MissingUpstreamCheckout(upstream));
    }

    let mut writer = PackWriter::new();

    for tileset in TILESETS {
        extract_tileset(&upstream, tileset, &mut writer)?;
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
    write_pack_atomically(output_path, &bytes)?;

    Ok(ExtractReport {
        entry_count,
        pack_size,
        output_path: output_path.to_path_buf(),
    })
}

fn write_pack_atomically(output_path: &Path, bytes: &[u8]) -> Result<(), ExtractError> {
    let staging_path = staging_path_for_current_process(output_path);
    let write_failed = |error: std::io::Error| {
        ExtractError::WriteFailed(output_path.to_path_buf(), error.to_string())
    };

    if let Err(error) = write_and_sync(&staging_path, bytes) {
        return Err(write_failed(remove_abandoned_staging_file(
            &staging_path,
            error,
        )));
    }
    if let Err(error) = std::fs::rename(&staging_path, output_path) {
        return Err(write_failed(remove_abandoned_staging_file(
            &staging_path,
            error,
        )));
    }
    Ok(())
}

fn remove_abandoned_staging_file(
    staging_path: &Path,
    original_error: std::io::Error,
) -> std::io::Error {
    match std::fs::remove_file(staging_path) {
        Ok(()) => original_error,
        Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => original_error,
        Err(cleanup_error) => std::io::Error::new(
            original_error.kind(),
            format!(
                "{original_error} (additionally, failed to remove abandoned staging file `{}`: {cleanup_error})",
                staging_path.display()
            ),
        ),
    }
}

fn staging_path_for_current_process(output_path: &Path) -> PathBuf {
    let mut name = output_path.as_os_str().to_os_string();
    name.push(format!(".tmp.{}", std::process::id()));
    PathBuf::from(name)
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;

    let file = std::fs::File::create(path)?;
    let mut staged = std::io::BufWriter::new(file);
    staged.write_all(bytes)?;
    staged.flush()?;
    staged.get_ref().sync_all()
}

#[derive(Clone, Copy)]
struct TilesetSource {
    category: &'static str,
    name: &'static str,
}

const TILESETS: [TilesetSource; 5] = [
    TilesetSource {
        category: "primary",
        name: "general",
    },
    TilesetSource {
        category: "primary",
        name: "building",
    },
    TilesetSource {
        category: "secondary",
        name: "petalburg",
    },
    TilesetSource {
        category: "secondary",
        name: "brendans_mays_house",
    },
    TilesetSource {
        category: "secondary",
        name: "lab",
    },
];
const TILESET_PALETTE_COUNT: u8 = 16;

fn read_file(path: &Path) -> Result<Vec<u8>, ExtractError> {
    std::fs::read(path).map_err(|e| ExtractError::ReadFailed(path.to_path_buf(), e.to_string()))
}

fn read_text(path: &Path) -> Result<String, ExtractError> {
    std::fs::read_to_string(path)
        .map_err(|e| ExtractError::ReadFailed(path.to_path_buf(), e.to_string()))
}

fn push_png_entry(path: &Path, id: String, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let bytes = read_file(path)?;
    let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.to_path_buf(), e))?;
    writer.push(build_image_entry(path, id, image)?);
    Ok(())
}

fn build_image_entry(
    path: &Path,
    id: String,
    image: png::IndexedImage,
) -> Result<PackEntry, ExtractError> {
    pack_format::image_entry(id, image.width, image.height, image.bit_depth, image.pixels)
        .map_err(|e| ExtractError::EntryShape(path.to_path_buf(), e))
}

fn push_palette_entry(
    path: &Path,
    id: String,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    push_palette_entry_with_limit(path, id, None, writer)
}

fn push_palette_entry_with_limit(
    path: &Path,
    id: String,
    color_limit: Option<usize>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let text = read_text(path)?;
    let mut colors =
        jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.to_path_buf(), e))?;
    if let Some(color_limit) = color_limit {
        if colors.len() < color_limit {
            return Err(ExtractError::PaletteShorterThanCut {
                path: path.to_path_buf(),
                cut: color_limit,
                actual: colors.len(),
            });
        }
        colors.truncate(color_limit);
    }
    writer.push(build_palette_entry(path, &colors, id)?);
    Ok(())
}

fn build_palette_entry(
    path: &Path,
    colors: &[jasc_pal::Rgb888],
    id: String,
) -> Result<PackEntry, ExtractError> {
    let colors: Vec<u16> = colors.iter().map(|c| c.to_gba555()).collect();
    pack_format::palette_entry(id, &colors).map_err(|e| match e {
        EntryShapeError::PaletteColorCountUnrepresentable(actual) => {
            ExtractError::PaletteColorCountUnrepresentable(path.to_path_buf(), actual)
        }
        other => ExtractError::EntryShape(path.to_path_buf(), other),
    })
}

fn push_raw_entry(path: &Path, id: String, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let payload = read_file(path)?;
    writer.push(pack_format::raw_entry(id, payload));
    Ok(())
}

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

fn extract_tileset(
    upstream: &Path,
    source: TilesetSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let base = upstream
        .join("data/tilesets")
        .join(source.category)
        .join(source.name);

    push_png_entry(
        &base.join("tiles.png"),
        format!("tileset/{}/tiles", source.name),
        writer,
    )?;

    let anim_dir = base.join("anim");
    if anim_dir.is_dir() {
        for png_path in collect_pngs_sorted(&anim_dir)? {
            let rel = png_path
                .strip_prefix(&anim_dir)
                .expect("collect_pngs_sorted only returns paths under anim_dir")
                .with_extension("");
            let rel_id = rel.to_string_lossy().replace('\\', "/");
            push_png_entry(
                &png_path,
                format!("tileset/{}/anim/{rel_id}", source.name),
                writer,
            )?;
        }
    }

    for slot in 0..TILESET_PALETTE_COUNT {
        let pal_path = base.join("palettes").join(format!("{slot:02}.pal"));
        push_palette_entry(
            &pal_path,
            format!("tileset/{}/palette/{slot:02}", source.name),
            writer,
        )?;
    }

    push_raw_entry(
        &base.join("metatiles.bin"),
        format!("tileset/{}/metatiles", source.name),
        writer,
    )?;
    push_raw_entry(
        &base.join("metatile_attributes.bin"),
        format!("tileset/{}/metatile-attributes", source.name),
        writer,
    )?;

    Ok(())
}

// `src/graphics.c`'s `gTitleScreenEmeraldVersionPal` and
// `gTitleScreenPressStartPal` declarations build these palettes from each
// PNG's embedded `PLTE` chunk instead of a sibling `.pal` file.
const TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS: [&str; 2] = ["emerald_version", "press_start"];

// `graphics_file_rules.mk` builds `pokemon_logo.gbapal` with this limit.
const TITLE_SCREEN_PALETTE_CUTS: [(&str, usize); 1] = [("pokemon_logo", 224)];

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
                    let colors: Vec<jasc_pal::Rgb888> = image
                        .palette
                        .iter()
                        .map(|&[r, g, b]| jasc_pal::Rgb888 { r, g, b })
                        .collect();
                    let entry =
                        build_palette_entry(&path, &colors, format!("title/palette/{stem}"))?;
                    writer.push(entry);
                }
                writer.push(build_image_entry(
                    &path,
                    format!("title/image/{stem}"),
                    image,
                )?);
            }
            Some("pal") => {
                let color_limit = TITLE_SCREEN_PALETTE_CUTS
                    .iter()
                    .find_map(|&(sheet, limit)| (sheet == stem).then_some(limit));
                push_palette_entry_with_limit(
                    &path,
                    format!("title/palette/{stem}"),
                    color_limit,
                    writer,
                )?;
            }
            Some("bin") => push_raw_entry(&path, format!("title/raw/{stem}"), writer)?,
            _ => {}
        }
    }
    Ok(())
}

fn extract_interface_palettes(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    push_palette_entry(
        &upstream.join("graphics/interface/main_menu_bg.pal"),
        "interface/palette/main_menu_bg".to_owned(),
        writer,
    )
}

const FIRST_GENERIC_NPC_PALETTE: u8 = 1;
const LAST_GENERIC_NPC_PALETTE: u8 = 4;

fn extract_sprites(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let people_dir = upstream.join("graphics/object_events/pics/people");
    for png_path in collect_pngs_sorted(&people_dir)? {
        let rel = png_path
            .strip_prefix(&people_dir)
            .expect("collect_pngs_sorted only returns paths under people_dir")
            .with_extension("");
        let rel_id = rel.to_string_lossy().replace('\\', "/");
        push_png_entry(&png_path, format!("sprite/{rel_id}"), writer)?;
    }

    let palettes_dir = upstream.join("graphics/object_events/palettes");
    for who in ["brendan", "may"] {
        push_palette_entry(
            &palettes_dir.join(format!("{who}.pal")),
            format!("sprite/palette/{who}"),
            writer,
        )?;
    }
    for n in FIRST_GENERIC_NPC_PALETTE..=LAST_GENERIC_NPC_PALETTE {
        let name = format!("npc_{n}");
        push_palette_entry(
            &palettes_dir.join(format!("{name}.pal")),
            format!("sprite/palette/{name}"),
            writer,
        )?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct LayoutSource {
    upstream_id: &'static str,
    pack_name: &'static str,
}

const LAYOUTS: [LayoutSource; 10] = [
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN",
        pack_name: "littleroot_town",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        pack_name: "littleroot_town_brendans_house_1f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        pack_name: "littleroot_town_brendans_house_2f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        pack_name: "littleroot_town_mays_house_1f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        pack_name: "littleroot_town_mays_house_2f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        pack_name: "littleroot_town_professor_birchs_lab",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
        pack_name: "littleroot_town_professor_birchs_lab_with_table",
    },
    LayoutSource {
        upstream_id: "LAYOUT_ROUTE101",
        pack_name: "route101",
    },
    LayoutSource {
        upstream_id: "LAYOUT_OLDALE_TOWN",
        pack_name: "oldale_town",
    },
    LayoutSource {
        upstream_id: "LAYOUT_ROUTE103",
        pack_name: "route103",
    },
];

const BORDER_SIDE_CELLS: usize = 2;
const BORDER_BLOCK_BYTES: usize =
    BORDER_SIDE_CELLS * BORDER_SIDE_CELLS * std::mem::size_of::<u16>();

fn extract_layouts(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let json_path = upstream.join("data/layouts/layouts.json");
    let text = read_text(&json_path)?;
    let entries =
        layouts_json::parse(&text).map_err(|e| ExtractError::LayoutsJson(json_path.clone(), e))?;

    for layout in LAYOUTS {
        let entry = entries
            .iter()
            .find(|entry| entry.id == layout.upstream_id)
            .ok_or(ExtractError::UnknownLayoutInJson(
                json_path.clone(),
                layout.upstream_id,
            ))?;

        let map_bytes = read_file(&upstream.join(&entry.blockdata_filepath))?;
        let width = usize::try_from(entry.width).unwrap_or(usize::MAX);
        let height = usize::try_from(entry.height).unwrap_or(usize::MAX);
        let expected_min = width
            .saturating_mul(height)
            .saturating_mul(std::mem::size_of::<u16>());
        if map_bytes.len() < expected_min {
            return Err(ExtractError::LayoutGridTooShort {
                layout_id: layout.upstream_id,
                expected: expected_min,
                actual: map_bytes.len(),
            });
        }
        writer.push(pack_format::raw_entry(
            format!("layout/{}/map", layout.pack_name),
            map_bytes,
        ));

        let border_bytes = read_file(&upstream.join(&entry.border_filepath))?;
        if border_bytes.len() != BORDER_BLOCK_BYTES {
            return Err(ExtractError::LayoutBorderWrongSize {
                layout_id: layout.upstream_id,
                actual: border_bytes.len(),
            });
        }
        writer.push(pack_format::raw_entry(
            format!("layout/{}/border", layout.pack_name),
            border_bytes,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        collect_pngs_sorted, extract_to, staging_path_for_current_process, upstream_present,
        write_pack_atomically, ExtractError, LAYOUTS,
    };

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    // Changes to `LAYOUTS` must also update `object_event_flags.rs`'s
    // `BUNDLED_LAYOUTS`, `overworld/npc.rs`'s `EXTRACTED_MAPS`,
    // `overworld/mod.rs`'s `resolve_tileset_pack_name`, and
    // `overworld_phase/decoration_tests.rs`'s `BUNDLED_LAYOUTS`.
    #[test]
    fn the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it() {
        let ids: Vec<&str> = LAYOUTS.iter().map(|layout| layout.upstream_id).collect();
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
            "update the cross-crate LAYOUTS mirrors with this manifest"
        );
    }

    fn assert_pack_contains_entry_id(bytes: &[u8], id: &str) {
        assert!(
            bytes
                .windows(id.len())
                .any(|window| window == id.as_bytes()),
            "missing pack entry id `{id}`"
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
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("embedded-palettes");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for name in super::TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS {
            let id = format!("title/palette/{name}");
            assert_pack_contains_entry_id(&bytes, &id);
        }
        let _ = std::fs::remove_file(report.output_path);
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn main_menu_bg_palette_gets_an_interface_palette_entry() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("interface-palette");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        let id = "interface/palette/main_menu_bg";
        assert_pack_contains_entry_id(&bytes, id);
        let _ = std::fs::remove_file(report.output_path);
    }

    #[test]
    fn missing_upstream_checkout_message_points_to_init_sh() {
        let err = ExtractError::MissingUpstreamCheckout(std::path::PathBuf::from("/nowhere"));
        let rendered = err.to_string();
        assert!(rendered.contains("init.sh"));
        assert!(rendered.contains("cargo xtask extract"));
    }

    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-atomic-write-test-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creating scratch dir");
        dir
    }

    #[test]
    fn a_failed_staging_write_leaves_the_existing_pack_untouched() {
        let dir = scratch_dir("write");
        let output_path = dir.join("pokeemerald.pack");
        let original = b"an existing, usable pack";
        std::fs::write(&output_path, original).unwrap();

        let staging_path = staging_path_for_current_process(&output_path);
        std::fs::create_dir(&staging_path).unwrap();

        let err = write_pack_atomically(&output_path, b"a truncated replacement").unwrap_err();
        assert!(
            matches!(&err, ExtractError::WriteFailed(path, _) if path == &output_path),
            "{err:?}"
        );
        assert_eq!(
            std::fs::read(&output_path).unwrap(),
            original,
            "a failed staging write destroyed the pack that was already on disk"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_rename_removes_the_staging_file_and_leaves_the_destination_untouched() {
        let dir = scratch_dir("rename");
        let output_path = dir.join("destination");
        std::fs::create_dir(&output_path).unwrap();
        let marker_path = output_path.join("marker");
        std::fs::write(&marker_path, b"unchanged").unwrap();

        let staging_path = staging_path_for_current_process(&output_path);
        let err = write_pack_atomically(&output_path, b"replacement").unwrap_err();
        assert!(
            matches!(&err, ExtractError::WriteFailed(path, _) if path == &output_path),
            "{err:?}"
        );
        assert!(
            !staging_path.exists(),
            "a failed rename must not leave its staging file behind"
        );
        assert_eq!(std::fs::read(&marker_path).unwrap(), b"unchanged");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_staging_file_that_was_never_created_is_not_reported_as_abandoned() {
        let dir = scratch_dir("never-created");
        let output_path = dir.join("absent").join("pokeemerald.pack");
        let staging_path = staging_path_for_current_process(&output_path);

        let err = write_pack_atomically(&output_path, b"replacement").unwrap_err();

        assert!(
            !staging_path.exists(),
            "no staging file should exist after a failed create"
        );
        assert!(
            !err.to_string().contains("abandoned staging file"),
            "nothing was staged, so nothing was abandoned: {err}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_staging_cleanup_names_the_artifact_it_left_behind() {
        let dir = scratch_dir("cleanup");
        let output_path = dir.join("pokeemerald.pack");
        let staging_path = staging_path_for_current_process(&output_path);
        std::fs::create_dir(&staging_path).unwrap();
        std::fs::write(staging_path.join("occupant"), b"occupant").unwrap();

        let err = write_pack_atomically(&output_path, b"replacement").unwrap_err();

        assert!(
            staging_path.is_dir(),
            "cleanup should have failed, leaving the staging directory behind"
        );
        assert!(
            err.to_string()
                .contains(&staging_path.display().to_string()),
            "a failed cleanup must name the artifact it left behind: {err}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn oversized_jasc_palette_is_rejected_not_truncated() {
        let count = usize::from(u16::MAX) + 1;
        let mut text = String::from("JASC-PAL\r\n0100\r\n");
        text.push_str(&count.to_string());
        text.push_str("\r\n");
        text.push_str(&"0 0 0\r\n".repeat(count));

        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-oversized-jasc-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("oversized.pal");
        std::fs::write(&path, &text).unwrap();

        let mut writer = pack_format::PackWriter::new();
        let err =
            super::push_palette_entry(&path, "test/palette".to_owned(), &mut writer).unwrap_err();
        assert!(
            matches!(
                &err,
                ExtractError::PaletteColorCountUnrepresentable(error_path, actual)
                    if error_path == &path && *actual == count
            ),
            "wrong error for a {count}-colour JASC palette: {err:?}"
        );
        assert_eq!(writer.len(), 0, "oversized palette must not be serialized");

        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn collect_pngs_sorted_rejects_missing_dir() {
        let err = collect_pngs_sorted(std::path::Path::new("/does/not/exist")).unwrap_err();
        assert!(matches!(err, super::ExtractError::ReadFailed(..)));
    }

    #[test]
    fn layouts_list_has_no_duplicate_ids_or_names() {
        let ids: Vec<_> = LAYOUTS.iter().map(|layout| layout.upstream_id).collect();
        let names: Vec<_> = LAYOUTS.iter().map(|layout| layout.pack_name).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        let unique_names: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(ids.len(), unique_ids.len(), "duplicate LAYOUT_* id");
        assert_eq!(names.len(), unique_names.len(), "duplicate pack name");
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
            assert!(name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn layout_grids_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("layouts");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for layout in LAYOUTS {
            for suffix in ["map", "border"] {
                let id = format!("layout/{}/{suffix}", layout.pack_name);
                assert_pack_contains_entry_id(&bytes, &id);
            }
        }
        let _ = std::fs::remove_file(report.output_path);
    }
}
