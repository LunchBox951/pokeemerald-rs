//! Locating the starters' static battler art (`src/data/pokemon_graphics`).
//!
//! The pack's `pokemon/<species>/{front,back}` images and
//! `palette/{normal,shiny}` palettes are the expectation. The ROM stores all
//! four as LZ77 streams behind four 8-byte-row species tables:
//! `gMonStillFrontPicTable` and `gMonBackPicTable` (`{data, size, tag}`) and
//! `gMonPaletteTable` and `gMonShinyPaletteTable` (`{data, tag}`).
//!
//! The walk:
//!
//! 1. Images are found by their decompressed tiles ([`super::images`]).
//!    Palettes are found by their decompressed colours, which can match more
//!    than one stream.
//! 2. Each table is found through the pointers to the art: biased back by
//!    each species' own row, they name candidate table bases, and exactly one
//!    base must hold, at every species' row, a pointer to that species' art
//!    and the size and tag upstream's `SPECIES_SPRITE`, `SPECIES_PAL`, and
//!    `SPECIES_SHINY_PAL` macros write.
//! 3. A palette's address is the pointer its table row holds, which settles
//!    a bank the ROM stores more than once.

use std::collections::BTreeSet;

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::inventory;
use super::locate::{only_one_matching, to_addr, u16_at_addr, u32_at_addr};
use super::plan::{
    BattlerPlan, BattlerSpeciesPlan, ImagePlan, PalettePlan, ReportLine, Resolution,
};
use super::tilesets::len32;
use super::Context;
use crate::extract::scope::BATTLERS;

/// One table row, in bytes.
const RECORD_BYTES: u32 = 8;
/// How many rows each table holds: `SPECIES_UNOWN_QMARK + 1`.
const TABLE_ROWS: u32 = 440;
/// `MON_PIC_SIZE`, the `size` of every picture row.
const PIC_SIZE: u16 = 0x800;
/// `SPECIES_SHINY_TAG`, added to a species' tag in the shiny palette table.
const SHINY_TAG: u16 = 500;
/// Offset of a picture row's `size` field.
const PIC_SIZE_FIELD: u32 = 4;
/// Offset of a picture row's `tag` field.
const PIC_TAG_FIELD: u32 = 6;
/// Offset of a palette row's `tag` field.
const PALETTE_TAG_FIELD: u32 = 4;

/// What a table's row for one species must carry besides the pointer.
#[derive(Clone, Copy)]
enum Row {
    /// `{data, size: MON_PIC_SIZE, tag: species}`.
    Picture,
    /// `{data, tag: species + offset}`.
    Palette { tag_offset: u16 },
}

impl Row {
    fn holds(self, ctx: &Context<'_>, row: u32, species: u16) -> bool {
        match self {
            Self::Picture => {
                u16_at_addr(ctx.rom, row + PIC_SIZE_FIELD) == Some(PIC_SIZE)
                    && u16_at_addr(ctx.rom, row + PIC_TAG_FIELD) == Some(species)
            }
            Self::Palette { tag_offset } => {
                u16_at_addr(ctx.rom, row + PALETTE_TAG_FIELD) == Some(species + tag_offset)
            }
        }
    }
}

/// Locate every starter's images, palettes, and the four tables.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an absent pack entry;
/// [`GenRomProfileError::NotFound`] for art the ROM does not hold;
/// [`GenRomProfileError::StructMismatch`] if a table cannot be pinned to
/// exactly one base.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<BattlerPlan, GenRomProfileError> {
    inventory::require_battlers(ctx.pack)?;

    let queries: Vec<ImageQuery> = BATTLERS
        .iter()
        .flat_map(|battler| [battler.front_id(), battler.back_id()])
        .map(ImageQuery::lz77)
        .collect();
    let images = locate_images(ctx, &queries, report)?;
    let palette_candidates = palette_candidates(ctx)?;
    let [front_table, back_table, palette_table, shiny_palette_table] =
        locate_tables(ctx, &images, &palette_candidates, report)?;

    let mut plans = Vec::with_capacity(BATTLERS.len());
    for (index, battler) in BATTLERS.iter().enumerate() {
        let species = battler.species.index();
        let name = super::locate::camel_case(battler.name);
        let normal = palette_plan(
            ctx,
            &battler.normal_id(),
            row_pointer(ctx, palette_table, species),
            &palette_candidates[index * 2],
            &format!("gMonPalette_{name}"),
            report,
        )?;
        let shiny = palette_plan(
            ctx,
            &battler.shiny_id(),
            row_pointer(ctx, shiny_palette_table, species),
            &palette_candidates[index * 2 + 1],
            &format!("gMonShinyPalette_{name}"),
            report,
        )?;
        plans.push(BattlerSpeciesPlan {
            species,
            front: images[index * 2].clone(),
            back: images[index * 2 + 1].clone(),
            normal,
            shiny,
        });
    }

    Ok(BattlerPlan {
        front_table,
        back_table,
        palette_table,
        shiny_palette_table,
        species: plans,
    })
}

