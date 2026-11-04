//! The battle-effect sprite sheets and palettes the animation catalog loads.

use crate::reader::GbaPtr;
use crate::roots::{ImageRoot, PaletteRoot};

/// One effect sprite's sheet and palette, and the registration size its sheet
/// row carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleEffectRoot {
    /// The effect sprite index: the row of both registration tables, and the
    /// tag's offset from `ANIM_SPRITES_START`.
    pub sprite: u16,
    /// The `size` the sheet row registers, in bytes. It is what the engine
    /// loads, and not always the length of the sheet's tile data.
    pub sheet_bytes: u16,
    /// The sheet `gBattleAnimPicTable[sprite]` points at.
    pub sheet: ImageRoot,
    /// The palette `gBattleAnimPaletteTable[sprite]` points at.
    pub palette: PaletteRoot,
}

/// The battle-effect sheets and palettes (`src/battle_anim.c`).
///
/// The two tables are recorded so a reader can corroborate every record's row
/// by pointer, size and tag rather than trust the addresses alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleEffectRoots {
    /// `gBattleAnimPicTable`: 8-byte `{data, size, tag}` rows.
    pub sheet_table: GbaPtr,
    /// `gBattleAnimPaletteTable`: 8-byte `{data, tag}` rows.
    pub palette_table: GbaPtr,
    /// One record per effect sprite the catalog loads, by ascending index.
    pub records: &'static [BattleEffectRoot],
}

impl BattleEffectRoots {
    /// No battle-effect roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        sheet_table: GbaPtr::AT_BASE,
        palette_table: GbaPtr::AT_BASE,
        records: &[],
    };
}
