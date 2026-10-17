//! Extracts the fixed battle textbox and menu text manifest (upstream
//! `graphics/battle_interface/`).
//!
//! Six source files become five records, the ones `LoadBattleTextboxAndBackground`
//! and `LoadBattleMenuWindowGfx` select. The textbox palette is `textbox_0.pal`
//! followed by `textbox_1.pal`, as `graphics_file_rules.mk` concatenates their
//! `.gbapal` files. A missing file or a neighbour the typed
//! `assets::BattleInterfaceHandle` never reads cannot change the pack.

use std::path::Path;

use super::{build_image_entry, build_palette_entry, jasc_pal, png, push_raw_bytes};
use super::{read_file, read_text, ExtractError};
use pack_format::PackWriter;

const DIRECTORY: &str = "graphics/battle_interface";
const IMAGE_PREFIX: &str = "battle-interface/image";
const PALETTE_PREFIX: &str = "battle-interface/palette";
const RAW_PREFIX: &str = "battle-interface/raw";

const TEXTBOX_SIZE: u32 = 128;
const TEXTBOX_BIT_DEPTH: u8 = 4;
/// Colours in one 4bpp bank, the size of every source palette.
const BANK_COLORS: usize = 16;
/// `textbox_map.bin`: a 64x32 tilemap of 2-byte entries.
const TEXTBOX_TILEMAP_BYTES: usize = 64 * 32 * 2;

/// A pack palette built by concatenating JASC files in order.
#[derive(Clone, Copy, Debug)]
struct PaletteSource {
    name: &'static str,
    files: &'static [&'static str],
}

const PALETTES: [PaletteSource; 3] = [
    PaletteSource {
        name: "textbox",
        files: &["textbox_0.pal", "textbox_1.pal"],
    },
    PaletteSource {
        name: "window_text",
        files: &["text.pal"],
    },
    PaletteSource {
        name: "pp_text",
        files: &["text_pp.pal"],
    },
];

/// Extracts the battle interface manifest: the textbox sheet, three palettes,
/// and the textbox tilemap.
pub(super) fn extract_battle_interface(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    extract_textbox_image(upstream, writer)?;
    for source in PALETTES {
        extract_palette(upstream, source, writer)?;
    }
    extract_tilemap(upstream, &mut read_file, writer)
}

fn extract_textbox_image(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let path = upstream.join(DIRECTORY).join("textbox.png");
    let bytes = read_file(&path)?;
    let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.bit_depth != TEXTBOX_BIT_DEPTH {
        return Err(mismatch(
            &path,
            format!(
                "is {}-bit: expected {TEXTBOX_BIT_DEPTH}-bit",
                image.bit_depth
            ),
        ));
    }
    if image.width != TEXTBOX_SIZE || image.height != TEXTBOX_SIZE {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {TEXTBOX_SIZE}x{TEXTBOX_SIZE}",
                image.width, image.height
            ),
        ));
    }
    writer.push(build_image_entry(
        &path,
        format!("{IMAGE_PREFIX}/textbox"),
        image,
    )?);
    Ok(())
}

fn extract_palette(
    upstream: &Path,
    source: PaletteSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let mut colors = Vec::with_capacity(source.files.len() * BANK_COLORS);
    let mut first_path = None;
    for file in source.files {
        let path = upstream.join(DIRECTORY).join(file);
        let text = read_text(&path)?;
        let bank = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.clone(), e))?;
        if bank.len() != BANK_COLORS {
            return Err(mismatch(
                &path,
                format!("has {} colours: expected {BANK_COLORS}", bank.len()),
            ));
        }
        colors.extend(bank);
        first_path.get_or_insert(path);
    }
    let path = first_path.expect("every palette names at least one file");
    writer.push(build_palette_entry(
        &path,
        &colors,
        format!("{PALETTE_PREFIX}/{}", source.name),
    )?);
    Ok(())
}