/// Every address each palette's decompressed colours turn up at, normal then
/// shiny per species.
fn palette_candidates(ctx: &Context<'_>) -> Result<Vec<Vec<u32>>, GenRomProfileError> {
    let ids: Vec<String> = BATTLERS
        .iter()
        .flat_map(|battler| [battler.normal_id(), battler.shiny_id()])
        .collect();
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(ctx.pack.get(id)?.palette_payload(id)?.0.to_vec());
    }
    let hits = ctx.lz77.find_all(&needles);
    ids.iter()
        .zip(&hits)
        .map(|(id, offsets)| {
            if offsets.is_empty() {
                return Err(GenRomProfileError::NotFound { id: id.clone() });
            }
            Ok(offsets.iter().copied().map(to_addr).collect())
        })
        .collect()
}

/// Pin the front, back, normal-palette, and shiny-palette tables, in that
/// order, through the art their starter rows point at.
fn locate_tables(
    ctx: &Context<'_>,
    images: &[ImagePlan],
    palettes: &[Vec<u32>],
    report: &mut Vec<ReportLine>,
) -> Result<[u32; 4], GenRomProfileError> {
    let rows = |art: &dyn Fn(usize) -> Vec<u32>| -> Vec<(u16, Vec<u32>)> {
        BATTLERS
            .iter()
            .enumerate()
            .map(|(index, battler)| (battler.species.index(), art(index)))
            .collect()
    };
    Ok([
        find_table(
            ctx,
            "gMonStillFrontPicTable",
            &rows(&|index| vec![images[index * 2].addr]),
            Row::Picture,
            report,
        )?,
        find_table(
            ctx,
            "gMonBackPicTable",
            &rows(&|index| vec![images[index * 2 + 1].addr]),
            Row::Picture,
            report,
        )?,
        find_table(
            ctx,
            "gMonPaletteTable",
            &rows(&|index| palettes[index * 2].clone()),
            Row::Palette { tag_offset: 0 },
            report,
        )?,
        find_table(
            ctx,
            "gMonShinyPaletteTable",
            &rows(&|index| palettes[index * 2 + 1].clone()),
            Row::Palette {
                tag_offset: SHINY_TAG,
            },
            report,
        )?,
    ])
}

/// The pointer in `species`' row of the table at `table`.
fn row_pointer(ctx: &Context<'_>, table: u32, species: u16) -> Option<u32> {
    u32_at_addr(ctx.rom, table + u32::from(species) * RECORD_BYTES)
}

/// The palette plan for the stream a table row points at.
fn palette_plan(
    ctx: &Context<'_>,
    id: &str,
    row_target: Option<u32>,
    candidates: &[u32],
    symbol: &str,
    report: &mut Vec<ReportLine>,
) -> Result<PalettePlan, GenRomProfileError> {
    let addr = row_target
        .filter(|addr| candidates.contains(addr))
        .ok_or_else(|| GenRomProfileError::NotFound { id: id.to_owned() })?;
    let asset = ctx.pack.get(id)?;
    let mut line = ReportLine::unique(id, addr, len32(&asset.payload)).symbol(symbol);
    if candidates.len() > 1 {
        line = line.with(Resolution::StructDerived).note(format!(
            "{} identical streams in the ROM; the species table names this one",
            candidates.len()
        ));
    }
    report.push(line);
    Ok(PalettePlan {
        id: id.to_owned(),
        addr,
        encoding: Encoding::Lz77,
        color_count: asset.palette_colors(id)?,
    })
}

