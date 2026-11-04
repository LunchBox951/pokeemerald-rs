//! The checkpoint battle environments' roots: the tall-grass and plain rows of
//! `sBattleEnvironmentTable`.

use crate::reader::GbaPtr;
use crate::roots::{BlobRoot, ImageRoot, PaletteRoot};

/// One environment's `struct BattleBackground`: the five members of its row.
///
/// The members sit in the struct's own declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleEnvironmentRoot {
    /// `tileset`: the 128x128 main-background tile sheet.
    pub tiles: ImageRoot,
    /// `tilemap`: the main background's 64x32 tilemap.
    pub tilemap: BlobRoot,
    /// `entryTileset`: the 128x128 entry-background tile sheet.
    pub entry_tiles: ImageRoot,
    /// `entryTilemap`: the entry background's one-screenblock tilemap.
    pub entry_tilemap: BlobRoot,
    /// `palette`: the 48-colour palette.
    pub palette: PaletteRoot,
}

/// The checkpoint battle environments' roots (`src/battle_bg.c`).
///
/// The table is recorded so a reader can corroborate each row's five pointers
/// instead of trusting the addresses alone: plain's row aliases the building
/// environment's art, so a stray copy of a stream would otherwise pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleEnvironmentRoots {
    /// `sBattleEnvironmentTable`: 20-byte `BattleBackground` rows.
    pub table: GbaPtr,
    /// `BATTLE_ENVIRONMENT_GRASS`, or `None` for a profile that carries none.
    pub tall_grass: Option<BattleEnvironmentRoot>,
    /// `BATTLE_ENVIRONMENT_PLAIN`, or `None` for a profile that carries none.
    pub plain: Option<BattleEnvironmentRoot>,
}

impl BattleEnvironmentRoots {
    /// No battle-environment roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        table: GbaPtr::AT_BASE,
        tall_grass: None,
        plain: None,
    };

    /// The records present, tall grass first.
    pub fn records(&self) -> impl Iterator<Item = &BattleEnvironmentRoot> {
        self.tall_grass.iter().chain(self.plain.iter())
    }
}
