//! Locating object-event sprite sheets and their palettes.
//!
//! The sheets are ordinary uncompressed art, so [`super::images`] handles
//! them, duplicates included.
//!
//! The palettes are not. Several object-event banks appear two or three
//! times in the ROM, and unlike duplicate art the copies are *not*
//! interchangeable: only one of them is the bank the game loads under a
//! given tag. The tie is broken through `sObjectEventSpritePalettes`, the
//! `{data, tag}` table `src/event_object_movement.c` declares. At least one
//! bank in the set is unique, so its own entry in that table locates the
//! table; the table's extent then says which copy of an ambiguous bank is
//! the real one.

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::{only_one_matching, to_offset, u16_at_addr, u32_at_addr};
use super::plan::{PalettePlan, ReportLine, Resolution, SpritePlan};
use super::tilesets::len32;
use super::Context;

/// One `struct SpritePalette` is a pointer, a `u16` tag, and two bytes of
/// padding.
const RECORD_BYTES: u32 = 8;
/// Offset of the `tag` field inside a record.
const FIELD_TAG: u32 = 4;
/// Offset of the padding after the tag.
const FIELD_PADDING: u32 = 6;
/// How far a neighbouring record's tag may sit from the seed's before the
/// table is assumed to have ended. Object-event tags are allocated as one
/// dense block, so a neighbour is always close.
const TAG_SPREAD: u16 = 0x100;

/// Locate every object-event sprite sheet and palette.
///
/// # Errors
///
/// Any [`GenRomProfileError`] a locator raises, plus
/// [`GenRomProfileError::StructMismatch`] if `sObjectEventSpritePalettes`
/// cannot be found or does not resolve a duplicated bank.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<SpritePlan, GenRomProfileError> {
    let queries: Vec<ImageQuery> = ctx
        .pack
        .ids_with_prefix("sprite/")
        .into_iter()
        .filter(|id| !id.starts_with("sprite/palette/"))
        .map(ImageQuery::raw)
        .collect();
    let sheets = locate_images(ctx, &queries, report)?;

    let ids = ctx.pack.ids_with_prefix("sprite/palette/");
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(ctx.pack.get(id)?.payload.clone());
    }
    let hits = ctx.raw.find_all(&needles);

    let seed = ids
        .iter()
        .zip(hits.iter())
        .find(|(_, offsets)| offsets.len() == 1)
        .map(|(id, offsets)| (id.clone(), super::locate::to_addr(offsets[0])))
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: "sObjectEventSpritePalettes".to_owned(),
            reason: "no object-event palette is unique, so nothing seeds the table".to_owned(),
        })?;
    let table = find_palette_table(ctx, seed.1)?;
    report.push(
        ReportLine::unique(
            "sObjectEventSpritePalettes",
            table.start,
            table.end - table.start,
        )
        .with(Resolution::StructDerived)
        .symbol("sObjectEventSpritePalettes")
        .note(format!("seeded from `{}`", seed.0)),
    );

    let mut palettes = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let asset = ctx.pack.get(id)?;
        let candidates: Vec<u32> = hits[index]
            .iter()
            .copied()
            .map(super::locate::to_addr)
            .collect();
        let (addr, resolution, note) = match candidates.as_slice() {
            [] => return Err(GenRomProfileError::NotFound { id: id.clone() }),
            [only] => (*only, Resolution::UniqueSignature, None),
            many => {
                let chosen = only_one_matching(
                    id,
                    "a reference from sObjectEventSpritePalettes",
                    many.iter().copied(),
                    |&addr| table.references(ctx, addr),
                )?;
                (
                    chosen,
                    Resolution::StructDerived,
                    Some(format!(
                        "{} identical banks in the ROM; the palette table names this one",
                        many.len()
                    )),
                )
            }
        };
        let mut line = ReportLine::unique(id, addr, len32(&asset.payload))
            .with(resolution)
            .symbol_contains(["gObjectEventPal_"]);
        if let Some(note) = note {
            line = line.note(note);
        }
        report.push(line);
        palettes.push(PalettePlan {
            id: id.clone(),
            addr,
            color_count: asset.palette_colors(id)?,
        });
    }

    Ok(SpritePlan {
        palette_table: table.start,
        sheets,
        palettes,
    })
}

/// The extent of `sObjectEventSpritePalettes`, in GBA bus addresses.
struct PaletteTable {
    start: u32,
    end: u32,
}

impl PaletteTable {
    /// Whether some record in the table points at `target`.
    fn references(&self, ctx: &Context<'_>, target: u32) -> bool {
        let mut at = self.start;
        while at < self.end {
            if u32_at_addr(ctx.rom, at) == Some(target) {
                return true;
            }
            at += RECORD_BYTES;
        }
        false
    }
}

/// Find the table by the one record that points at a bank found only once.
fn find_palette_table(ctx: &Context<'_>, seed: u32) -> Result<PaletteTable, GenRomProfileError> {
    let id = "sObjectEventSpritePalettes";
    let record = only_one_matching(
        id,
        "the `struct SpritePalette` layout",
        ctx.pointers.refs_to(seed).iter().copied(),
        |&at| record_shape(ctx, at).is_some(),
    )?;
    let seed_tag = record_shape(ctx, record).ok_or_else(|| GenRomProfileError::StructMismatch {
        id: id.to_owned(),
        reason: "the seed record stopped reading as one".to_owned(),
    })?;

    let mut start = record;
    while let Some(previous) = start.checked_sub(RECORD_BYTES) {
        if !is_neighbour(ctx, previous, seed_tag) {
            break;
        }
        start = previous;
    }
    let mut end = record + RECORD_BYTES;
    while is_neighbour(ctx, end, seed_tag) {
        end += RECORD_BYTES;
    }
    Ok(PaletteTable { start, end })
}

/// The tag of a well-formed `struct SpritePalette` at `at`, if it is one.
fn record_shape(ctx: &Context<'_>, at: u32) -> Option<u16> {
    let data = u32_at_addr(ctx.rom, at)?;
    to_offset(data)?;
    if u16_at_addr(ctx.rom, at + FIELD_PADDING)? != 0 {
        return None;
    }
    let tag = u16_at_addr(ctx.rom, at + FIELD_TAG)?;
    (tag != 0).then_some(tag)
}

/// Whether `at` continues the same table as a record tagged `seed_tag`.
fn is_neighbour(ctx: &Context<'_>, at: u32, seed_tag: u16) -> bool {
    record_shape(ctx, at).is_some_and(|tag| tag.abs_diff(seed_tag) < TAG_SPREAD)
}

#[cfg(test)]
mod tests {
    use super::{PaletteTable, RECORD_BYTES};

    #[test]
    fn a_table_spans_whole_records() {
        let table = PaletteTable {
            start: 0x0850_BBC8,
            end: 0x0850_BBC8 + RECORD_BYTES * 4,
        };
        assert_eq!(table.end - table.start, 32);
    }
}
