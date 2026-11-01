//! Checkout extraction of the trainer battlers over synthetic checkouts, and
//! once against a real one.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use assets::{AssetPack, BattlerDirection, PackError, TrainerPicId};
use pack_format::PackWriter;

use super::{extract_trainer_battlers, TRAINERS};
use crate::extract::png::tests::indexed_png_from_raw_with_plte;
use crate::extract::trainer_pic::extract_trainer_pics;
use crate::extract::{extract_to, png, repo_root, upstream_present, ExtractError};

const FRAME_ROWS: u32 = 64;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-trainer-battler-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn palette_text(colors: usize, first: u8) -> String {
    let mut text = format!("JASC-PAL\r\n0100\r\n{colors}\r\n");
    for i in 0..colors {
        let channel = if i == 0 { first } else { 0 };
        let _ = write!(text, "{channel} 0 0\r\n");
    }
    text
}

/// A 64-wide PNG whose every 64-row frame holds the pixel the closure names.
fn png_of(height: u32, bit_depth: u8, pixel_of_frame: impl Fn(u32) -> u8) -> Vec<u8> {
    let row_bytes = (64 * usize::from(bit_depth)).div_ceil(8);
    let mut raw = Vec::new();
    for row in 0..height {
        raw.push(0);
        let pixel = pixel_of_frame(row / FRAME_ROWS);
        raw.resize(raw.len() + row_bytes, pixel);
    }
    let plte = vec![7; 16 * 3];
    indexed_png_from_raw_with_plte(bit_depth, 64, height, &raw, &plte)
}

/// A pixel byte (two equal nibbles) per trainer, direction and frame.
fn front_pixel(trainer: usize) -> u8 {
    0x11 * (u8::try_from(trainer).unwrap() * 5 + 1)
}

fn back_pixel(trainer: usize, frame: u32) -> u8 {
    0x11 * (u8::try_from(trainer).unwrap() * 5 + 2 + u8::try_from(frame).unwrap())
}

fn write_at(dir: &Path, file: &str, bytes: impl AsRef<[u8]>) {
    std::fs::write(dir.join("graphics/trainers").join(file), bytes).unwrap();
}

/// A complete synthetic checkout whose trainers and frames all differ.
fn write_checkout(dir: &Path) {
    let root = dir.join("graphics/trainers");
    for sub in ["front_pics", "back_pics", "palettes"] {
        std::fs::create_dir_all(root.join(sub)).unwrap();
    }
    for (index, trainer) in TRAINERS.iter().enumerate() {
        write_at(dir, trainer.pic.png, png_of(64, 4, |_| front_pixel(index)));
        write_at(
            dir,
            trainer.back_png,
            png_of(256, 4, |frame| back_pixel(index, frame)),
        );
        let first = u8::try_from(index).unwrap() + 1;
        write_at(dir, trainer.pic.palette, palette_text(16, 8 * first));
    }
}

fn extracted(dir: &Path) -> (AssetPack, PathBuf) {
    let mut writer = PackWriter::new();
    extract_trainer_battlers(dir, &mut writer).unwrap();
    assert_eq!(writer.len(), 6);
    let path = dir.join("t.pack");
    std::fs::write(&path, writer.finish().unwrap()).unwrap();
    (AssetPack::load(&path).unwrap(), path)
}

#[test]
fn the_manifest_names_two_trainers_with_six_unique_ids_and_their_files() {
    let ids: Vec<&str> = TRAINERS
        .iter()
        .flat_map(|t| [t.front_id, t.back_id, t.palette_id])
        .collect();
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 6);
    assert!(ids.iter().all(|id| id.starts_with("trainer/")));
    let files: Vec<_> = TRAINERS
        .iter()
        .map(|t| (t.pic.png, t.back_png, t.pic.palette))
        .collect();
    assert_eq!(
        files,
        [
            (
                "front_pics/brendan.png",
                "back_pics/brendan.png",
                "palettes/brendan.pal"
            ),
            (
                "front_pics/may.png",
                "back_pics/may.png",
                "palettes/may.pal"
            ),
        ]
    );
    let trainers: Vec<_> = TRAINERS.iter().map(|t| t.pic.trainer_pic).collect();
    assert_eq!(trainers, [TrainerPicId::BRENDAN, TrainerPicId::MAY]);
}

