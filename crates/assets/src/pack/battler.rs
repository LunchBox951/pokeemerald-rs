//! Static battler art for Treecko, Torchic, and Mudkip.
//! Starter inventory: `src/starter_choose.c:113-118`.
//!
//! Pack IDs stay private; [`AssetPack::species_battler`] validates each requested pair.

use super::{AssetPack, PackError, PalettedImage};
use crate::SpeciesId;

/// One mon-picture frame (`include/constants/pokemon.h:262-264`).
pub(super) const BATTLER_SIDE: u32 = 64;
const BATTLER_BIT_DEPTH: u8 = 4;
const BATTLER_PALETTE_COLORS: u16 = 1 << BATTLER_BIT_DEPTH;

/// Which side of a battler an image shows, for species and trainers alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattlerDirection {
    /// `gMonStillFrontPicTable` entry (`src/data/pokemon_graphics/still_front_pic_table.h:1`).
    Front,
    /// `gMonBackPicTable` entry (`src/data/pokemon_graphics/back_pic_table.h:1`).
    Back,
}

/// Explicit palette choice; shininess is not inferred from trainer ID or personality.
/// Upstream selection: `src/pokemon.c:6541-6552`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteVariant {
    /// `gMonPaletteTable` entry (`src/data/pokemon_graphics/palette_table.h:1`).
    Normal,
    /// `gMonShinyPaletteTable` entry (`src/data/pokemon_graphics/shiny_palette_table.h:1`).
    Shiny,
}

/// Requested identity and art returned by [`AssetPack::species_battler`].
#[derive(Debug, Clone, Copy)]
pub struct BattlerHandle<'a> {
    /// Lookup key, without species substitution.
    pub species: SpeciesId,
    /// Requested view, without front/back substitution.
    pub direction: BattlerDirection,
    /// Requested palette, without normal/shiny substitution.
    pub variant: PaletteVariant,
    /// 64x64 4bpp image paired with exactly 16 colours.
    pub art: PalettedImage<'a>,
}

/// Image and palette pack keys; `None` for every species outside the starter inventory.
pub(super) fn ids(
    species: SpeciesId,
    direction: BattlerDirection,
    variant: PaletteVariant,
) -> Option<(&'static str, &'static str)> {
    let (front_image_id, back_image_id, normal_palette_id, shiny_palette_id) = match species {
        SpeciesId::TREECKO => (
            "pokemon/treecko/front",
            "pokemon/treecko/back",
            "pokemon/treecko/palette/normal",
            "pokemon/treecko/palette/shiny",
        ),
        SpeciesId::TORCHIC => (
            "pokemon/torchic/front",
            "pokemon/torchic/back",
            "pokemon/torchic/palette/normal",
            "pokemon/torchic/palette/shiny",
        ),
        SpeciesId::MUDKIP => (
            "pokemon/mudkip/front",
            "pokemon/mudkip/back",
            "pokemon/mudkip/palette/normal",
            "pokemon/mudkip/palette/shiny",
        ),
        _ => return None,
    };
    let image_id = match direction {
        BattlerDirection::Front => front_image_id,
        BattlerDirection::Back => back_image_id,
    };
    let palette_id = match variant {
        PaletteVariant::Normal => normal_palette_id,
        PaletteVariant::Shiny => shiny_palette_id,
    };
    Some((image_id, palette_id))
}

impl AssetPack {
    /// Requested starter art, with the image-palette pair validated on every call.
    ///
    /// # Errors
    ///
    /// [`PackError::UnsupportedBattlerSpecies`] for species outside the starter inventory;
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for missing or wrong-kind entries;
    /// [`PackError::MalformedBattlerAsset`] for invalid image dimensions, source bit depth,
    /// palette size, or pixel indices. Payload lengths are checked by `pack_format` at load,
    /// so a loaded pack never reaches this method with a misshapen payload.
    pub fn species_battler(
        &self,
        species: SpeciesId,
        direction: BattlerDirection,
        variant: PaletteVariant,
    ) -> Result<BattlerHandle<'_>, PackError> {
        let (image_id, palette_id) = ids(species, direction, variant)
            .ok_or(PackError::UnsupportedBattlerSpecies(species))?;
        let art = self.paired_image(
            image_id,
            palette_id,
            (BATTLER_SIDE, BATTLER_SIDE),
            BATTLER_BIT_DEPTH,
            BATTLER_PALETTE_COLORS,
            |id, detail| PackError::MalformedBattlerAsset { id, detail },
        )?;
        Ok(BattlerHandle {
            species,
            direction,
            variant,
            art,
        })
    }
}
