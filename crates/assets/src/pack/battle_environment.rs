//! Typed checkpoint battle environments: the tall-grass and plain
//! backgrounds that `DrawMainBattleBackground` and `DrawBattleEntryBackground`
//! load (upstream `src/battle_bg.c`, `struct BattleBackground`).
//!
//! Pack ids stay private so scene code composes no strings. Every lookup
//! re-validates shapes, since a hand-built or corrupt pack can bypass
//! extraction. A handle is the record's *data* only: choosing an environment
//! from the map and the player's tile, drawing the layers, loading the palette
//! and running the entry animation are separate concerns.

use super::{AssetPack, ImageRef, PackError, PaletteRef};

const TALL_GRASS_TILES_ID: &str = "battle-environment/image/tall_grass/tiles";
const TALL_GRASS_TILEMAP_ID: &str = "battle-environment/raw/tall_grass/map";
const TALL_GRASS_ENTRY_TILES_ID: &str = "battle-environment/image/tall_grass/anim_tiles";
const TALL_GRASS_ENTRY_TILEMAP_ID: &str = "battle-environment/raw/tall_grass/anim_map";
const TALL_GRASS_PALETTE_ID: &str = "battle-environment/palette/tall_grass";
/// The plain environment aliases the building environment's art and differs
/// only in its palette.
const BUILDING_TILES_ID: &str = "battle-environment/image/building/tiles";
const BUILDING_TILEMAP_ID: &str = "battle-environment/raw/building/map";
const BUILDING_ENTRY_TILES_ID: &str = "battle-environment/image/building/anim_tiles";
const BUILDING_ENTRY_TILEMAP_ID: &str = "battle-environment/raw/building/anim_map";
const PLAIN_PALETTE_ID: &str = "battle-environment/palette/plain";

const BIT_DEPTH: u8 = 4;
/// Both tile sheets are 128x128 pixels: 256 tiles of 8x8.
const SHEET_SHAPE: (u32, u32) = (128, 128);
/// Three 16-colour banks, loaded from background palette bank 2.
const PALETTE_COLORS: u16 = 48;
const TILEMAP_ENTRY_BYTES: usize = 2;
/// The main background is BG3 at screen size 1: 64x32 tiles.
const MAIN_TILEMAP_TILES: (usize, usize) = (64, 32);
/// The entry background is a single 32x32 screenblock, not BG1's 32x64 canvas.
const ENTRY_TILEMAP_TILES: (usize, usize) = (32, 32);

/// Which checkpoint environment record to resolve.
///
/// This names a *record*; it does not select an environment from map or
/// battle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleEnvironment {
    /// `BATTLE_ENVIRONMENT_GRASS`.
    TallGrass,
    /// `BATTLE_ENVIRONMENT_PLAIN`.
    Plain,
}

/// The five `struct BattleBackground` members of one environment, borrowing
/// the pack for `'a`.
///
/// [`AssetPack::battle_environment`] returns it with every entry validated.
/// Tilemaps are little-endian GBA text-map entries in native screenblock
/// order, exactly as upstream decompresses them; they are not reordered.
#[derive(Debug, Clone, Copy)]
pub struct BattleEnvironmentHandle<'a> {
    /// The 128x128 main-background tile sheet; pixels are 4-bit indices.
    pub tiles: ImageRef<'a>,
    /// The main background's tilemap (64x32 entries).
    pub tilemap: &'a [u8],
    /// The 128x128 entry-background tile sheet; pixels are 4-bit indices.
    pub entry_tiles: ImageRef<'a>,
    /// The entry background's tilemap (one 32x32 screenblock).
    pub entry_tilemap: &'a [u8],
    /// The 48-colour palette (three 16-colour banks) both backgrounds select from.
    pub palette: PaletteRef<'a>,
}

impl BattleEnvironmentHandle<'_> {
    /// The main tilemap's size in tiles as `(columns, rows)`.
    #[must_use]
    pub const fn tilemap_dimensions(&self) -> (usize, usize) {
        MAIN_TILEMAP_TILES
    }

    /// The entry tilemap's size in tiles as `(columns, rows)`.
    #[must_use]
    pub const fn entry_tilemap_dimensions(&self) -> (usize, usize) {
        ENTRY_TILEMAP_TILES
    }
}

impl AssetPack {
    /// Bundle the five members of one battle environment.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedBattleEnvironmentAsset`] when a
    /// sheet is not 128x128 at 4 bits, a pixel index leaves the 4-bit range, the
    /// palette is not 48 colours, or a tilemap length differs from upstream.
    pub fn battle_environment(
        &self,
        environment: BattleEnvironment,
    ) -> Result<BattleEnvironmentHandle<'_>, PackError> {
        let ids = match environment {
            BattleEnvironment::TallGrass => [
                TALL_GRASS_TILES_ID,
                TALL_GRASS_TILEMAP_ID,
                TALL_GRASS_ENTRY_TILES_ID,
                TALL_GRASS_ENTRY_TILEMAP_ID,
                TALL_GRASS_PALETTE_ID,
            ],
            BattleEnvironment::Plain => [
                BUILDING_TILES_ID,
                BUILDING_TILEMAP_ID,
                BUILDING_ENTRY_TILES_ID,
                BUILDING_ENTRY_TILEMAP_ID,
                PLAIN_PALETTE_ID,
            ],
        };
        let [tiles, tilemap, entry_tiles, entry_tilemap, palette] = ids;
        let tiles = self.environment_sheet(tiles, palette)?;
        Ok(BattleEnvironmentHandle {
            tiles: tiles.image,
            tilemap: self.environment_tilemap(tilemap, MAIN_TILEMAP_TILES)?,
            entry_tiles: self.environment_sheet(entry_tiles, palette)?.image,
            entry_tilemap: self.environment_tilemap(entry_tilemap, ENTRY_TILEMAP_TILES)?,
            palette: tiles.palette,
        })
    }

    fn environment_sheet(
        &self,
        image_id: &str,
        palette_id: &str,
    ) -> Result<super::PalettedImage<'_>, PackError> {
        self.paired_image(
            image_id,
            palette_id,
            SHEET_SHAPE,
            BIT_DEPTH,
            PALETTE_COLORS,
            |id, detail| PackError::MalformedBattleEnvironmentAsset { id, detail },
        )
    }

    fn environment_tilemap(
        &self,
        id: &str,
        (columns, rows): (usize, usize),
    ) -> Result<&[u8], PackError> {
        let bytes = self.raw(id)?;
        let expected = columns * rows * TILEMAP_ENTRY_BYTES;
        if bytes.len() != expected {
            return Err(PackError::MalformedBattleEnvironmentAsset {
                id: id.to_owned(),
                detail: format!("is {} bytes: expected {expected}", bytes.len()),
            });
        }
        Ok(bytes)
    }
}
