//! The Brendan and May trainer front pictures: the checkout side of the
//! `gTrainerFrontPicTable` and `gTrainerFrontPicPaletteTable` rows for `TRAINER_PIC_BRENDAN`
//! and `TRAINER_PIC_MAY` (`src/data/trainer_graphics/front_pic_tables.h`,
//! `src/data/graphics/trainers.h:214-218`).

use std::path::{Path, PathBuf};

use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, read_file, read_text, ExtractError,
};
use pack_format::PackWriter;

/// Width of every trainer picture, and the height of a front picture.
pub(super) const SIDE: u32 = 64;
const COLORS: usize = 16;
/// The PNG bit depth the typed accessor requires of every trainer picture.
const BIT_DEPTH: u8 = 4;

/// One trainer picture's checkout files, relative to `graphics/trainers/`,
/// and the pack ids it publishes under.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PicSource {
    pub(super) png: &'static str,
    pub(super) palette: &'static str,
    /// The lower-case trainer name; the picture and palette symbols spell it in camel case.
    pub(crate) name: &'static str,
    /// The row of `gTrainerFrontPicTable` and `gTrainerFrontPicPaletteTable` this picture fills.
    pub(crate) trainer_pic: assets::TrainerPicId,
    pub(crate) image_id: &'static str,
    pub(crate) palette_id: &'static str,
}

pub(crate) const PICS: [PicSource; 2] = [
    PicSource {
        name: "brendan",
        png: "front_pics/brendan.png",
        palette: "palettes/brendan.pal",
        trainer_pic: assets::TrainerPicId::BRENDAN,
        image_id: "trainer-pic/brendan/front",
        palette_id: "trainer-pic/brendan/palette",
    },
    PicSource {
        name: "may",
        png: "front_pics/may.png",
        palette: "palettes/may.pal",
        trainer_pic: assets::TrainerPicId::MAY,
        image_id: "trainer-pic/may/front",
        palette_id: "trainer-pic/may/palette",
    },
];

/// Extracts the Brendan and May front images and their palettes.
pub(super) fn extract_trainer_pics(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let directory = upstream.join(TRAINER_GRAPHICS);
    let mut entries = Vec::with_capacity(PICS.len() * 2);
    for pic in PICS {
        let (palette_path, colors) = read_palette(&directory, pic.palette)?;
        let (path, image) = read_picture(&directory, pic.png, SIDE)?;
        entries.push(build_image_entry(&path, pic.image_id.to_owned(), image)?);
        entries.push(build_palette_entry(
            &palette_path,
            &colors,
            pic.palette_id.to_owned(),
        )?);
    }
    // Queue only once both pictures validated, so a refusal publishes neither.
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// Where the trainer graphics sit in the checkout.
pub(super) const TRAINER_GRAPHICS: &str = "graphics/trainers";

/// Reads one 16-colour palette file of a trainer picture.
pub(super) fn read_palette(
    directory: &Path,
    file: &str,
) -> Result<(PathBuf, Vec<jasc_pal::Rgb888>), ExtractError> {
    let path = directory.join(file);
    let colors =
        jasc_pal::parse(&read_text(&path)?).map_err(|e| ExtractError::Pal(path.clone(), e))?;
    if colors.len() != COLORS {
        return Err(mismatch(
            &path,
            format!("has {} colours: expected {COLORS}", colors.len()),
        ));
    }
    Ok((path, colors))
}

/// Reads one 4bpp, 64-pixel-wide trainer picture of `height` pixels whose
/// indices all fit a 16-colour palette.
pub(super) fn read_picture(
    directory: &Path,
    file: &str,
    height: u32,
) -> Result<(PathBuf, png::IndexedImage), ExtractError> {
    let path = directory.join(file);
    let image = png::decode(&read_file(&path)?).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.width != SIDE || image.height != height {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {SIDE}x{height}",
                image.width, image.height
            ),
        ));
    }
    if image.bit_depth != BIT_DEPTH {
        return Err(mismatch(
            &path,
            format!("is {}-bit: expected {BIT_DEPTH}-bit", image.bit_depth),
        ));
    }
    if let Some(&pixel) = image
        .pixels
        .iter()
        .find(|&&pixel| usize::from(pixel) >= COLORS)
    {
        return Err(mismatch(
            &path,
            format!("has pixel index {pixel}: its palette only has {COLORS} colours"),
        ));
    }
    Ok((path, image))
}

