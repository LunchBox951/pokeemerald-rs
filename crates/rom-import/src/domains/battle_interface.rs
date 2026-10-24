//! The battle textbox, menu text palettes, health-box elements, battle bar,
//! singles health-box frames, enemy shadow, and ball palettes:
//! `battle-interface/*`.
//!
//! The textbox sheet, its 32-colour palette, and the window-text palette are
//! LZ77 streams; the PP text palette is a raw `.gbapal` run; the textbox
//! tilemap is an LZ77 stream copied through as a blob. Every root is plain, so
//! each goes through the shared codecs.
//!
//! The textbox palette is a single ROM root. The build concatenates
//! `textbox_0.pal` and `textbox_1.pal` into `gBattleTextboxPalette`, so the
//! importer reads it as one 32-colour palette and invents no second asset.
//!
//! The health-box elements are raw 4bpp tile runs, copied through as blobs.
//! `gHealthboxElementsGfxTable` (`src/graphics.c:358-370`) is a single
//! uncompressed `INCBIN_U8` of thirteen `.4bpp` files, so the thirteen roots
//! are consecutive slices of one run. They are read as one family: the ids
//! must be the table's, in its order, each the length its source sheet's tile
//! count gives, each starting where the one before it ends. A profile that
//! disagrees is [`ImportError::StructMismatch`], so a reordered or relocated
//! record cannot pass for a table row. The battle bar is the one compressed
//! raw record: `gBattleInterfaceGfx_BattleBar` is the LZ77 stream of
//! `hpbar_anim_unused`, `numbers1`, and `numbers2` concatenated
//! (`graphics_file_rules.mk:90-93`).
//!
//! The singles frames and the enemy shadow are ordinary compressed images, and
//! the two ball palettes ordinary raw palettes, so they ride in the same
//! `images` and `palettes` slices as the textbox records and go through the
//! shared codecs. `gHealthboxSinglesPlayerGfx` and `gHealthboxSinglesOpponentGfx`
//! (`src/graphics.c:628-629`) are `.4bpp.lz` cut with `-mwidth 8 -mheight 8`
//! and `-mwidth 8 -mheight 4`; each root carries its own metatile shape, which
//! the image reader walks to rebuild the PNG raster order the checkout pack
//! holds. `gEnemyMonShadow_Gfx` (`graphics.c:1037`) is a plain `.4bpp.lz`, and
//! `gBattleInterface_BallStatusBarPal` and `gBattleInterface_BallDisplayPal`
//! (`graphics.c:354,356`) are uncompressed `.gbapal` runs.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::{BlobRoot, Encoding, Roots};

/// One 8x8 4bpp tile, in bytes.
pub const HEALTHBOX_TILE_BYTES: u32 = 32;
/// The table's name in an error.
const TABLE: &str = "gHealthboxElementsGfxTable";
/// The battle bar's name in an error.
const BATTLE_BAR_SYMBOL: &str = "gBattleInterfaceGfx_BattleBar";

/// The rows of `gHealthboxElementsGfxTable` in table order: each record's
/// pack id and its source sheet's tile count (`src/graphics.c:358-370`; the
/// sheets' sizes are in `graphics/battle_interface/*.png`).
pub const HEALTHBOX_ELEMENTS: [(&str, u32); 13] = [
    ("battle-interface/raw/healthbox_element/hpbar", 12),
    ("battle-interface/raw/healthbox_element/expbar", 9),
    ("battle-interface/raw/healthbox_element/status", 15),
    ("battle-interface/raw/healthbox_element/misc", 11),
    ("battle-interface/raw/healthbox_element/hpbar_anim", 18),
    ("battle-interface/raw/healthbox_element/misc_frameend", 1),
    ("battle-interface/raw/healthbox_element/ball_display", 4),
    (
        "battle-interface/raw/healthbox_element/ball_caught_indicator",
        1,
    ),
    ("battle-interface/raw/healthbox_element/status2", 15),
    ("battle-interface/raw/healthbox_element/status3", 15),
    ("battle-interface/raw/healthbox_element/status4", 15),
    (
        "battle-interface/raw/healthbox_element/healthbox_doubles_frameend",
        1,
    ),
    (
        "battle-interface/raw/healthbox_element/healthbox_doubles_frameend_bar",
        1,
    ),
];

/// The battle bar's pack id and tile count: `hpbar_anim_unused` (18),
/// `numbers1` (11), and `numbers2` (12) concatenated.
pub const HEALTHBOX_BATTLE_BAR: (&str, u32) = ("battle-interface/raw/battle_bar", 41);

/// Write every battle-interface entry.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no battle-interface roots writes nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises;
/// [`ImportError::StructMismatch`] if the health-box element roots are not the
/// table's thirteen records in order and back to back, or the battle bar is
/// not the compressed run of 41 tiles.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.battle_interface;
    check_elements(family.healthbox_elements)?;
    if let Some(bar) = &family.battle_bar {
        check_battle_bar(bar)?;
    }
    let mut entries = Vec::with_capacity(
        family.images.len()
            + family.palettes.len()
            + family.tilemaps.len()
            + family.healthbox_elements.len()
            + usize::from(family.battle_bar.is_some()),
    );
    for root in family.images {
        entries.push(super::image(&reader, root)?);
    }
    for root in family.palettes {
        entries.push(super::palette(&reader, root)?);
    }
    for root in family.tilemaps {
        entries.push(super::blob(&reader, root)?);
    }
    for root in family.healthbox_elements.iter().chain(&family.battle_bar) {
        entries.push(super::blob(&reader, root)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn table_mismatch(field: &'static str) -> ImportError {
    ImportError::StructMismatch { root: TABLE, field }
}

/// Corroborate the element roots against the table they are rows of.
///
/// An empty slice is a profile with no health-box roots and passes. Otherwise
/// all thirteen records must be present in table order, each raw and exactly
/// its sheet's tiles long, each beginning at the byte after its predecessor.
fn check_elements(elements: &[BlobRoot]) -> Result<(), ImportError> {
    if elements.is_empty() {
        return Ok(());
    }
    if elements.len() != HEALTHBOX_ELEMENTS.len() {
        return Err(table_mismatch("element count"));
    }
    let mut next: Option<u32> = None;
    for (root, (id, tiles)) in elements.iter().zip(HEALTHBOX_ELEMENTS) {
        if root.id != id {
            return Err(table_mismatch("element order"));
        }
        if root.encoding != Encoding::Raw || root.len != tiles * HEALTHBOX_TILE_BYTES {
            return Err(table_mismatch("element length"));
        }
        if next.is_some_and(|expected| root.addr.raw() != expected) {
            return Err(table_mismatch("element contiguity"));
        }
        next = Some(
            root.addr
                .raw()
                .checked_add(root.len)
                .ok_or_else(|| table_mismatch("element contiguity"))?,
        );
    }
    Ok(())
}

/// The battle bar is the compressed run of its three sheets' tiles.
fn check_battle_bar(bar: &BlobRoot) -> Result<(), ImportError> {
    let (id, tiles) = HEALTHBOX_BATTLE_BAR;
    if bar.id != id || bar.encoding != Encoding::Lz77 || bar.len != tiles * HEALTHBOX_TILE_BYTES {
        return Err(ImportError::StructMismatch {
            root: BATTLE_BAR_SYMBOL,
            field: "battle bar shape",
        });
    }
    Ok(())
}

#[cfg(test)]
mod frame_tests;
#[cfg(test)]
mod tests;
