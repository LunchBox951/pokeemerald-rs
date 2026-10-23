//! Typed static battler art (upstream `gMonStillFrontPicTable`, `gMonBackPicTable`,
//! `gMonPaletteTable`, `gMonShinyPaletteTable`).
//!
//! The pack ids stay private so battle code composes no strings. Only the
//! species the pack carries resolve; every lookup re-validates shapes, since
//! a hand-built or corrupt pack can bypass extraction. Which palette a
//! battler draws is the caller's choice, as upstream picks it by species and
//! personality (`GetMonSpritePalFromSpeciesAndPersonality`).

use super::{AssetPack, PackError, PalettedImage};
use crate::SpeciesId;

/// Width and height of every battler image: one 64x64 mon pic (`MON_PIC_WIDTH`).
pub(super) const BATTLER_SIDE: u32 = 64;
const BATTLER_BIT_DEPTH: u8 = 4;
const BATTLER_PALETTE_COLORS: u16 = 16;

/// Which side of the battler an image shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattlerDirection {
    /// The opposing side's art (`gMonStillFrontPicTable`).
    Front,
    /// The player's side's art (`gMonBackPicTable`).
    Back,
}

/// Which colouring a battler draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteVariant {
    /// The species palette (`gMonPaletteTable`).
    Normal,
    /// The shiny palette (`gMonShinyPaletteTable`).
    Shiny,
}

/// One species' battler image and palette, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct BattlerHandle<'a> {
    /// The species the art belongs to.
    pub species: SpeciesId,
    /// Which side the image shows.
    pub direction: BattlerDirection,
    /// Which palette the image is paired with.
    pub variant: PaletteVariant,
    /// The 64x64 image with its 16-colour palette.
    pub art: PalettedImage<'a>,
}

/// The species whose battler art the pack carries, with their asset directory.
const SPECIES: [(SpeciesId, &str); 3] = [
    (SpeciesId::TREECKO, "treecko"),
    (SpeciesId::TORCHIC, "torchic"),
    (SpeciesId::MUDKIP, "mudkip"),
];

/// Image and palette ids for one battler, or `None` for a species without art.
pub(super) fn ids(
    species: SpeciesId,
    direction: BattlerDirection,
    variant: PaletteVariant,
) -> Option<(&'static str, &'static str)> {
    let index = SPECIES.iter().position(|&(id, _)| id == species)?;
    Some(match (index, direction, variant) {
        (0, BattlerDirection::Front, PaletteVariant::Normal) => {
            ("pokemon/treecko/front", "pokemon/treecko/palette/normal")
        }
        (0, BattlerDirection::Front, PaletteVariant::Shiny) => {
            ("pokemon/treecko/front", "pokemon/treecko/palette/shiny")
        }
        (0, BattlerDirection::Back, PaletteVariant::Normal) => {
            ("pokemon/treecko/back", "pokemon/treecko/palette/normal")
        }
        (0, BattlerDirection::Back, PaletteVariant::Shiny) => {
            ("pokemon/treecko/back", "pokemon/treecko/palette/shiny")
        }
        (1, BattlerDirection::Front, PaletteVariant::Normal) => {
            ("pokemon/torchic/front", "pokemon/torchic/palette/normal")
        }
        (1, BattlerDirection::Front, PaletteVariant::Shiny) => {
            ("pokemon/torchic/front", "pokemon/torchic/palette/shiny")
        }
        (1, BattlerDirection::Back, PaletteVariant::Normal) => {
            ("pokemon/torchic/back", "pokemon/torchic/palette/normal")
        }
        (1, BattlerDirection::Back, PaletteVariant::Shiny) => {
            ("pokemon/torchic/back", "pokemon/torchic/palette/shiny")
        }
        (_, BattlerDirection::Front, PaletteVariant::Normal) => {
            ("pokemon/mudkip/front", "pokemon/mudkip/palette/normal")
        }
        (_, BattlerDirection::Front, PaletteVariant::Shiny) => {
            ("pokemon/mudkip/front", "pokemon/mudkip/palette/shiny")
        }
        (_, BattlerDirection::Back, PaletteVariant::Normal) => {
            ("pokemon/mudkip/back", "pokemon/mudkip/palette/normal")
        }
        (_, BattlerDirection::Back, PaletteVariant::Shiny) => {
            ("pokemon/mudkip/back", "pokemon/mudkip/palette/shiny")
        }
    })
}

impl AssetPack {
    /// One species' battler image paired with a palette, validated.
    ///
    /// # Errors
    ///
    /// [`PackError::UnsupportedBattlerSpecies`] for a species the pack has no
    /// battler art for; [`PackError::UnknownAsset`] or [`PackError::WrongKind`]
    /// for a missing or wrong-kind entry; [`PackError::MalformedBattlerAsset`]
    /// when dimensions, source bit depth, palette size, or pixel indices differ
    /// from upstream.
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
