//! The pairs the checkout's pack must hold: those the animation catalog loads.
//!
//! The inventory is [`BattleEffectRecord::inventory`] over the catalog entry,
//! never whatever `battle-effect/` entries the pack happens to hold, so a pair
//! missing from the pack, a surplus one, or one of the wrong shape is refused
//! before any root is located.

use std::collections::BTreeSet;

use assets::pack::BattleEffectRecord;
use assets::BattleAnimationCatalog;

use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::pack_source::PackSource;

const PREFIX: &str = "battle-effect/";
/// Every sheet is a 4bpp sprite sheet.
const BIT_DEPTH: u8 = 4;
/// Colours in one 4bpp palette bank.
const BANK_COLORS: u16 = 16;

/// The records the catalog requires, each with its sheet and palette validated.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an absent catalog, sheet or
/// palette, or when the catalog loads no effect sprite at all;
/// [`GenRomProfileError::WrongPackEntryKind`] for a sheet that is not an image
/// or a palette that is not a palette; [`GenRomProfileError::EntryShape`] for a
/// catalog that does not decode, a sheet that is not whole 4bpp tiles with one
/// byte per pixel, a palette that is not a whole number of banks at two bytes
/// a colour, or a pixel past its palette;
/// [`GenRomProfileError::StructMismatch`] for a `battle-effect/` entry outside
/// the inventory.
pub(in crate::gen_rom_profile) fn records(
    pack: &PackSource,
) -> Result<Vec<BattleEffectRecord>, GenRomProfileError> {
    let payload = pack.raw_payload(rom_import::BATTLE_ANIM_PACK_ID)?;
    let catalog =
        BattleAnimationCatalog::decode(payload).map_err(|err| GenRomProfileError::EntryShape {
            id: rom_import::BATTLE_ANIM_PACK_ID.to_owned(),
            reason: err.to_string(),
        })?;
    let records: Vec<BattleEffectRecord> = BattleEffectRecord::inventory(&catalog).collect();
    if records.is_empty() {
        return Err(GenRomProfileError::MissingPackEntry(format!("{PREFIX}*")));
    }

    let mut expected = BTreeSet::new();
    for record in &records {
        let (image_id, palette_id) = (record.image_id(), record.palette_id());
        let colors = check_palette(pack, &palette_id)?;
        check_sheet(pack, &image_id, colors)?;
        expected.insert(image_id);
        expected.insert(palette_id);
    }
    if let Some(surplus) = pack
        .ids_with_prefix(PREFIX)
        .into_iter()
        .find(|id| !expected.contains(id))
    {
        return Err(GenRomProfileError::StructMismatch {
            id: surplus,
            reason: "is not in the animation catalog's battle-effect inventory".to_owned(),
        });
    }
    Ok(records)
}

fn shape(id: &str, reason: String) -> GenRomProfileError {
    GenRomProfileError::EntryShape {
        id: id.to_owned(),
        reason,
    }
}

/// The palette's colour count, once its payload and bank count are known good.
fn check_palette(pack: &PackSource, id: &str) -> Result<u16, GenRomProfileError> {
    let (_, colors) = pack.get(id)?.palette_payload(id)?;
    if colors == 0 || !colors.is_multiple_of(BANK_COLORS) {
        return Err(shape(
            id,
            format!("{colors} colours is not a nonzero whole number of {BANK_COLORS}-colour banks"),
        ));
    }
    Ok(colors)
}

fn check_sheet(pack: &PackSource, id: &str, colors: u16) -> Result<(), GenRomProfileError> {
    let (pixels, _, _, depth) = pack.get(id)?.tiled_image_raster(id)?;
    if depth != BIT_DEPTH {
        return Err(shape(id, format!("is {depth}bpp: expected {BIT_DEPTH}bpp")));
    }
    if let Some(&index) = pixels.iter().find(|&&index| u16::from(index) >= colors) {
        return Err(shape(
            id,
            format!("holds palette index {index}, past its {colors}-colour palette"),
        ));
    }
    Ok(())
}
