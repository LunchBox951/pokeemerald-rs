//! Pack and palette construction contract: title palettes splice from pack entries,
//! and pack-loading failures report distinct, actionable errors.

use super::test_support::{write_synthetic_palette_pack, GREEN_BGR555_LE, RED_BGR555_LE};
use super::{title_palette_from_refs, TitleSceneError, LOGO_PALETTE_COLORS};
use assets::AssetPack;

#[test]
fn title_palette_splices_rayquaza_clouds_after_224_logo_colors() {
    let logo = RED_BGR555_LE.repeat(256);
    let rayquaza_clouds = GREEN_BGR555_LE.repeat(16);
    let fixture = write_synthetic_palette_pack(&[
        ("title/palette/pokemon_logo", &logo),
        ("title/palette/rayquaza_and_clouds", &rayquaza_clouds),
    ]);
    let pack = AssetPack::load(&fixture.path).unwrap();
    let palette = title_palette_from_refs(
        pack.palette("title/palette/pokemon_logo").unwrap(),
        pack.palette("title/palette/rayquaza_and_clouds").unwrap(),
    );
    let split = u8::try_from(LOGO_PALETTE_COLORS).unwrap();
    assert_eq!(palette.color(split - 1).to_rgb888().r, 255);
    assert_eq!(palette.color(split).to_rgb888().g, 255);
    assert_eq!(palette.color(split + 15).to_rgb888().g, 255);
    assert_eq!(palette.color(split + 16), rendering::Bgr555::default());
}

#[test]
fn title_palette_never_reads_past_either_entrys_own_color_count() {
    let logo = RED_BGR555_LE;
    let rayquaza_clouds = GREEN_BGR555_LE;
    let fixture = write_synthetic_palette_pack(&[
        ("title/palette/pokemon_logo", &logo),
        ("title/palette/rayquaza_and_clouds", &rayquaza_clouds),
    ]);
    let pack = AssetPack::load(&fixture.path).unwrap();
    let palette = title_palette_from_refs(
        pack.palette("title/palette/pokemon_logo").unwrap(),
        pack.palette("title/palette/rayquaza_and_clouds").unwrap(),
    );
    assert_eq!(palette.color(0).to_rgb888().r, 255);
    assert_eq!(palette.color(1), rendering::Bgr555::default());
    let split = u8::try_from(LOGO_PALETTE_COLORS).unwrap();
    assert_eq!(palette.color(split).to_rgb888().g, 255);
    assert_eq!(palette.color(split + 1), rendering::Bgr555::default());
}

#[test]
fn title_scene_error_display_messages_are_informative() {
    let err = TitleSceneError::ImageNotTileAligned {
        id: "title/image/x",
        width: 12,
        height: 8,
    };
    let rendered = err.to_string();
    assert!(rendered.contains("title/image/x"));
    assert!(rendered.contains("12"));
}

#[test]
fn is_pack_missing_is_true_only_for_not_found() {
    let missing = TitleSceneError::from(assets::PackError::NotFound("/x".into()));
    assert!(missing.is_pack_missing());
    let other = TitleSceneError::from(assets::PackError::BadMagic);
    assert!(!other.is_pack_missing());
}

#[test]
fn stale_pack_reports_the_rebuild_remedy_for_both_audiences() {
    // A pack that loads but predates this build (missing one of the
    // `title/*` entries this module requires) must render an actionable
    // rebuild message, not just the bare "no entry with id" symptom --
    // and be distinguishable via `is_pack_stale`. Both audiences are
    // named, as `PackError::NotFound`/`UnsupportedVersion` do: a player's
    // imported pack goes stale the same way an extracted one does, and
    // `cargo xtask extract` needs a decomp checkout the player has not
    // got (README's "Playing" section promises the re-import).
    let stale = TitleSceneError::from(assets::PackError::UnknownAsset(
        "title/palette/emerald_version".to_owned(),
    ));
    assert!(stale.is_pack_stale());
    assert!(!stale.is_pack_missing());
    let rendered = stale.to_string();
    assert!(rendered.contains("title/palette/emerald_version"));
    assert!(rendered.contains("cargo xtask extract"));
    assert!(rendered.contains("--import-rom"));

    let other = TitleSceneError::from(assets::PackError::BadMagic);
    assert!(!other.is_pack_stale());
}

#[test]
fn load_default_reports_pack_missing_when_no_pack_is_extracted() {
    if AssetPack::default_path().is_file() {
        return;
    }
    let err = super::load_default().unwrap_err();
    assert!(err.is_pack_missing());
    let rendered = err.to_string();
    // Both remedies survive the wrap into `TitleSceneError`: a player needs
    // the import flag, a developer needs the checkout commands.
    assert!(rendered.contains("--import-rom"), "{rendered}");
    assert!(rendered.contains("init.sh"), "{rendered}");
    assert!(rendered.contains("cargo xtask extract"), "{rendered}");
}
