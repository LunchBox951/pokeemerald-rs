//! Typed evolution-scene graphics: the animated background and the sparkle
//! sprite that `EvolutionScene` and `LoadEvoSparkleSpriteAndPal` load
//! (upstream `src/evolution_scene.c`, `src/evolution_graphics.c`).
//!
//! Pack ids stay private so scene code composes no strings. Every lookup
//! re-validates shapes, since a hand-built or corrupt pack can bypass
//! extraction. This record is the scene's *data* only: the Pokémon pictures,
//! the battle background, and the scene's timing are separate concerns.

use super::{AssetPack, ImageRef, PackError, PaletteRef, PalettedImage};

const BACKGROUND_IMAGE_ID: &str = "evolution-scene/image/bg";
const SPARKLE_IMAGE_ID: &str = "evolution-scene/image/evo_sparkle";
const INTRO_PALETTE_ID: &str = "evolution-scene/palette/bg_anim_intro";
const ANIMATION_PALETTE_ID: &str = "evolution-scene/palette/bg_anim";
const SPARKLE_PALETTE_ID: &str = "evolution-scene/palette/evo_sparkle";
const INNER_TILEMAP_ID: &str = "evolution-scene/raw/bg_inner";
const OUTER_TILEMAP_ID: &str = "evolution-scene/raw/bg_outer";

const BIT_DEPTH: u8 = 4;
const PALETTE_ENTRY_BYTES: usize = 2;
/// The intro palette file lists all 256 colours of an 8-bit palette, although
/// the scene copies only its first bank to the screen.
const INTRO_COLORS: u16 = 256;
/// The lookup table the background animation draws its colours from.
const ANIMATION_COLORS: u16 = 32;
const SPARKLE_COLORS: u16 = 16;
const BACKGROUND_SHAPE: (u32, u32) = (128, 112);
const SPARKLE_SHAPE: (u32, u32) = (8, 8);
/// A 32x32 background of two-byte tilemap entries.
const BACKGROUND_TILEMAP_BYTES: usize = 32 * 32 * 2;

/// Every graphic the evolution scene loads, borrowing the pack for `'a`.
///
/// [`AssetPack::evolution_scene`] returns it with every entry validated.
#[derive(Debug, Clone, Copy)]
pub struct EvolutionSceneHandle<'a> {
    /// The 128x112 background tile sheet shared by both background layers;
    /// its pixels are 4-bit indices.
    pub background: ImageRef<'a>,
    /// The inner layer's 32x32 tilemap (`bg_inner.bin`).
    pub inner_tilemap: &'a [u8],
    /// The outer layer's 32x32 tilemap (`bg_outer.bin`).
    pub outer_tilemap: &'a [u8],
    /// The intro palette file (`bg_anim_intro.pal`): all 256 colours it lists.
    /// The scene copies only the first 16 to the screen.
    pub intro_palette: PaletteRef<'a>,
    /// The 32-colour table (`bg_anim.pal`) the running animation draws the
    /// background's colours from.
    pub animation_palette: PaletteRef<'a>,
    /// The 8x8 sparkle sprite with its own palette.
    pub sparkle: PalettedImage<'a>,
}

impl AssetPack {
    /// Bundle every evolution-scene graphic.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedEvolutionSceneAsset`] when
    /// dimensions, bit depth (4 for both images), palette size, pixel indices,
    /// or tilemap length differ from upstream.
    pub fn evolution_scene(&self) -> Result<EvolutionSceneHandle<'_>, PackError> {
        let background = self.evolution_image(
            BACKGROUND_IMAGE_ID,
            INTRO_PALETTE_ID,
            BACKGROUND_SHAPE,
            INTRO_COLORS,
        )?;
        Ok(EvolutionSceneHandle {
            background: background.image,
            inner_tilemap: self.evolution_tilemap(INNER_TILEMAP_ID)?,
            outer_tilemap: self.evolution_tilemap(OUTER_TILEMAP_ID)?,
            intro_palette: background.palette,
            animation_palette: self.evolution_palette(ANIMATION_PALETTE_ID, ANIMATION_COLORS)?,
            sparkle: self.evolution_image(
                SPARKLE_IMAGE_ID,
                SPARKLE_PALETTE_ID,
                SPARKLE_SHAPE,
                SPARKLE_COLORS,
            )?,
        })
    }

    fn evolution_image(
        &self,
        image_id: &str,
        palette_id: &str,
        shape: (u32, u32),
        palette_colors: u16,
    ) -> Result<PalettedImage<'_>, PackError> {
        self.paired_image(
            image_id,
            palette_id,
            shape,
            BIT_DEPTH,
            palette_colors,
            |id, detail| PackError::MalformedEvolutionSceneAsset { id, detail },
        )
    }

    fn evolution_palette(&self, id: &str, colors: u16) -> Result<PaletteRef<'_>, PackError> {
        let palette = self.palette(id)?;
        if palette.color_count != colors
            || palette.raw.len() != usize::from(colors) * PALETTE_ENTRY_BYTES
        {
            return Err(PackError::MalformedEvolutionSceneAsset {
                id: id.to_owned(),
                detail: format!(
                    "declares {} colours in {} bytes: expected {colors} colours",
                    palette.color_count,
                    palette.raw.len()
                ),
            });
        }
        Ok(palette)
    }

    fn evolution_tilemap(&self, id: &str) -> Result<&[u8], PackError> {
        let bytes = self.raw(id)?;
        if bytes.len() != BACKGROUND_TILEMAP_BYTES {
            return Err(PackError::MalformedEvolutionSceneAsset {
                id: id.to_owned(),
                detail: format!(
                    "is {} bytes: expected {BACKGROUND_TILEMAP_BYTES}",
                    bytes.len()
                ),
            });
        }
        Ok(bytes)
    }
}
