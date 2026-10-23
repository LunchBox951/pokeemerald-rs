//! Builds the developer asset pack from a local `pokeemerald/` checkout.
//!
//! [`run`] publishes the deterministic pack at the repository's
//! [`OUTPUT_RELATIVE_PATH`]. The runtime pack resolver is intentionally not
//! used because this command owns the checkout-local build product.
//!
//! The facade owns pipeline order, checkout and output paths, and the
//! pack-entry helpers shared by several extractors. Each child module owns one
//! asset family's decoding and entries, `scope` owns the manifests naming them,
//! and `error` defines the public failure contracts.
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
//! - the starter chooser's tile, Poké Ball, and circle sheets with their
//!   palettes and both Birch tilemaps;
//! - Birch's speech scene (shadow sheet, background tilemap and palettes, his
//!   sprite, and Lotad's front picture with its normal palette) and the wall
//!   clock (face and hand sheets, both gender palettes, the prompt palette, and
//!   both tilemaps);
//! - the static battler art of the starters and the checkpoint's other eleven
//!   Pokémon (`scope::BATTLERS`): front and back images with normal and shiny
//!   palettes;
//! - the Brendan and May trainer front pictures with their palettes;
//! - the ordinary bag's screen, tilemap, sex palettes, male and female sheets
//!   and palette, rotating ball, select and HM marks, and the image and
//!   palette of every item icon `gItemIconTable` selects; and
//! - the Pokédex screen's 14 sheets with their embedded palettes, six
//!   palette files, and 15 raw tilemap and data files;
//! - the battle textbox sheet, its 32-colour palette and tilemap, and the battle-window
//!   and PP text palettes;
//! - `mus_title`, its transitive voicegroup tree, and every sample that tree
//!   references; and
//! - the battle-message token table (`gBattleStringsTable` plus the special
//!   templates `BufferStringBattle` selects).
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
//! - `starter-chooser/{image,palette,raw}/<stem>`
//! - `opening-scene/{image,palette,raw}/<name>`
//! - `trainer-pic/<trainer>/{front,palette}`
//! - `bag/{image,palette,raw}/<name>` and `bag/item-icon/{image,palette}/<stem>`
//! - `battle-interface/{image,palette,raw}/<name>`
//! - `pokemon/<species>/{front,back}` and `pokemon/<species>/palette/{normal,shiny}`
//! - `pokedex-graphics/{image,image-palette,palette,raw}/<name>`
//! - `audio/sample/{direct-sound,programmable-wave}/<name>`
//! - `audio/voicegroup/<label>` and `audio/song/<name>`
//! - `battle-message/table`
//!
//! Names preserve normalized upstream directory, file, or symbol names; they
//! never use linker-style `gTileset_*` identifiers.

mod audio_samples;
mod bag;
mod battle_anim;
mod battle_interface;
mod battle_message;
mod battler;
#[cfg(test)]
mod checkpoint_maps_tests;
mod error;
mod fonts;
pub mod inflate;
mod interface;
pub mod jasc_pal;
mod layouts;
mod layouts_json;
pub(crate) mod midi;
mod opening_scene;
pub mod png;
mod pokedex_graphics;
mod publish;
pub(crate) mod scope;
mod sprites;
mod starter_chooser;
mod text_window;
mod tilesets;
mod title;
mod trainer_pic;
pub(crate) mod voicegroups;
mod wav;

use std::path::{Path, PathBuf};

pub use error::ExtractError;
pub use pack_format::OUTPUT_RELATIVE_PATH;
use pack_format::{EntryShapeError, PackEntry, PackWriter};
use publish::write_pack_atomically;

use scope::TILESETS;

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
        tilesets::extract_tileset(&upstream, tileset, &mut writer)?;
    }
    title::extract_title_screen(&upstream, &mut writer)?;
    sprites::extract_sprites(&upstream, &mut writer)?;
    trainer_pic::extract_trainer_pics(&upstream, &mut writer)?;
    layouts::extract_layouts(&upstream, &mut writer)?;
    fonts::extract_fonts(&upstream, &mut writer)?;
    text_window::extract_text_window(&upstream, &mut writer)?;
    interface::extract_interface_palettes(&upstream, &mut writer)?;
    starter_chooser::extract_starter_chooser(&upstream, &mut writer)?;
    opening_scene::extract_opening_scene(&upstream, &mut writer)?;
    battler::extract_battlers(&upstream, &mut writer)?;
    bag::extract_bag(&upstream, &mut writer)?;
    pokedex_graphics::extract_pokedex_graphics(&upstream, &mut writer)?;
    battle_interface::extract_battle_interface(&upstream, &mut writer)?;
    audio_samples::extract_audio_samples(&upstream, &mut writer)?;
    voicegroups::extract_voicegroups(&upstream, &mut writer)?;
    midi::extract_song(&upstream, &mut writer)?;
    battle_message::extract_battle_message(&upstream, &mut writer)?;
    battle_anim::extract_battle_anim(&upstream, &mut writer)?;

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
    push_raw_bytes(id, payload, writer);
    Ok(())
}

fn push_raw_bytes(id: String, payload: Vec<u8>, writer: &mut PackWriter) {
    writer.push(pack_format::raw_entry(id, payload));
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

#[cfg(test)]
mod tests {
    use super::{collect_pngs_sorted, extract_to, jasc_pal, upstream_present, ExtractError};

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
        let err = ExtractError::MissingUpstreamCheckout(std::path::PathBuf::from("/nowhere"));
        let rendered = err.to_string();
        assert!(rendered.contains("init.sh"));
        assert!(rendered.contains("cargo xtask extract"));
    }

    #[test]
    fn oversized_jasc_palette_is_rejected_not_truncated() {
        let count = usize::from(u16::MAX) + 1;
        let text = format!("JASC-PAL\r\n0100\r\n{count}\r\n");

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
                ExtractError::Pal(error_path, jasc_pal::JascPalError::TooManyColors { declared, .. })
                    if error_path == &path && *declared == count
            ),
            "wrong error for a {count}-colour JASC palette: {err:?}"
        );
        assert_eq!(writer.len(), 0, "oversized palette must not be serialized");

        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn zero_colour_jasc_palette_is_rejected_without_queuing_an_entry() {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-zero-jasc-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("zero.pal");
        std::fs::write(&path, "JASC-PAL\r\n0100\r\n0\r\n").unwrap();

        let mut writer = pack_format::PackWriter::new();
        let err =
            super::push_palette_entry(&path, "test/palette".to_owned(), &mut writer).unwrap_err();
        assert!(
            matches!(
                &err,
                ExtractError::Pal(error_path, jasc_pal::JascPalError::NoColors)
                    if error_path == &path
            ),
            "wrong error for a zero-colour JASC palette: {err:?}"
        );
        assert_eq!(writer.len(), 0, "zero-colour palette must not be queued");

        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn collect_pngs_sorted_rejects_missing_dir() {
        let err = collect_pngs_sorted(std::path::Path::new("/does/not/exist")).unwrap_err();
        assert!(matches!(err, super::ExtractError::ReadFailed(..)));
    }
}