/// Find the one table whose every species row points at that species' art.
fn find_table(
    ctx: &Context<'_>,
    name: &'static str,
    rows: &[(u16, Vec<u32>)],
    shape: Row,
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let Some((first_species, first_art)) = rows.first() else {
        return Err(GenRomProfileError::StructMismatch {
            id: name.to_owned(),
            reason: "no species to anchor the table".to_owned(),
        });
    };
    let first_bias = u32::from(*first_species) * RECORD_BYTES;
    let bases: BTreeSet<u32> = first_art
        .iter()
        .flat_map(|&art| ctx.pointers.refs_to(art).iter().copied())
        .filter_map(|at| at.checked_sub(first_bias))
        .collect();
    let base = only_one_matching(
        name,
        "every species' row pointing at its art with its tag",
        bases,
        |&base| {
            rows.iter().all(|(species, art)| {
                let row = base + u32::from(*species) * RECORD_BYTES;
                u32_at_addr(ctx.rom, row).is_some_and(|ptr| art.contains(&ptr))
                    && shape.holds(ctx, row, *species)
            })
        },
    )?;
    report.push(
        ReportLine::unique(name, base, TABLE_ROWS * RECORD_BYTES)
            .with(Resolution::StructDerived)
            .symbol(name)
            .note("anchored by the pointers from its starter rows"),
    );
    Ok(base)
}

#[cfg(test)]
mod tests {
    use pack_format::{image_entry_from_tiles, palette_entry, PackEntry};
    use rom_import::fixture::RomFixture;
    use rom_import::Encoding;

    use super::{locate, PIC_SIZE, RECORD_BYTES, SHINY_TAG};
    use crate::extract::scope::BATTLERS;
    use crate::gen_rom_profile::error::GenRomProfileError;
    use crate::gen_rom_profile::plan::Resolution;
    use crate::gen_rom_profile::tests::with_context;

    const FRONT_TABLE: u32 = 0x10_0000;
    const BACK_TABLE: u32 = 0x11_0000;
    const PALETTE_TABLE: u32 = 0x12_0000;
    const SHINY_TABLE: u32 = 0x13_0000;
    /// Each species' four streams sit `0x4000` apart from here, `0x1000` per slot.
    const ART: u32 = 0x20_0000;
    /// An unreferenced second copy of a palette.
    const STRAY: u32 = 0x18_0000;

    fn lz77_literals(data: &[u8]) -> Vec<u8> {
        let size = u32::try_from(data.len()).expect("small").to_le_bytes();
        let mut out = vec![0x10, size[0], size[1], size[2]];
        for chunk in data.chunks(8) {
            out.push(0);
            out.extend_from_slice(chunk);
        }
        out
    }

    fn art_at(index: usize, slot: u32) -> u32 {
        ART + u32::try_from(index).unwrap() * 0x4000 + slot * 0x1000
    }

    /// 64 tiles that differ per image.
    fn tiles(seed: usize) -> Vec<u8> {
        (0..64 * 32)
            .map(|index| u8::try_from((index * 37 + seed * 11 + 5) % 251).unwrap())
            .collect()
    }

    fn colors(seed: usize) -> Vec<u16> {
        (0..16u16)
            .map(|index| index * 0x0421 + u16::try_from(seed).unwrap() * 0x0101 + 0x0123)
            .collect()
    }

    fn colour_bytes(seed: usize) -> Vec<u8> {
        colors(seed).iter().flat_map(|c| c.to_le_bytes()).collect()
    }

    fn entries() -> Vec<PackEntry> {
        let mut entries = Vec::new();
        for (index, battler) in BATTLERS.iter().enumerate() {
            for (slot, id) in [battler.front_id(), battler.back_id()]
                .into_iter()
                .enumerate()
            {
                entries.push(
                    image_entry_from_tiles(id, &tiles(index * 4 + slot), 4, 64, 64, None)
                        .expect("image"),
                );
            }
            for (slot, id) in [battler.normal_id(), battler.shiny_id()]
                .into_iter()
                .enumerate()
            {
                entries.push(palette_entry(id, &colors(index * 4 + 2 + slot)).expect("palette"));
            }
        }
        entries
    }

    fn picture_row(data: u32, tag: u16) -> Vec<u8> {
        let mut row = data.to_le_bytes().to_vec();
        row.extend_from_slice(&PIC_SIZE.to_le_bytes());
        row.extend_from_slice(&tag.to_le_bytes());
        row
    }

    fn palette_row(data: u32, tag: u16) -> Vec<u8> {
        let mut row = data.to_le_bytes().to_vec();
        row.extend_from_slice(&tag.to_le_bytes());
        row.extend_from_slice(&[0, 0]);
        row
    }