/// The buffer validated is the buffer queued: a second read of the path
/// could publish bytes that changed after the length check (#2105).
fn extract_tilemap(
    upstream: &Path,
    read: &mut dyn FnMut(&Path) -> Result<Vec<u8>, ExtractError>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream.join(DIRECTORY).join("textbox_map.bin");
    let payload = read(&path)?;
    if payload.len() != TEXTBOX_TILEMAP_BYTES {
        return Err(mismatch(
            &path,
            format!(
                "is {} bytes: expected {TEXTBOX_TILEMAP_BYTES}",
                payload.len()
            ),
        ));
    }
    push_raw_bytes(format!("{RAW_PREFIX}/textbox_map"), payload, writer);
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::BattleInterfaceAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use assets::AssetPack;

    use super::super::png::tests::indexed_png_from_raw_with_plte;
    use super::super::{extract_to, repo_root, upstream_present};
    use super::{
        extract_battle_interface, extract_palette, extract_textbox_image, extract_tilemap,
        DIRECTORY, PALETTES, TEXTBOX_TILEMAP_BYTES,
    };
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    const MANIFEST_IDS: [&str; 5] = [
        "battle-interface/image/textbox",
        "battle-interface/palette/textbox",
        "battle-interface/palette/window_text",
        "battle-interface/palette/pp_text",
        "battle-interface/raw/textbox_map",
    ];
    const SOURCE_FILES: [&str; 6] = [
        "textbox.png",
        "textbox_0.pal",
        "textbox_1.pal",
        "text.pal",
        "text_pp.pal",
        "textbox_map.bin",
    ];

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-battle-interface-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(DIRECTORY)).unwrap();
        dir
    }

    fn png_bytes(bit_depth: u8, width: u32, height: u32) -> Vec<u8> {
        let row = if bit_depth == 4 {
            vec![0x11; width as usize / 2]
        } else {
            vec![1; width as usize]
        };
        let mut raw = Vec::new();
        for _ in 0..height {
            raw.push(0);
            raw.extend_from_slice(&row);
        }
        indexed_png_from_raw_with_plte(bit_depth, width, height, &raw, &[7; 16 * 3])
    }

    /// A JASC palette of `colors` entries, each channel `channel`.
    fn jasc(colors: usize, channel: u8) -> String {
        let mut text = format!("JASC-PAL\r\n0100\r\n{colors}\r\n");
        text.push_str(&format!("{channel} {channel} {channel}\r\n").repeat(colors));
        text
    }

    fn write(dir: &Path, file: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = dir.join(DIRECTORY).join(file);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn write_valid_checkout(dir: &Path) {
        write(dir, "textbox.png", png_bytes(4, 128, 128));
        write(dir, "textbox_0.pal", jasc(16, 8));
        write(dir, "textbox_1.pal", jasc(16, 248));
        write(dir, "text.pal", jasc(16, 16));
        write(dir, "text_pp.pal", jasc(16, 24));
        write(dir, "textbox_map.bin", vec![0x5A; TEXTBOX_TILEMAP_BYTES]);
    }

    fn mismatch_path(err: ExtractError) -> PathBuf {
        match err {
            ExtractError::BattleInterfaceAssetMismatch { path, .. } => path,
            other => panic!("expected a battle-interface mismatch, got {other}"),
        }
    }

    #[test]
    fn manifest_names_five_unique_records_from_six_unique_files() {
        assert_eq!(MANIFEST_IDS.iter().collect::<BTreeSet<_>>().len(), 5);
        assert_eq!(SOURCE_FILES.iter().collect::<BTreeSet<_>>().len(), 6);
        let palettes: Vec<_> = PALETTES.iter().map(|p| (p.name, p.files)).collect();
        assert_eq!(
            palettes,
            [
                ("textbox", &["textbox_0.pal", "textbox_1.pal"][..]),
                ("window_text", &["text.pal"][..]),
                ("pp_text", &["text_pp.pal"][..]),
            ]
        );
        assert_eq!(TEXTBOX_TILEMAP_BYTES, 4096);
    }

    #[test]
    fn a_valid_manifest_yields_the_records_the_typed_handle_reads() {
        let dir = scratch_dir("valid");
        write_valid_checkout(&dir);
        let mut writer = PackWriter::new();
        extract_battle_interface(&dir, &mut writer).unwrap();
        assert_eq!(writer.len(), MANIFEST_IDS.len());
        let pack_path = dir.join("battle-interface.pack");
        std::fs::write(&pack_path, writer.finish().unwrap()).unwrap();

        let pack = AssetPack::load(&pack_path).unwrap();
        let handle = pack.battle_interface().unwrap();
        // `textbox_0.pal` fills the low bank and `textbox_1.pal` the high bank.
        assert_eq!(handle.textbox.palette.color_count, 32);
        assert_eq!(handle.textbox.palette.color(0), Some(0x0421));
        assert_eq!(handle.textbox.palette.color(15), Some(0x0421));
        assert_eq!(handle.textbox.palette.color(16), Some(0x7FFF));
        assert_eq!(handle.textbox.palette.color(31), Some(0x7FFF));
        assert_eq!(handle.textbox.tilemap.len(), TEXTBOX_TILEMAP_BYTES);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_file_in_a_complete_checkout_fails_closed() {
        for file in SOURCE_FILES {
            let dir = scratch_dir("missing-one");
            write_valid_checkout(&dir);
            std::fs::remove_file(dir.join(DIRECTORY).join(file)).unwrap();
            let mut writer = PackWriter::new();
            let err = extract_battle_interface(&dir, &mut writer).unwrap_err();
            assert!(
                matches!(&err, ExtractError::ReadFailed(path, _) if path.ends_with(file)),
                "{file}: {err}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_textbox_that_is_not_the_upstream_sheet_is_rejected() {
        let dir = scratch_dir("textbox");
        let mut writer = PackWriter::new();

        let path = write(&dir, "textbox.png", b"not a png");
        let err = extract_textbox_image(&dir, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Png(error_path, _) if error_path == path));

        write(&dir, "textbox.png", png_bytes(4, 64, 64));
        let err = extract_textbox_image(&dir, &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path);

        write(&dir, "textbox.png", png_bytes(8, 128, 128));
        let err = extract_textbox_image(&dir, &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path);

        assert_eq!(writer.len(), 0);
        write(&dir, "textbox.png", png_bytes(4, 128, 128));
        extract_textbox_image(&dir, &mut writer).unwrap();
        assert_eq!(writer.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn palette_files_that_are_malformed_or_miscounted_are_rejected() {
        let dir = scratch_dir("palette");
        let mut writer = PackWriter::new();
        let path = write(&dir, "textbox_1.pal", "not a palette");
        write(&dir, "textbox_0.pal", jasc(16, 8));
        let err = extract_palette(&dir, PALETTES[0], &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Pal(error_path, _) if error_path == path));

        for colors in [15, 17, 32] {
            write(&dir, "textbox_1.pal", jasc(colors, 8));
            assert_eq!(
                mismatch_path(extract_palette(&dir, PALETTES[0], &mut writer).unwrap_err()),
                path,
                "{colors} colours"
            );
        }
        assert_eq!(writer.len(), 0);

        write(&dir, "textbox_1.pal", jasc(16, 8));
        extract_palette(&dir, PALETTES[0], &mut writer).unwrap();
        assert_eq!(writer.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tilemaps_of_the_wrong_length_are_rejected() {
        let dir = scratch_dir("tilemap-length");
        let path = dir.join(DIRECTORY).join("textbox_map.bin");
        let mut writer = PackWriter::new();
        for len in [0, TEXTBOX_TILEMAP_BYTES - 1, TEXTBOX_TILEMAP_BYTES + 1] {
            std::fs::write(&path, vec![0; len]).unwrap();
            let err =
                extract_tilemap(&dir, &mut crate::extract::read_file, &mut writer).unwrap_err();
            assert_eq!(mismatch_path(err), path, "{len} bytes");
        }
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tilemap_pack_entry_holds_the_validated_bytes() {
        let expected: Vec<u8> = (0..=u8::MAX).cycle().take(TEXTBOX_TILEMAP_BYTES).collect();
        let mut reads = 0;
        let mut read = |_: &Path| {
            reads += 1;
            // Any read after the first sees a different, wrong-length file.
            Ok(if reads == 1 {
                expected.clone()
            } else {
                vec![0xFF; TEXTBOX_TILEMAP_BYTES - 1]
            })
        };
        let mut writer = PackWriter::new();
        extract_tilemap(Path::new("upstream"), &mut read, &mut writer).unwrap();
        assert_eq!(reads, 1);
        let pack = writer.finish().unwrap();
        let entries = pack_format::parse_directory(&pack).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "battle-interface/raw/textbox_map");
        assert_eq!(
            &pack[entries[0].offset..entries[0].offset + entries[0].length],
            expected.as_slice()
        );
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn battle_interface_is_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-battle-interface-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();
        let handle = pack.battle_interface().unwrap();

        let upstream = repo_root().join("pokeemerald").join(DIRECTORY);
        assert_eq!(
            handle.textbox.tilemap,
            std::fs::read(upstream.join("textbox_map.bin")).unwrap()
        );
        let bank = |file: &str| -> Vec<u16> {
            let text = std::fs::read_to_string(upstream.join(file)).unwrap();
            super::jasc_pal::parse(&text)
                .unwrap()
                .iter()
                .map(|c| c.to_gba555())
                .collect()
        };
        let composed: Vec<u16> = [bank("textbox_0.pal"), bank("textbox_1.pal")].concat();
        assert_eq!(
            handle.textbox.palette.colors().collect::<Vec<_>>(),
            composed
        );
        assert_eq!(
            handle.window_text_palette.colors().collect::<Vec<_>>(),
            bank("text.pal")
        );
        assert_eq!(
            handle.pp_text_palette.colors().collect::<Vec<_>>(),
            bank("text_pp.pal")
        );
        let _ = std::fs::remove_file(path);
    }
}
