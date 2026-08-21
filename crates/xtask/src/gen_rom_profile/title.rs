//! Locating the title screen.
//!
//! The graphics and tilemaps are ordinary: every one is an LZ77 stream, so
//! [`super::images`] and a compressed search settle them.
//!
//! The palettes are not. `gTitleScreenBgPalettes` concatenates the logo's
//! palette with the rayquaza/clouds palette, and the ROM's copy of the logo
//! palette is *shorter than the pack's*: upstream's own build rule cuts it
//! to 224 colours (`graphics_file_rules.mk`'s `-num_colors 224`), while
//! `cargo xtask extract` reads all 256 entries of the `.pal` file, the last
//! 32 of which are black. So the logo palette is located by adjacency --
//! it is what sits immediately before the rayquaza palette -- and the
//! generator records the colour count the ROM really holds, plus a note,
//! because the equivalence harness has to reconcile that difference.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::{camel_case, exactly_one, slice_at_addr};
use super::palettes::locate_unique;
use super::plan::{
    BlobPlan, PalettePlan, ReportLine, Resolution, SymbolExpectation, TitleScreenPlan,
};
use super::tilesets::len32;
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

/// Locate the three `title/raw/*` tile arrangements, all LZ77 streams.
fn locate_tilemaps(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    let ids = ctx.pack.ids_with_prefix("title/raw/");
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(ctx.pack.get(id)?.payload.clone());
    }
    let hits = ctx.lz77.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let addr = exactly_one(id, &hits[index])?;
        let len = len32(&ctx.pack.get(id)?.payload);
        let name = camel_case(id.trim_start_matches("title/raw/"));
        report.push(ReportLine::unique(id, addr, len).symbol_contains([
            "TitleScreen".to_owned(),
            name,
            "Tilemap".to_owned(),
        ]));
        plans.push(BlobPlan {
            id: id.clone(),
            addr,
            encoding: Encoding::Lz77,
            len,
        });
    }
    Ok(plans)
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

    let (addr, color_count, dropped) = locate_trimmed_palette(ctx, adjacent)?;
    report.push(
        ReportLine::unique(TRIMMED_PALETTE, addr, u32::from(color_count) * 2)
            .with(Resolution::StructDerived)
            .symbol("gTitleScreenBgPalettes")
            .note(format!(
                "the ROM holds {color_count} colours, {dropped} fewer than the pack's \
                 (upstream cuts it with -num_colors); the pack's extra colours are all black"
            )),
    );
    plans.push(PalettePlan {
        id: TRIMMED_PALETTE.to_owned(),
        addr,
        color_count,
    });
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((plans, addr))
}

/// What a linker map should say at one title palette's address.
///
/// The logo palette *is* `gTitleScreenBgPalettes`, and the rayquaza/clouds
/// palette sits 224 colours inside it, so only the first is a symbol.
fn palette_symbol(id: &str) -> SymbolExpectation {
    match id {
        ADJACENT_PALETTE => SymbolExpectation::Interior,
        "title/palette/emerald_version" => {
            SymbolExpectation::Exact("gTitleScreenEmeraldVersionPal".to_owned())
        }
        "title/palette/press_start" => {
            SymbolExpectation::Exact("gTitleScreenPressStartPal".to_owned())
        }
        // `sUnusedUnknownPal` is a `static` with no live caller; assert
        // only that a symbol starts there.
        _ => SymbolExpectation::Unnamed,
    }
}

/// Find the logo palette by walking back from the palette stored after it.
///
/// Returns its address, the colour count the ROM really holds, and how many
/// of the pack's colours were dropped.
fn locate_trimmed_palette(
    ctx: &Context<'_>,
    adjacent: u32,
) -> Result<(u32, u16, u16), GenRomProfileError> {
    let asset = ctx.pack.get(TRIMMED_PALETTE)?;
    let pack_colors = asset.palette_colors(TRIMMED_PALETTE)?;

    // Walk the possible cuts, longest first: the ROM holds some prefix of
    // the pack's colours, ending where the next palette begins.
    for colors in (1..=pack_colors).rev() {
        let bytes = usize::from(colors) * 2;
        let Some(addr) = adjacent.checked_sub(u32::from(colors) * 2) else {
            continue;
        };
        if slice_at_addr(ctx.rom, addr, bytes) == Some(&asset.payload[..bytes])
            && asset.payload[bytes..].iter().all(|&byte| byte == 0)
        {
            return Ok((addr, colors, pack_colors - colors));
        }
    }
    Err(GenRomProfileError::StructMismatch {
        id: TRIMMED_PALETTE.to_owned(),
        reason: format!("no prefix of it sits immediately before {adjacent:08X}"),
    })
}