    fn rom(shiny_tag_bias: u16) -> Vec<u8> {
        let mut rom = RomFixture::new().emerald_header();
        for (index, battler) in BATTLERS.iter().enumerate() {
            let species = battler.species.index();
            let row = u32::from(species) * RECORD_BYTES;
            let at = |base: u32| (base + row) as usize;
            rom = rom
                .write(
                    at(FRONT_TABLE),
                    &picture_row(0x0800_0000 + art_at(index, 0), species),
                )
                .write(
                    at(BACK_TABLE),
                    &picture_row(0x0800_0000 + art_at(index, 1), species),
                )
                .write(
                    at(PALETTE_TABLE),
                    &palette_row(0x0800_0000 + art_at(index, 2), species),
                )
                .write(
                    at(SHINY_TABLE),
                    &palette_row(
                        0x0800_0000 + art_at(index, 3),
                        species + SHINY_TAG + shiny_tag_bias,
                    ),
                )
                .write(art_at(index, 0) as usize, &lz77_literals(&tiles(index * 4)))
                .write(
                    art_at(index, 1) as usize,
                    &lz77_literals(&tiles(index * 4 + 1)),
                )
                .write(
                    art_at(index, 2) as usize,
                    &lz77_literals(&colour_bytes(index * 4 + 2)),
                )
                .write(
                    art_at(index, 3) as usize,
                    &lz77_literals(&colour_bytes(index * 4 + 3)),
                );
        }
        rom.finish()
    }

    #[test]
    fn every_starter_and_table_is_found_from_the_art() {
        let rom = rom(0);
        with_context("battlers", &rom, entries(), |ctx| {
            let mut report = Vec::new();
            let plan = locate(ctx, &mut report).expect("located");
            assert_eq!(plan.front_table, 0x0800_0000 + FRONT_TABLE);
            assert_eq!(plan.back_table, 0x0800_0000 + BACK_TABLE);
            assert_eq!(plan.palette_table, 0x0800_0000 + PALETTE_TABLE);
            assert_eq!(plan.shiny_palette_table, 0x0800_0000 + SHINY_TABLE);
            assert_eq!(plan.species.len(), 3);
            for (index, battler) in plan.species.iter().enumerate() {
                assert_eq!(battler.species, BATTLERS[index].species.index());
                assert_eq!(battler.front.addr, 0x0800_0000 + art_at(index, 0));
                assert_eq!(battler.back.addr, 0x0800_0000 + art_at(index, 1));
                assert_eq!(battler.normal.addr, 0x0800_0000 + art_at(index, 2));
                assert_eq!(battler.shiny.addr, 0x0800_0000 + art_at(index, 3));
                assert_eq!(battler.front.encoding, Encoding::Lz77);
                assert_eq!(battler.normal.encoding, Encoding::Lz77);
                assert_eq!(battler.normal.color_count, 16);
            }
            // 12 art roots and four tables.
            assert_eq!(report.len(), 16);
        });
    }

    #[test]
    fn a_table_row_with_the_wrong_tag_pins_no_table() {
        let rom = rom(1);
        with_context("battlers-bad-tag", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(
                    &err,
                    GenRomProfileError::StructMismatch { id, .. } if id == "gMonShinyPaletteTable"
                ),
                "{err}"
            );
        });
    }

    #[test]
    fn a_palette_stored_twice_takes_the_copy_its_table_row_names() {
        // The stray copy sits at a lower address than the one the table names.
        let mut bytes = rom(0);
        let stream = lz77_literals(&colour_bytes(2));
        bytes[STRAY as usize..STRAY as usize + stream.len()].copy_from_slice(&stream);
        with_context("battlers-stray", &bytes, entries(), |ctx| {
            let mut report = Vec::new();
            let plan = locate(ctx, &mut report).expect("located");
            assert_eq!(plan.species[0].normal.addr, 0x0800_0000 + art_at(0, 2));
            let line = report
                .iter()
                .find(|line| line.id == BATTLERS[0].normal_id())
                .expect("a report line");
            assert_eq!(line.resolution, Resolution::StructDerived);
        });
    }

    #[test]
    fn a_palette_the_rom_lacks_is_not_found() {
        let mut bytes = rom(0);
        let at = art_at(1, 3) as usize;
        bytes[at..at + 4].fill(0xFF);
        with_context("battlers-missing", &bytes, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id } if id == &BATTLERS[1].shiny_id()),
                "{err}"
            );
        });
    }

    #[test]
    fn a_pack_without_a_starter_entry_is_refused_up_front() {
        let mut pack = entries();
        pack.retain(|entry| entry.id != BATTLERS[2].back_id());
        let rom = rom(0);
        with_context("battlers-pack-gap", &rom, pack, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == &BATTLERS[2].back_id()),
                "{err}"
            );
        });
    }
}
