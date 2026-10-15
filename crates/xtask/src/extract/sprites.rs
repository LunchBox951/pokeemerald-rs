//! Extracts the player and NPC sprite sheets and their palettes.

use std::path::Path;

use super::scope::{sprite_palette_ids, PEOPLE_SHEETS};
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
    use super::{extract_sprites, ExtractError, PackWriter, PEOPLE_SHEETS};

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
        for name in ["brendan", "may", "npc_1", "npc_2", "npc_3", "npc_4"] {
            let pal = palettes.join(format!("{name}.pal"));
            std::fs::write(pal, "JASC-PAL\r\n0100\r\n1\r\n0 0 0\r\n").expect("pal");
        }
        for stem in PEOPLE_SHEETS {
            let path = people.join(format!("{stem}.png"));
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, &png).expect("sheet");
        }
        extract_sprites(&upstream, &mut PackWriter::new()).expect("complete");

        std::fs::remove_file(people.join("brendan/walking.png")).expect("remove");
        let walking = extract_sprites(&upstream, &mut PackWriter::new());
        std::fs::remove_file(people.join("may/running.png")).expect("remove");
        std::fs::remove_dir_all(&people).expect("remove all");
        let domain = extract_sprites(&upstream, &mut PackWriter::new());
        std::fs::remove_dir_all(&upstream).expect("cleanup");

        let walking = walking.expect_err("walking");
        assert!(
            matches!(&walking, ExtractError::ReadFailed(p, _) if p == &people.join("brendan/walking.png")),
            "{walking:?}"
        );
        let domain = domain.expect_err("domain");
        assert!(
            matches!(&domain, ExtractError::ReadFailed(p, _) if p == &people.join(format!("{}.png", PEOPLE_SHEETS[0]))),
            "{domain:?}"
        );
    }
}
