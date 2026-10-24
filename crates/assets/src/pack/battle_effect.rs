//! Typed battle-effect sprite sheets and palettes, derived from the animation
//! catalog.
//!
//! Upstream registers one sheet in `gBattleAnimPicTable` and one palette in
//! `gBattleAnimPaletteTable` per `ANIM_TAG_*` (`src/data/battle_anim.h`);
//! `Cmd_loadspritegfx` loads that pair (`src/battle_anim.c`). The pack carries
//! exactly the pairs some catalog program loads, so the required inventory is
//! not a separate list: [`BattleEffectRecord::inventory`] walks the
//! [`AnimCommand::LoadSpriteGfx`] operands of a [`BattleAnimationCatalog`].
//!
//! The pack ids (`battle-effect/<tag>/image`, `battle-effect/<tag>/palette`)
//! are built here and only here. Producers (extraction, ROM import) take them
//! from a [`BattleEffectRecord`]; consumers call [`AssetPack::battle_effect`]
//! and never format an id. Every lookup re-validates shapes, since a hand-built
//! or corrupt pack can bypass extraction.

use std::collections::BTreeSet;

use super::{AssetPack, PackError, PalettedImage};
use crate::battle_anim::{
    AnimCommand, BattleAnimationCatalog, BattleAnimationProgram, EffectSpriteId,
};

/// Every battle-effect sheet is a 4bpp sprite sheet (`*.4bpp.lz` in
/// `src/data/battle_anim.h`).
const EFFECT_BIT_DEPTH: u8 = 4;
/// Colours in one 4bpp palette bank. A battle-effect palette is a whole number
/// of banks: all are one bank except `gBattleAnimSpritePal_MusicNotes2`, which
/// holds five (`graphics/battle_anims/sprites/music_notes_2.pal`;
/// `AnimTask_LoadMusicNotesPals` loads banks 0 to 2, `src/battle_anim_effects_2.c:3019-3035`).
const PALETTE_BANK_COLORS: u16 = 1 << EFFECT_BIT_DEPTH;
/// A GBA tile is 8x8 pixels, so a sheet's sides are whole tiles.
const TILE_SIDE: u32 = 8;

/// The sheet and palette one `loadspritegfx` reaches, as pack ids.
///
/// Built from an [`EffectSpriteId`], so the id scheme has one owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BattleEffectRecord {
    sprite: EffectSpriteId,
}

impl BattleEffectRecord {
    /// The record for `sprite`.
    #[must_use]
    pub const fn new(sprite: EffectSpriteId) -> Self {
        Self { sprite }
    }

    /// The effect sprite this record carries.
    #[must_use]
    pub const fn sprite(self) -> EffectSpriteId {
        self.sprite
    }

    /// The pack id of the sprite sheet, `battle-effect/<tag>/image`.
    #[must_use]
    pub fn image_id(self) -> String {
        format!("battle-effect/{:03}/image", self.sprite.get())
    }

    /// The pack id of the palette, `battle-effect/<tag>/palette`.
    #[must_use]
    pub fn palette_id(self) -> String {
        format!("battle-effect/{:03}/palette", self.sprite.get())
    }

    /// The records `catalog` requires: one per distinct
    /// [`AnimCommand::LoadSpriteGfx`] operand across all its programs, in
    /// [`EffectSpriteId`] order.
    ///
    /// The order is the sprite id's, not the programs' or the commands', so it
    /// is stable for any catalog holding the same set.
    pub fn inventory(
        catalog: &BattleAnimationCatalog,
    ) -> impl ExactSizeIterator<Item = Self> + DoubleEndedIterator {
        let sprites: BTreeSet<EffectSpriteId> = catalog
            .programs()
            .iter()
            .flat_map(BattleAnimationProgram::commands)
            .filter_map(|command| match command {
                AnimCommand::LoadSpriteGfx(sprite) => Some(*sprite),
                _ => None,
            })
            .collect();
        sprites.into_iter().map(Self::new)
    }
}

/// One effect sprite sheet and its palette, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct BattleEffectHandle<'a> {
    /// The effect sprite identity.
    pub sprite: EffectSpriteId,
    /// The 4bpp sheet with its palette, a whole number of 16-colour banks.
    pub art: PalettedImage<'a>,
}

impl AssetPack {
    /// One battle-effect sprite sheet paired with its palette, validated.
    ///
    /// The sheet's dimensions vary per effect, so the check requires only
    /// whole 8x8 tiles; it does not pin a per-sprite size. Likewise the palette
    /// must be a non-zero whole number of 16-colour banks, not one fixed length.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind half, named by its id;
    /// [`PackError::MalformedBattleEffectAsset`] when the sheet is empty or not
    /// whole tiles, is not 4-bit, has a payload other than one byte per pixel,
    /// when the palette is not a non-zero multiple of 16 colours, or when a
    /// pixel index falls outside the palette.
    pub fn battle_effect(
        &self,
        sprite: EffectSpriteId,
    ) -> Result<BattleEffectHandle<'_>, PackError> {
        let record = BattleEffectRecord::new(sprite);
        let (image_id, palette_id) = (record.image_id(), record.palette_id());
        let sheet = self.image(&image_id)?;
        let whole_tiles = |side: u32| side != 0 && side.is_multiple_of(TILE_SIDE);
        if !whole_tiles(sheet.width) || !whole_tiles(sheet.height) {
            return Err(PackError::MalformedBattleEffectAsset {
                id: image_id,
                detail: format!(
                    "is {}x{}: expected whole {TILE_SIDE}x{TILE_SIDE} tiles",
                    sheet.width, sheet.height
                ),
            });
        }
        let colors = self.palette(&palette_id)?.color_count;
        if colors == 0 || !colors.is_multiple_of(PALETTE_BANK_COLORS) {
            return Err(PackError::MalformedBattleEffectAsset {
                id: palette_id,
                detail: format!(
                    "declares {colors} colours: expected a whole number of \
                     {PALETTE_BANK_COLORS}-colour banks"
                ),
            });
        }
        let art = self.paired_image(
            &image_id,
            &palette_id,
            (sheet.width, sheet.height),
            EFFECT_BIT_DEPTH,
            colors,
            |id, detail| PackError::MalformedBattleEffectAsset { id, detail },
        )?;
        Ok(BattleEffectHandle { sprite, art })
    }
}
