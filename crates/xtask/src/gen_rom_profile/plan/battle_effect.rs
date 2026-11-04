//! The battle-effect sheets and palettes the animation catalog loads, owned.

use crate::gen_rom_profile::plan::{ImagePlan, PalettePlan};

/// One effect sprite's sheet and palette, owned.
#[derive(Debug, Clone)]
pub struct BattleEffectRecordPlan {
    /// The effect sprite index: the row of both registration tables.
    pub sprite: u16,
    /// The `size` the sheet's registration row carries, in bytes.
    pub sheet_bytes: u16,
    /// The sheet.
    pub sheet: ImagePlan,
    /// The palette.
    pub palette: PalettePlan,
}

/// The battle-effect roots, owned.
#[derive(Debug, Clone)]
pub struct BattleEffectPlan {
    /// `gBattleAnimPicTable`.
    pub sheet_table: u32,
    /// `gBattleAnimPaletteTable`.
    pub palette_table: u32,
    /// One record per effect sprite the catalog loads, by ascending index.
    pub records: Vec<BattleEffectRecordPlan>,
}
