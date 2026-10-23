//! Typed static battler art (upstream `gMonStillFrontPicTable`, `gMonBackPicTable`,
//! `gMonPaletteTable`, `gMonShinyPaletteTable`).
//!
//! The pack ids stay private so battle code composes no strings. Only the
//! species the pack carries resolve; every lookup re-validates shapes, since
//! a hand-built or corrupt pack can bypass extraction. Which palette a
//! battler draws is the caller's choice, as upstream picks it by species and
//! personality (`GetMonSpritePalFromSpeciesAndPersonality`).

use super::starter_chooser::PalettedImage;
use super::{AssetPack, PackError};
use crate::SpeciesId;

/// Width and height of every battler picture: one 64x64 mon pic (`MON_PIC_WIDTH`).
pub(super) const BATTLER_SIDE: u32 = 64;
/// Rows of Poochyena's stored still-front image, which upstream draws as a
/// 64x256 sheet: only the first `BATTLER_SIDE` rows are the picture, since
/// every table row loads `MON_PIC_SIZE` bytes (`still_front_pic_table.h:292`).
const POOCHYENA_FRONT_SHEET_HEIGHT: u32 = 256;
const BATTLER_PICTURE_PIXELS: usize = 64 * 64;
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

/// The pack ids of one species' battler art.
struct BattlerArt {
    species: SpeciesId,
    /// Rows of the stored front image; the picture is its first
    /// [`BATTLER_SIDE`] rows.
    front_height: u32,
    front: &'static str,
    back: &'static str,
    normal_palette: &'static str,
    shiny_palette: &'static str,
}

/// Names the pack ids of `$species` from its asset directory `$name`.
macro_rules! battler_art {
    ($species:ident, $name:literal) => {
        battler_art!($species, $name, BATTLER_SIDE)
    };
    ($species:ident, $name:literal, $front_height:expr) => {
        BattlerArt {
            species: SpeciesId::$species,
            front_height: $front_height,
            front: concat!("pokemon/", $name, "/front"),
            back: concat!("pokemon/", $name, "/back"),
            normal_palette: concat!("pokemon/", $name, "/palette/normal"),
            shiny_palette: concat!("pokemon/", $name, "/palette/shiny"),
        }
    };
}

/// The species whose battler art the pack carries.
const ARTS: [BattlerArt; 14] = [
    battler_art!(TREECKO, "treecko"),
    battler_art!(TORCHIC, "torchic"),
    battler_art!(MUDKIP, "mudkip"),
    battler_art!(POOCHYENA, "poochyena", POOCHYENA_FRONT_SHEET_HEIGHT),
    battler_art!(MIGHTYENA, "mightyena"),
    battler_art!(WURMPLE, "wurmple"),
    battler_art!(SILCOON, "silcoon"),
    battler_art!(BEAUTIFLY, "beautifly"),
    battler_art!(CASCOON, "cascoon"),
    battler_art!(DUSTOX, "dustox"),
    battler_art!(ZIGZAGOON, "zigzagoon"),
    battler_art!(LINOONE, "linoone"),
    battler_art!(WINGULL, "wingull"),
    battler_art!(PELIPPER, "pelipper"),
];

fn art_of(species: SpeciesId) -> Option<&'static BattlerArt> {
    ARTS.iter().find(|art| art.species == species)
}

/// The image and palette ids of one species' battler.
fn art_ids(
    art: &BattlerArt,
    direction: BattlerDirection,
    variant: PaletteVariant,
) -> (&'static str, &'static str) {
    let image = match direction {
        BattlerDirection::Front => art.front,
        BattlerDirection::Back => art.back,
    };
    let palette = match variant {
        PaletteVariant::Normal => art.normal_palette,
        PaletteVariant::Shiny => art.shiny_palette,
    };
    (image, palette)
}

/// Image and palette ids for one battler, or `None` for a species without art.
pub(super) fn ids(
    species: SpeciesId,
    direction: BattlerDirection,
    variant: PaletteVariant,
) -> Option<(&'static str, &'static str)> {
    art_of(species).map(|art| art_ids(art, direction, variant))
}

/// Rows of the image stored under `direction`, which is taller than the
/// picture for the one sheet upstream draws that way.
fn stored_height(art: &BattlerArt, direction: BattlerDirection) -> u32 {
    match direction {
        BattlerDirection::Front => art.front_height,
        BattlerDirection::Back => BATTLER_SIDE,
    }
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
        let battler = art_of(species).ok_or(PackError::UnsupportedBattlerSpecies(species))?;
        let (image_id, palette_id) = art_ids(battler, direction, variant);
        let mut art = self.paired_image(
            image_id,
            palette_id,
            (BATTLER_SIDE, stored_height(battler, direction)),
            BATTLER_BIT_DEPTH,
            BATTLER_PALETTE_COLORS,
            |id, detail| PackError::MalformedBattlerAsset { id, detail },
        )?;
        // Validation covered the whole stored image; the picture is its top rows.
        art.image.height = BATTLER_SIDE;
        art.image.pixels = &art.image.pixels[..BATTLER_PICTURE_PIXELS];
        Ok(BattlerHandle {
            species,
            direction,
            variant,
            art,
        })
    }
}
