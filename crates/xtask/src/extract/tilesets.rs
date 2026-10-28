//! Extracts tile sheets, animations, palettes, and metatile tables for the
//! pinned tilesets.

use std::path::Path;

use super::scope::TilesetSource;
use super::{push_palette_entry, push_png_entry, push_raw_entry, ExtractError};
use pack_format::PackWriter;

const TILESET_PALETTE_COUNT: u8 = 16;

/// Refuse a checkout missing any pinned animation frame PNG.
fn require_animation_sources(anim_dir: &Path, source: TilesetSource) -> Result<(), ExtractError> {
    for &(anim, frames) in source.animations {
        for n in 0..frames {
            let path = anim_dir.join(anim).join(format!("{n}.png"));
            if !path.is_file() {
                return Err(ExtractError::ReadFailed(
                    path,
                    "required tileset animation frame is missing".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn extract_tileset(
    upstream: &Path,
    source: TilesetSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let base = upstream
        .join("data/tilesets")
        .join(source.category)
        .join(source.name);

    push_png_entry(&base.join("tiles.png"), source.tiles_id(), writer)?;

    let anim_dir = base.join("anim");
    require_animation_sources(&anim_dir, source)?;
    for &(anim, frames) in source.animations {
        for n in 0..frames {
            push_png_entry(
                &anim_dir.join(anim).join(format!("{n}.png")),
                format!("tileset/{}/anim/{anim}/{n}", source.name),
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

#[cfg(test)]
mod tests {
    use super::{extract_tileset, ExtractError, PackWriter, TILESET_PALETTE_COUNT};
    use crate::extract::scope::{TilesetSource, TILESETS};

    /// A synthetic checkout holding every source `extract_tileset` reads for
    /// `source`, each decodable.
    fn synthetic_tileset_checkout(tag: &str, source: TilesetSource) -> std::path::PathBuf {
        let upstream = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&upstream);
        let base = upstream
            .join("data/tilesets")
            .join(source.category)
            .join(source.name);
        let png = crate::extract::png::tests::tiny_indexed_png(8, 1, 1, &[0]);
        std::fs::create_dir_all(base.join("palettes")).expect("dir");
        std::fs::write(base.join("tiles.png"), &png).expect("tiles");
        for slot in 0..TILESET_PALETTE_COUNT {
            let pal = base.join("palettes").join(format!("{slot:02}.pal"));
            std::fs::write(pal, "JASC-PAL\r\n0100\r\n1\r\n0 0 0\r\n").expect("pal");
        }
        std::fs::write(base.join("metatiles.bin"), [0u8; 16]).expect("metatiles");
        std::fs::write(base.join("metatile_attributes.bin"), [0u8; 2]).expect("attrs");
        for &(name, frames) in source.animations {
            let dir = base.join("anim").join(name);
            std::fs::create_dir_all(&dir).expect("dir");
            for n in 0..frames {
                std::fs::write(dir.join(format!("{n}.png")), &png).expect("frame");
            }
        }
        upstream
    }

    #[test]
    fn untracked_animation_pngs_are_not_packed_and_ids_match_the_manifest() {
        for source in [TILESETS[0], TILESETS[1]] {
            assert!(!source.animations.is_empty(), "{}", source.name);
            let upstream = synthetic_tileset_checkout("untracked-anim", source);
            let anim = upstream
                .join("data/tilesets")
                .join(source.category)
                .join(source.name)
                .join("anim");
            let first = source.animations[0].0;
            // Neither decodable nor in the manifest: a manifest-driven
            // extractor never opens them, so a directory walk would fail.
            std::fs::write(anim.join("zz_untracked.png"), b"not a png").expect("stray");
            std::fs::write(anim.join(first).join("99.png"), b"not a png").expect("stray");
            std::fs::write(anim.join(first).join("zz.PNG"), b"not a png").expect("stray");
            std::fs::create_dir_all(anim.join("zz_dir")).expect("dir");
            std::fs::write(anim.join("zz_dir/0.png"), b"not a png").expect("stray");

            let mut writer = PackWriter::new();
            let result = extract_tileset(&upstream, source, &mut writer);
            let _ = std::fs::remove_dir_all(&upstream);
            result.expect("complete");
            let bytes = writer.finish().expect("finish");
            let ids: Vec<String> = pack_format::parse_directory(&bytes)
                .expect("pack parses")
                .into_iter()
                .map(|entry| entry.id)
                .collect();

            let name = source.name;
            let mut expected = vec![
                source.tiles_id(),
                format!("tileset/{name}/metatiles"),
                format!("tileset/{name}/metatile-attributes"),
            ];
            expected.extend(source.animation_ids());
            expected.extend(
                (0..TILESET_PALETTE_COUNT).map(|n| format!("tileset/{name}/palette/{n:02}")),
            );
            expected.sort();
            assert_eq!(ids, expected, "{name}");
        }
    }

    #[test]
    fn a_checkout_missing_a_required_animation_frame_is_refused() {
        let general = TILESETS[0];
        let upstream = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-missing-anim-frame-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&upstream);
        let base = upstream
            .join("data/tilesets")
            .join(general.category)
            .join(general.name);
        let anim = base.join("anim");
        // Every source `extract_tileset` reads, each decodable, so only the
        // required-frame check can refuse the checkout.
        let png = crate::extract::png::tests::tiny_indexed_png(8, 1, 1, &[0]);
        std::fs::create_dir_all(base.join("palettes")).expect("dir");
        std::fs::write(base.join("tiles.png"), &png).expect("tiles");
        for slot in 0..TILESET_PALETTE_COUNT {
            let pal = base.join("palettes").join(format!("{slot:02}.pal"));
            std::fs::write(pal, "JASC-PAL\r\n0100\r\n1\r\n0 0 0\r\n").expect("pal");
        }
        std::fs::write(base.join("metatiles.bin"), [0u8; 16]).expect("metatiles");
        std::fs::write(base.join("metatile_attributes.bin"), [0u8; 2]).expect("attrs");
        for &(name, frames) in general.animations {
            std::fs::create_dir_all(anim.join(name)).expect("dir");
            for n in 0..frames {
                std::fs::write(anim.join(name).join(format!("{n}.png")), &png).expect("frame");
            }
        }
        extract_tileset(&upstream, general, &mut PackWriter::new()).expect("complete");

        std::fs::remove_file(anim.join("water/7.png")).expect("remove");
        let one = extract_tileset(&upstream, general, &mut PackWriter::new());

        std::fs::remove_dir_all(anim.join("water")).expect("remove animation");
        let whole = extract_tileset(&upstream, general, &mut PackWriter::new());

        std::fs::remove_dir_all(&anim).expect("remove all");
        let domain = extract_tileset(&upstream, general, &mut PackWriter::new());
        std::fs::remove_dir_all(&upstream).expect("cleanup");

        let one = one.expect_err("one frame");
        assert!(
            matches!(&one, ExtractError::ReadFailed(p, _) if p == &anim.join("water/7.png")),
            "{one:?}"
        );
        let whole = whole.expect_err("one animation");
        assert!(
            matches!(&whole, ExtractError::ReadFailed(p, _) if p == &anim.join("water/0.png")),
            "{whole:?}"
        );
        let domain = domain.expect_err("domain");
        assert!(
            matches!(&domain, ExtractError::ReadFailed(p, _) if p == &anim.join("flower/0.png")),
            "{domain:?}"
        );
    }
}
