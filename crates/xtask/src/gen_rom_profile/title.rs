//! Locating the title screen.
//!
//! Graphics and tilemaps are LZ77 streams, located through [`super::images`]
//! and a compressed search.
//!
//! `gTitleScreenBgPalettes` concatenates the logo palette and the
//! rayquaza/clouds palette, so the logo palette is located as whatever sits
//! immediately before the rayquaza palette ([`locate_trimmed_palette`]).
//!
//! Upstream's build rule cuts the logo palette to 224 colours
//! (`graphics_file_rules.mk`'s `-num_colors 224`) while its `.pal` holds 256
//! with the last 32 black. `xtask::extract::scope::TITLE_SCREEN_PALETTE_CUTS`
//! applies the same cut, so the pack's whole palette matches. Palette import
//! never restores omitted black, so a pack cut differently from the ROM fails
//! closed rather than yielding a plan that imports to a different entry.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::{camel_case, slice_at_addr};
use super::lz77_blobs::locate_lz77_blobs;
use super::palettes::locate_unique;
use super::plan::{
    BlobPlan, PalettePlan, ReportLine, Resolution, SymbolExpectation, TitleScreenPlan,
};
use super::Context;

/// The pack id of the palette upstream cuts short.
const TRIMMED_PALETTE: &str = "title/palette/pokemon_logo";
/// The pack id of the palette stored immediately after it.
const ADJACENT_PALETTE: &str = "title/palette/rayquaza_and_clouds";

/// Locate the whole title screen.
///
/// # Errors
///
/// Any [`GenRomProfileError`] a locator raises.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<TitleScreenPlan, GenRomProfileError> {
    super::inventory::require_title(ctx.pack)?;
    let queries: Vec<ImageQuery> = ctx
        .pack
        .ids_with_prefix("title/image/")
        .into_iter()
        .map(ImageQuery::lz77)
        .collect();
    let images = locate_images(ctx, &queries, report)?;
    let tilemaps = locate_tilemaps(ctx, report)?;
    let (palettes, bg_palettes) = locate_palettes(ctx, report)?;

    Ok(TitleScreenPlan {
        images,
        tilemaps,
        palettes,
        bg_palettes,
    })
}

/// Locate the `title/raw/*` tilemaps, all LZ77 streams.
fn locate_tilemaps(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    let ids = ctx.pack.ids_with_prefix("title/raw/");
    locate_lz77_blobs(
        ctx,
        &ids,
        &|id| tilemap_symbol(&camel_case(id.trim_start_matches("title/raw/"))),
        report,
    )
}

/// What a linker map should say at the tilemap named `name` (CamelCase).
///
/// `sTitleScreenRayquazaTilemap` is `static` (`title_screen.c:64`), so a map
/// omits it.
fn tilemap_symbol(name: &str) -> SymbolExpectation {
    if name == "Rayquaza" {
        SymbolExpectation::Local("sTitleScreenRayquazaTilemap".to_owned())
    } else {
        SymbolExpectation::Contains(vec![
            "TitleScreen".to_owned(),
            name.to_owned(),
            "Tilemap".to_owned(),
        ])
    }
}