pub(super) fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::TrainerPicAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fmt::Write as _;

    use assets::{AssetPack, PackError, TrainerPicId};

    use super::super::png::tests::indexed_png_from_raw_with_plte;
    use super::super::{extract_to, repo_root, upstream_present};
    use super::{extract_trainer_pics, PICS};
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-trainer-pic-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    type Case = (&'static str, fn(&std::path::Path), &'static str);

    fn palette_text(colors: usize, first: u8) -> String {
        let mut text = format!("JASC-PAL\r\n0100\r\n{colors}\r\n");
        for i in 0..colors {
            let channel = if i == 0 { first } else { 0 };
            let _ = write!(text, "{channel} 0 0\r\n");
        }
        text
    }

    fn pic_png(side: u32, bit_depth: u8, pixel: u8) -> Vec<u8> {
        let row_bytes = (side as usize * usize::from(bit_depth)).div_ceil(8);
        let mut raw = Vec::new();
        for _ in 0..side {
            raw.push(0);
            raw.resize(raw.len() + row_bytes, pixel);
        }
        let plte = vec![7; 16 * 3];
        indexed_png_from_raw_with_plte(bit_depth, side, side, &raw, &plte)
    }

    /// A complete synthetic checkout whose two pictures differ.
    fn write_checkout(dir: &std::path::Path) {
        let root = dir.join("graphics/trainers");
        std::fs::create_dir_all(root.join("front_pics")).unwrap();
        std::fs::create_dir_all(root.join("palettes")).unwrap();
        for (i, pic) in PICS.iter().enumerate() {
            let first = u8::try_from(i).unwrap() + 1;
            std::fs::write(root.join(pic.png), pic_png(64, 4, 0x11 * first)).unwrap();
            std::fs::write(root.join(pic.palette), palette_text(16, 8 * first)).unwrap();
        }
    }

    #[test]
    fn manifest_names_two_pairs_with_unique_ids() {
        let ids: Vec<&str> = PICS
            .iter()
            .flat_map(|pic| [pic.image_id, pic.palette_id])
            .collect();
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 4);
        let files: Vec<_> = PICS.iter().map(|pic| (pic.png, pic.palette)).collect();
        assert_eq!(
            files,
            [
                ("front_pics/brendan.png", "palettes/brendan.pal"),
                ("front_pics/may.png", "palettes/may.pal"),
            ]
        );
    }

    #[test]
    fn a_complete_checkout_extracts_exactly_two_matched_pairs() {
        let dir = scratch_dir("complete");
        write_checkout(&dir);
        let mut writer = PackWriter::new();
        extract_trainer_pics(&dir, &mut writer).unwrap();
        assert_eq!(writer.len(), 4);
        let bytes = writer.finish().unwrap();
        let pack_path = dir.join("t.pack");
        std::fs::write(&pack_path, bytes).unwrap();
        let pack = AssetPack::load(&pack_path).unwrap();

        let brendan = pack.trainer_pic(TrainerPicId::BRENDAN).unwrap();
        let may = pack.trainer_pic(TrainerPicId::MAY).unwrap();
        assert_eq!(brendan.id, TrainerPicId::BRENDAN);
        assert_eq!(may.id, TrainerPicId::MAY);
        for handle in [brendan, may] {
            assert_eq!((handle.art.image.width, handle.art.image.height), (64, 64));
            assert_eq!(handle.art.image.bit_depth, 4);
            assert_eq!(handle.art.palette.color_count, 16);
        }
        assert_ne!(brendan.art.image.pixels, may.art.image.pixels);
        assert!(!brendan.art.palette.colors().eq(may.art.palette.colors()));
        assert!(matches!(
            pack.trainer_pic(TrainerPicId::HIKER),
            Err(PackError::UnsupportedTrainerPic(TrainerPicId::HIKER))
        ));
        assert!(matches!(
            pack.trainer_pic(TrainerPicId::RS_BRENDAN),
            Err(PackError::UnsupportedTrainerPic(TrainerPicId::RS_BRENDAN))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_sources_fail_instead_of_being_skipped() {
        let dir = scratch_dir("missing");
        let mut writer = PackWriter::new();
        let err = extract_trainer_pics(&dir, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::ReadFailed(path, _) if path.starts_with(&dir)));
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sources_of_the_wrong_shape_are_rejected_without_queuing_anything() {
        let root_of = |dir: &std::path::Path| dir.join("graphics/trainers");
        let cases: [Case; 4] = [
            (
                "palette-count",
                |dir| {
                    let path = dir.join("graphics/trainers/palettes/may.pal");
                    std::fs::write(path, palette_text(15, 1)).unwrap();
                },
                "palettes/may.pal",
            ),
            (
                "dimensions",
                |dir| {
                    let path = dir.join("graphics/trainers/front_pics/may.png");
                    std::fs::write(path, pic_png(32, 4, 0)).unwrap();
                },
                "front_pics/may.png",
            ),
            (
                "bit-depth",
                |dir| {
                    let path = dir.join("graphics/trainers/front_pics/may.png");
                    std::fs::write(path, pic_png(64, 8, 0)).unwrap();
                },
                "front_pics/may.png",
            ),
            (
                "two-bit-depth",
                |dir| {
                    let path = dir.join("graphics/trainers/front_pics/may.png");
                    std::fs::write(path, pic_png(64, 2, 0)).unwrap();
                },
                "front_pics/may.png",
            ),
        ];
        for (name, corrupt, file) in cases {
            let dir = scratch_dir(name);
            write_checkout(&dir);
            corrupt(&dir);
            let mut writer = PackWriter::new();
            let err = extract_trainer_pics(&dir, &mut writer).unwrap_err();
            assert!(
                matches!(&err, ExtractError::TrainerPicAssetMismatch { path, .. }
                    if *path == root_of(&dir).join(file)),
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
        let path = dir.join("graphics/trainers/front_pics/brendan.png");
        std::fs::write(&path, b"not a png").unwrap();
        let mut writer = PackWriter::new();
        let err = extract_trainer_pics(&dir, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Png(p, _) if p == path));
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn trainer_pictures_are_extracted_and_resolve_through_the_typed_lookup() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-trainer-pic-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();
        let upstream = repo_root().join("pokeemerald/graphics/trainers");

        let mut palettes = Vec::new();
        for (id, name) in [
            (TrainerPicId::BRENDAN, "brendan"),
            (TrainerPicId::MAY, "may"),
        ] {
            let art = pack.trainer_pic(id).unwrap().art;
            let source = std::fs::read(upstream.join(format!("front_pics/{name}.png"))).unwrap();
            let decoded = super::png::decode(&source).unwrap();
            assert_eq!(art.image.pixels, decoded.pixels.as_slice(), "{name}");
            assert_eq!((art.image.width, art.image.height), (64, 64));
            assert_eq!(art.image.bit_depth, 4);
            assert_eq!(art.palette.color_count, 16);
            palettes.push(art.palette.colors().collect::<Vec<_>>());
        }
        assert_ne!(palettes[0], palettes[1]);
        let _ = std::fs::remove_file(path);
    }
}
