//! Extracts the player, NPC, and non-people object-event sprite sheets and
//! their palettes.

use std::path::Path;

use super::scope::{sprite_palette_ids, ObjectSheet, OBJECT_SHEETS, PEOPLE_SHEETS};
use super::{collect_pngs_sorted, push_palette_entry, push_png_entry, ExtractError};
use pack_format::PackWriter;

pub(super) const FIRST_GENERIC_NPC_PALETTE: u8 = 1;
pub(super) const LAST_GENERIC_NPC_PALETTE: u8 = 4;

/// Refuse a checkout missing any pinned people sheet PNG.
fn require_sprite_sources(people_dir: &Path) -> Result<(), ExtractError> {
    for stem in PEOPLE_SHEETS {
        let path = people_dir.join(format!("{stem}.png"));
        if !path.is_file() {
            return Err(ExtractError::ReadFailed(
                path,
                "required people sprite sheet is missing".to_owned(),
            ));
        }
    }
    Ok(())
}

fn object_sheet_path(pics_dir: &Path, sheet: ObjectSheet) -> std::path::PathBuf {
    pics_dir.join(sheet.dir).join(format!("{}.png", sheet.stem))
}

pub(super) fn extract_sprites(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let people_dir = upstream.join("graphics/object_events/pics/people");
    require_sprite_sources(&people_dir)?;
    for png_path in collect_pngs_sorted(&people_dir)? {
        let rel = png_path
            .strip_prefix(&people_dir)
            .expect("collect_pngs_sorted only returns paths under people_dir")
            .with_extension("");
        let rel_id = rel.to_string_lossy().replace('\\', "/");
        push_png_entry(&png_path, format!("sprite/{rel_id}"), writer)?;
    }

    let pics_dir = upstream.join("graphics/object_events/pics");
    for sheet in OBJECT_SHEETS {
        push_png_entry(
            &object_sheet_path(&pics_dir, sheet),
            sheet.pack_id(),
            writer,
        )?;
    }

    let palettes_dir = upstream.join("graphics/object_events/palettes");
    for id in sprite_palette_ids() {
        let name = id
            .strip_prefix("sprite/palette/")
            .expect("sprite palette inventory uses the palette prefix");
        let path = palettes_dir.join(format!("{name}.pal"));
        push_palette_entry(&path, id, writer)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        extract_sprites, object_sheet_path, sprite_palette_ids, ExtractError, ObjectSheet,
        PackWriter, OBJECT_SHEETS, PEOPLE_SHEETS,
    };

    #[test]
    fn a_checkout_missing_a_required_sprite_sheet_is_refused() {
        let upstream = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-missing-sprite-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&upstream);
        let people = upstream.join("graphics/object_events/pics/people");
        let palettes = upstream.join("graphics/object_events/palettes");
        let png = crate::extract::png::tests::tiny_indexed_png(8, 1, 1, &[0]);
        std::fs::create_dir_all(&palettes).expect("dir");
        for id in sprite_palette_ids() {
            let name = id.strip_prefix("sprite/palette/").expect("palette id");
            let pal = palettes.join(format!("{name}.pal"));
            std::fs::write(pal, "JASC-PAL\r\n0100\r\n1\r\n0 0 0\r\n").expect("pal");
        }
        for stem in PEOPLE_SHEETS {
            let path = people.join(format!("{stem}.png"));
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, &png).expect("sheet");
        }
        let pics = upstream.join("graphics/object_events/pics");
        for sheet in OBJECT_SHEETS {
            let path = object_sheet_path(&pics, sheet);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, &png).expect("object sheet");
        }
        extract_sprites(&upstream, &mut PackWriter::new()).expect("complete");

        std::fs::remove_file(people.join("brendan/walking.png")).expect("remove");
        let walking = extract_sprites(&upstream, &mut PackWriter::new());
        std::fs::write(people.join("brendan/walking.png"), &png).expect("restore");
        let truck = object_sheet_path(
            &pics,
            ObjectSheet {
                dir: "misc",
                stem: "truck",
            },
        );
        std::fs::remove_file(&truck).expect("remove truck");
        let object = extract_sprites(&upstream, &mut PackWriter::new());
        std::fs::remove_file(people.join("may/running.png")).expect("remove");
        std::fs::remove_dir_all(&people).expect("remove all");
        let domain = extract_sprites(&upstream, &mut PackWriter::new());
        std::fs::remove_dir_all(&upstream).expect("cleanup");

        let walking = walking.expect_err("walking");
        assert!(
            matches!(&walking, ExtractError::ReadFailed(p, _) if p == &people.join("brendan/walking.png")),
            "{walking:?}"
        );
        let object = object.expect_err("object sheet");
        assert!(
            matches!(&object, ExtractError::ReadFailed(p, _) if p.ends_with("misc/truck.png")),
            "{object:?}"
        );
        let domain = domain.expect_err("domain");
        assert!(
            matches!(&domain, ExtractError::ReadFailed(p, _) if p == &people.join(format!("{}.png", PEOPLE_SHEETS[0]))),
            "{domain:?}"
        );
    }
}