/// Locate every title palette, including the one the ROM stores short.
fn locate_palettes(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<(Vec<PalettePlan>, u32), GenRomProfileError> {
    let all = ctx.pack.ids_with_prefix("title/palette/");
    let unique_ids: Vec<String> = all
        .iter()
        .filter(|id| id.as_str() != TRIMMED_PALETTE)
        .cloned()
        .collect();
    let mut plans = locate_unique(ctx, &unique_ids, &palette_symbol, report)?;

    let adjacent = plans
        .iter()
        .find(|plan| plan.id == ADJACENT_PALETTE)
        .ok_or_else(|| GenRomProfileError::MissingPackEntry(ADJACENT_PALETTE.to_owned()))?
        .addr;

    let (addr, color_count) = locate_trimmed_palette(ctx, adjacent)?;
    report.push(
        ReportLine::unique(TRIMMED_PALETTE, addr, u32::from(color_count) * 2)
            .with(Resolution::StructDerived)
            .symbol("gTitleScreenBgPalettes")
            .note(format!(
                "located by adjacency: {color_count} colours ending where \
                 {ADJACENT_PALETTE} begins"
            )),
    );
    plans.push(PalettePlan {
        id: TRIMMED_PALETTE.to_owned(),
        addr,
        color_count,
        encoding: Encoding::Raw,
    });
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((plans, addr))
}

/// What a linker map should say at one title palette's address.
///
/// The logo palette starts `gTitleScreenBgPalettes` and the rayquaza/clouds
/// palette sits inside it, so only the logo palette has a symbol of its own.
fn palette_symbol(id: &str) -> SymbolExpectation {
    match id {
        ADJACENT_PALETTE => SymbolExpectation::Interior,
        "title/palette/emerald_version" => {
            SymbolExpectation::Exact("gTitleScreenEmeraldVersionPal".to_owned())
        }
        "title/palette/press_start" => {
            SymbolExpectation::Exact("gTitleScreenPressStartPal".to_owned())
        }
        // `sUnusedUnknownPal` is a `static` (title_screen.c:61); a linker map
        // does not list it.
        "title/palette/unused" => SymbolExpectation::Local("sUnusedUnknownPal".to_owned()),
        _ => SymbolExpectation::Unnamed,
    }
}

/// Find the logo palette by walking back from the palette stored after it.
///
/// The palette must be the pack's whole colour count: the importer reads
/// exactly `color_count` colours and never restores omitted trailing black,
/// so a plan that stored a shorter prefix would import to a different pack
/// entry. A pack cut differently from the ROM therefore fails closed
/// instead of being trimmed to fit.
///
/// Returns its address and colour count.
fn locate_trimmed_palette(
    ctx: &Context<'_>,
    adjacent: u32,
) -> Result<(u32, u16), GenRomProfileError> {
    let asset = ctx.pack.get(TRIMMED_PALETTE)?;
    let (payload, colors) = asset.palette_payload(TRIMMED_PALETTE)?;
    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: TRIMMED_PALETTE.to_owned(),
        reason,
    };

    let bytes = payload.len();
    let addr = u32::try_from(bytes)
        .ok()
        .and_then(|len| adjacent.checked_sub(len))
        .ok_or_else(|| {
            mismatch(format!(
                "its {colors} colours do not fit before {adjacent:08X}"
            ))
        })?;
    if slice_at_addr(ctx.rom, addr, bytes) == Some(payload) {
        Ok((addr, colors))
    } else {
        Err(mismatch(format!(
            "its {colors} colours do not sit immediately before {adjacent:08X}; \
             the pack must be cut the way the ROM stores it"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen_rom_profile::images::local_image_symbol;
    use crate::gen_rom_profile::tests::{assert_survives_ld_map, distinctive_tiles, lz77_literals};
    use pack_format::{image_entry_from_tiles, palette_entry, raw_entry};
    use rom_import::fixture::RomFixture;

    /// Tiles unique to `seed`, so each planted sheet has exactly one hit.
    fn seeded_tiles(seed: u8) -> Vec<u8> {
        distinctive_tiles(4)
            .into_iter()
            .map(|byte| byte ^ seed)
            .collect()
    }

    #[test]
    fn static_title_roots_survive_a_map_that_omits_locals() {
        // The production locators build every report line, so the test fails
        // if `assign`, `locate_tilemaps` or `palette_symbol` stops marking an
        // upstream `static` root as local.
        let images = [("rayquaza", 0x11), ("logo_shine", 0x22), ("clouds", 0x33)];
        let tilemap: Vec<u8> = (0..64u8).map(|n| n.wrapping_mul(13) ^ 0x5a).collect();
        let colors: Vec<u16> = (0..16u16).map(|n| n * 0x0421 + 0x0777).collect();
        let color_bytes: Vec<u8> = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
        let mut fixture = RomFixture::new().emerald_header();
        let mut entries = Vec::new();
        for (index, (name, seed)) in (0usize..).zip(images) {
            let tiles = seeded_tiles(seed);
            fixture = fixture.write(0x40_0000 + index * 0x1000, &lz77_literals(&tiles));
            entries.push(
                image_entry_from_tiles(format!("title/image/{name}"), &tiles, 4, 16, 16, None)
                    .expect("image entry"),
            );
        }
        let rom = fixture
            .write(0x41_0000, &lz77_literals(&tilemap))
            .write(0x42_0000, &color_bytes)
            .finish();
        entries.push(raw_entry("title/raw/rayquaza".into(), tilemap));
        entries.push(palette_entry("title/palette/unused".into(), &colors).expect("palette"));

        with_context("title-statics", &rom, entries, |ctx| {
            let mut report = Vec::new();
            let queries: Vec<ImageQuery> = images
                .iter()
                .map(|(name, _)| ImageQuery::lz77(format!("title/image/{name}")))
                .collect();
            locate_images(ctx, &queries, &mut report).expect("images located");
            locate_tilemaps(ctx, &mut report).expect("tilemap located");
            locate_unique(
                ctx,
                &["title/palette/unused".to_owned()],
                &palette_symbol,
                &mut report,
            )
            .expect("palette located");
            assert_eq!(report.len(), 5);
            assert_survives_ld_map("title-statics-map", 0x0854_0000, &mut report);
        });
    }

    #[test]
    fn global_title_roots_keep_their_map_check() {
        assert!(matches!(
            tilemap_symbol("Clouds"),
            SymbolExpectation::Contains(_)
        ));
        assert_eq!(local_image_symbol("title/image/pokemon_logo"), None);
        assert_eq!(
            palette_symbol("title/palette/press_start"),
            SymbolExpectation::Exact("gTitleScreenPressStartPal".to_owned())
        );
    }
    use crate::gen_rom_profile::tests::{image_twin, with_context};

    /// Colours of a synthetic 16-colour palette, distinct per `seed`.
    fn palette_colors(seed: u16) -> Vec<u16> {
        (0..16u16)
            .map(|n| 0x1000 + seed * 0x40 + n * 3 + 1)
            .collect()
    }

    fn color_bytes(colors: &[u16]) -> Vec<u8> {
        colors.iter().flat_map(|c| c.to_le_bytes()).collect()
    }

    /// Address of the first logo colour in the synthetic title ROM.
    const LOGO_ADDR: usize = 0x43_0000;

    /// A complete synthetic title pack and ROM. The ROM holds 8 logo colours
    /// with the rayquaza/clouds palette right after them; the pack holds
    /// those 8 followed by `logo_pack_tail`.
    fn title_fixture(logo_pack_tail: &[u16]) -> (Vec<u8>, Vec<pack_format::PackEntry>) {
        let images = [
            ("clouds", 0x11u8),
            ("emerald_version", 0x22),
            ("logo_shine", 0x33),
            ("pokemon_logo", 0x44),
            ("press_start", 0x55),
            ("rayquaza", 0x66),
        ];
        let tilemaps = [("clouds", 3u8), ("pokemon_logo", 5), ("rayquaza", 7)];
        let mut fixture = RomFixture::new().emerald_header();
        let mut entries = Vec::new();
        for (index, (name, seed)) in (0usize..).zip(images) {
            let tiles = seeded_tiles(seed);
            fixture = fixture.write(0x40_0000 + index * 0x1000, &lz77_literals(&tiles));
            entries.push(
                image_entry_from_tiles(format!("title/image/{name}"), &tiles, 4, 16, 16, None)
                    .expect("image entry"),
            );
        }
        for (index, (name, seed)) in (0usize..).zip(tilemaps) {
            let map: Vec<u8> = (0..64u8)
                .map(|n| n.wrapping_mul(seed * 2 + 11) ^ (seed * 17))
                .collect();
            fixture = fixture.write(0x41_0000 + index * 0x1000, &lz77_literals(&map));
            entries.push(raw_entry(format!("title/raw/{name}"), map));
        }
        for (index, name) in (0usize..).zip(["emerald_version", "press_start", "unused"]) {
            let colors = palette_colors(u16::try_from(index).expect("small") + 1);
            fixture = fixture.write(0x42_0000 + index * 0x100, &color_bytes(&colors));
            entries.push(palette_entry(format!("title/palette/{name}"), &colors).expect("palette"));
        }
        let logo_rom: Vec<u16> = palette_colors(9)[..8].to_vec();
        let mut logo_pack = logo_rom.clone();
        logo_pack.extend_from_slice(logo_pack_tail);
        let rayquaza = palette_colors(10);
        let rom = fixture
            .write(LOGO_ADDR, &color_bytes(&logo_rom))
            .write(LOGO_ADDR + 16, &color_bytes(&rayquaza))
            .finish();
        entries.push(palette_entry(TRIMMED_PALETTE.into(), &logo_pack).expect("palette"));
        entries.push(palette_entry(ADJACENT_PALETTE.into(), &rayquaza).expect("palette"));
        (rom, entries)
    }

    #[test]
    fn locate_composes_the_title_and_places_the_logo_palette_by_adjacency() {
        let (rom, entries) = title_fixture(&[]);
        with_context("title-locate-e2e", &rom, entries, |ctx| {
            let mut report = Vec::new();
            let plan = locate(ctx, &mut report).expect("title located");
            assert_eq!(plan.images.len(), 6);
            assert_eq!(plan.tilemaps.len(), 3);
            assert_eq!(plan.palettes.len(), 5);
            // 6 images + 3 tilemaps + 5 palettes, one line each.
            assert_eq!(report.len(), 14);
            let logo = plan
                .palettes
                .iter()
                .find(|p| p.id == TRIMMED_PALETTE)
                .expect("logo palette");
            let adjacent = plan
                .palettes
                .iter()
                .find(|p| p.id == ADJACENT_PALETTE)
                .expect("adjacent palette");
            assert_eq!(
                logo.addr,
                0x0800_0000 + u32::try_from(LOGO_ADDR).expect("fits")
            );
            assert_eq!(logo.color_count, 8);
            assert_eq!(logo.addr + 16, adjacent.addr);
            assert_eq!(plan.bg_palettes, logo.addr);
            let line = report
                .iter()
                .find(|l| l.id == TRIMMED_PALETTE)
                .expect("logo report line");
            assert_eq!(line.addr, logo.addr);
            assert_eq!(line.len, 16);
            assert_eq!(
                line.symbol,
                SymbolExpectation::Exact("gTitleScreenBgPalettes".to_owned())
            );
            assert!(
                line.note
                    .as_deref()
                    .is_some_and(|note| note.contains("8 colours") && note.contains("begins")),
                "{:?}",
                line.note
            );
        });
    }

    #[test]
    fn importing_the_logo_plan_reproduces_the_source_pack_entry() {
        // Import reads exactly `color_count * 2` bytes at the root's address
        // and builds the entry from them (rom-import domains/mod.rs), so this
        // mirrors that read and compares it with the entry the plan came from.
        let (rom, entries) = title_fixture(&[]);
        let source = entries
            .iter()
            .find(|e| e.id == TRIMMED_PALETTE)
            .map(|e| (format!("{:?}", e.kind), e.payload.clone()))
            .expect("source logo entry");
        let logo = with_context("title-trim-roundtrip", &rom, entries, |ctx| {
            locate(ctx, &mut Vec::new())
        })
        .expect("title located")
        .palettes
        .into_iter()
        .find(|p| p.id == TRIMMED_PALETTE)
        .expect("logo plan");
        let bytes =
            slice_at_addr(&rom, logo.addr, usize::from(logo.color_count) * 2).expect("in rom");
        let colors: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let imported = palette_entry(TRIMMED_PALETTE.into(), &colors).expect("palette");
        assert_eq!(
            (format!("{:?}", imported.kind), imported.payload),
            source,
            "import of the plan must reproduce the pack entry"
        );
    }

    #[test]
    fn a_pack_logo_palette_longer_than_the_rom_stores_is_refused() {
        // The pack holds 16 colours, the last 8 black, and the ROM only the
        // first 8. Import would emit the shorter palette, so the plan cannot
        // be trimmed to fit: the stale cut fails closed.
        let (rom, entries) = title_fixture(&[0; 8]);
        let err = with_context("title-locate-stale-cut", &rom, entries, |ctx| {
            locate(ctx, &mut Vec::new())
        })
        .expect_err("a pack cut differently from the ROM fails closed");
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, reason }
                if id == TRIMMED_PALETTE && reason.contains("16 colours")),
            "{err:?}"
        );
    }

    #[test]
    fn a_pack_logo_palette_with_a_non_black_tail_is_refused() {
        // The ROM holds the first 8 colours, but the pack's other 8 are not
        // black, so the pack's palette does not match the bytes before the
        // adjacent palette.
        let (rom, entries) = title_fixture(&[0x7fff; 8]);
        let err = with_context("title-locate-bad-tail", &rom, entries, |ctx| {
            locate(ctx, &mut Vec::new())
        })
        .expect_err("a non-black pack tail fails closed");
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == TRIMMED_PALETTE),
            "{err:?}"
        );
    }

    #[test]
    fn a_logo_palette_not_adjacent_to_the_rayquaza_palette_is_refused() {
        let (mut rom, entries) = title_fixture(&[]);
        // Corrupt the last logo colour in the ROM.
        rom[LOGO_ADDR + 14] ^= 0xff;
        let err = with_context("title-locate-bad-adjacency", &rom, entries, |ctx| {
            locate(ctx, &mut Vec::new())
        })
        .expect_err("a logo palette that does not abut the next one fails closed");
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == TRIMMED_PALETTE),
            "{err:?}"
        );
    }

    #[test]
    fn a_title_tilemap_of_the_wrong_kind_is_refused() {
        let payload = [9u8, 8, 7, 6, 5, 4, 3, 2];
        let id = "title/raw/demo";
        let mut rom = vec![0x10, 8, 0, 0];
        rom.push(0);
        rom.extend_from_slice(&payload);
        let err = with_context(
            "title-tilemap-wrong-kind",
            &rom,
            vec![image_twin(id, &payload)],
            |ctx| locate_tilemaps(ctx, &mut Vec::new()),
        )
        .expect_err("an image-kind tilemap is refused");
        assert!(
            matches!(&err, GenRomProfileError::WrongPackEntryKind { id: got, .. } if got == id),
            "{err:?}"
        );
    }
}