#[test]
fn a_complete_checkout_extracts_four_direction_pairs_that_resolve() {
    let dir = scratch_dir("complete");
    write_checkout(&dir);
    let (pack, _) = extracted(&dir);
    for (index, trainer) in TRAINERS.iter().enumerate() {
        let id = trainer.pic.trainer_pic;
        let front = pack.trainer_battler(id, BattlerDirection::Front).unwrap();
        let back = pack.trainer_battler(id, BattlerDirection::Back).unwrap();
        assert_eq!((front.art.image.width, front.art.image.height), (64, 64));
        assert_eq!((back.art.image.width, back.art.image.height), (64, 256));
        assert_eq!(
            front.art.palette.colors().collect::<Vec<_>>(),
            back.art.palette.colors().collect::<Vec<_>>()
        );
        assert!(front
            .art
            .image
            .pixels
            .iter()
            .all(|&p| p == front_pixel(index) & 0xF));
        for (frame, rows) in back.art.image.pixels.chunks(64 * 64).enumerate() {
            let expected = back_pixel(index, u32::try_from(frame).unwrap()) & 0xF;
            assert!(rows.iter().all(|&p| p == expected), "frame {frame}");
        }
    }
    let brendan = pack
        .trainer_battler(TrainerPicId::BRENDAN, BattlerDirection::Front)
        .unwrap();
    let may = pack
        .trainer_battler(TrainerPicId::MAY, BattlerDirection::Front)
        .unwrap();
    assert_ne!(
        brendan.art.palette.colors().collect::<Vec<_>>(),
        may.art.palette.colors().collect::<Vec<_>>()
    );
    assert!(matches!(
        pack.trainer_battler(TrainerPicId::HIKER, BattlerDirection::Back),
        Err(PackError::UnsupportedTrainerBattler(TrainerPicId::HIKER))
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_battler_entries_repeat_the_portrait_art_under_their_own_ids() {
    let dir = scratch_dir("with-portraits");
    write_checkout(&dir);
    let mut writer = PackWriter::new();
    extract_trainer_pics(&dir, &mut writer).unwrap();
    extract_trainer_battlers(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), 10);
    let path = dir.join("t.pack");
    std::fs::write(&path, writer.finish().unwrap()).unwrap();
    let pack = AssetPack::load(&path).unwrap();
    for id in [TrainerPicId::BRENDAN, TrainerPicId::MAY] {
        let portrait = pack.trainer_pic(id).unwrap().art;
        let front = pack
            .trainer_battler(id, BattlerDirection::Front)
            .unwrap()
            .art;
        assert_eq!(portrait.image.pixels, front.image.pixels);
        assert_eq!(
            portrait.palette.colors().collect::<Vec<_>>(),
            front.palette.colors().collect::<Vec<_>>()
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_sources_fail_instead_of_being_skipped() {
    let dir = scratch_dir("missing");
    let mut writer = PackWriter::new();
    let err = extract_trainer_battlers(&dir, &mut writer).unwrap_err();
    assert!(matches!(err, ExtractError::ReadFailed(path, _) if path.starts_with(&dir)));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_back_sheet_names_its_path() {
    let dir = scratch_dir("missing-back");
    write_checkout(&dir);
    let back = dir.join("graphics/trainers/back_pics/may.png");
    std::fs::remove_file(&back).unwrap();
    let mut writer = PackWriter::new();
    let err = extract_trainer_battlers(&dir, &mut writer).unwrap_err();
    assert!(matches!(err, ExtractError::ReadFailed(path, _) if path == back));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sources_of_the_wrong_shape_are_rejected_without_queuing_anything() {
    let cases: [(&str, &str, Vec<u8>); 6] = [
        (
            "back as a single frame",
            "back_pics/may.png",
            png_of(64, 4, |_| 0),
        ),
        (
            "back of two frames",
            "back_pics/may.png",
            png_of(128, 4, |_| 0),
        ),
        (
            "back at eight bits",
            "back_pics/may.png",
            png_of(256, 8, |_| 0),
        ),
        (
            "front as a sheet",
            "front_pics/may.png",
            png_of(256, 4, |_| 0),
        ),
        (
            "front at two bits",
            "front_pics/may.png",
            png_of(64, 2, |_| 0),
        ),
        (
            "palette of fifteen colours",
            "palettes/may.pal",
            palette_text(15, 1).into_bytes(),
        ),
    ];
    for (name, file, bytes) in cases {
        let dir = scratch_dir(&name.replace(' ', "-"));
        write_checkout(&dir);
        write_at(&dir, file, bytes);
        let mut writer = PackWriter::new();
        let err = extract_trainer_battlers(&dir, &mut writer).unwrap_err();
        let expected = dir.join("graphics/trainers").join(file);
        assert!(
            matches!(&err, ExtractError::TrainerPicAssetMismatch { path, .. } if *path == expected),
            "{name}: {err:?}"
        );
        // Brendan validated first, yet nothing was queued.
        assert_eq!(writer.len(), 0, "{name}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn sources_that_are_not_png_are_rejected() {
    let dir = scratch_dir("bad-png");
    write_checkout(&dir);
    write_at(&dir, "back_pics/brendan.png", b"not a png");
    let path = dir.join("graphics/trainers/back_pics/brendan.png");
    let mut writer = PackWriter::new();
    let err = extract_trainer_battlers(&dir, &mut writer).unwrap_err();
    assert!(matches!(err, ExtractError::Png(p, _) if p == path));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn trainer_battlers_are_extracted_and_resolve_through_the_typed_lookup() {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-trainer-battler-{}.pack",
        std::process::id()
    ));
    extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let upstream = repo_root().join("pokeemerald/graphics/trainers");

    for trainer in TRAINERS {
        for (direction, file, height) in [
            (BattlerDirection::Front, trainer.pic.png, 64),
            (BattlerDirection::Back, trainer.back_png, 256),
        ] {
            let art = pack
                .trainer_battler(trainer.pic.trainer_pic, direction)
                .unwrap()
                .art;
            let decoded = png::decode(&std::fs::read(upstream.join(file)).unwrap()).unwrap();
            assert_eq!(art.image.pixels, decoded.pixels.as_slice(), "{file}");
            assert_eq!((art.image.width, art.image.height), (64, height));
            assert_eq!(art.palette.color_count, 16);
        }
    }
    let _ = std::fs::remove_file(path);
}
