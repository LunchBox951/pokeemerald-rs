//! Typed summary-screen graphics (upstream `graphics/summary_screen/`,
//! `src/graphics.c:1125-1134` and `:1253-1255`, `src/pokemon_summary_screen.c`).
//!
//! The summary screen draws one background (a 4bpp tile sheet over eight
//! palette banks, with a 32x32 screen per page), the move selector's sprite
//! sheet, three move-effect tilemaps, a status tilemap, a marking palette, and
//! the A and B button sheets blitted into windows. This module returns each
//! record validated against upstream's shape; it draws nothing. Pack ids stay
//! private so scene code composes no strings, and every lookup re-validates
//! shapes, since a hand-built or corrupt pack can bypass extraction.
//!
//! The maps are 16-bit screen entries as upstream stores them. The battle and
//! contest move-effect maps hold 80 entries and the cancel map 64, though the
//! sliding window that draws them reads fewer (`pokemon_summary_screen.c:388-405`);
//! each is returned in full. Sheets validate through
//! [`AssetPack::bank_image`] and palettes through [`AssetPack::sized_palette`],
//! so the 128-colour background palette is never folded onto one bank, and
//! the button sheets carry no palette of their own.

use super::{AssetPack, ImageRef, PackError, PaletteRef};

const IMAGE_PREFIX: &str = "summary-screen-graphics/image";
const PALETTE_PREFIX: &str = "summary-screen-graphics/palette";
const RAW_PREFIX: &str = "summary-screen-graphics/raw";

/// Bytes of one 16-bit screen entry.
const ENTRY_BYTES: usize = 2;
/// A page tilemap: a 32x32 screen.
const PAGE_TILEMAP_BYTES: usize = 32 * 32 * ENTRY_BYTES;
/// `status_tilemap.bin`: a 10x2 screen.
const STATUS_TILEMAP_BYTES: usize = 10 * 2 * ENTRY_BYTES;
/// Colours in one 4bpp bank.
const BANK_COLORS: u16 = 16;

/// One of the summary screen's 4bpp sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryScreenSheet {
    /// `gSummaryScreen_Gfx`: the 240-tile background, 128x120.
    Tiles,
    /// `gSummaryMoveSelect_Gfx`: the 32-tile move-selector sprite sheet, 16x128.
    MoveSelect,
    /// `sButtons_Gfx[0]`: the A button, four tiles, 16x16.
    AButton,
    /// `sButtons_Gfx[1]`: the B button, four tiles, 16x16.
    BButton,
}

impl SummaryScreenSheet {
    /// Every sheet in extraction order.
    pub const ALL: [Self; 4] = [Self::Tiles, Self::MoveSelect, Self::AButton, Self::BButton];

    /// The sheet's width and height in pixels.
    #[must_use]
    pub const fn size(self) -> (u32, u32) {
        match self {
            Self::Tiles => (128, 120),
            Self::MoveSelect => (16, 128),
            Self::AButton | Self::BButton => (16, 16),
        }
    }

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Tiles => "tiles",
            Self::MoveSelect => "move_select",
            Self::AButton => "a_button",
            Self::BButton => "b_button",
        }
    }
}

/// One of the summary screen's palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryScreenPalette {
    /// `gSummaryScreen_Pal`: eight 16-colour banks (128 colours), loaded over
    /// the background's banks.
    Tiles,
    /// `gSummaryMoveSelect_Pal`: one bank.
    MoveSelect,
    /// `sMarkings_Pal`: one bank for the marking sprites.
    Markings,
}

impl SummaryScreenPalette {
    /// Every palette in extraction order.
    pub const ALL: [Self; 3] = [Self::Tiles, Self::MoveSelect, Self::Markings];

    /// The palette's colour count.
    #[must_use]
    pub const fn color_count(self) -> u16 {
        match self {
            Self::Tiles => 8 * BANK_COLORS,
            Self::MoveSelect | Self::Markings => BANK_COLORS,
        }
    }

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Tiles => "tiles",
            Self::MoveSelect => "move_select",
            Self::Markings => "markings",
        }
    }
}

/// One summary page's 32x32 screen (`src/graphics.c:1130-1134`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryScreenPage {
    /// `gSummaryPage_Info_Tilemap`.
    Info,
    /// `gSummaryPage_InfoEgg_Tilemap`.
    InfoEgg,
    /// `gSummaryPage_Skills_Tilemap`.
    Skills,
    /// `gSummaryPage_BattleMoves_Tilemap`.
    BattleMoves,
    /// `gSummaryPage_ContestMoves_Tilemap`.
    ContestMoves,
}

impl SummaryScreenPage {
    /// Every page in pack order: the info pages, then skills and the move pages.
    pub const ALL: [Self; 5] = [
        Self::Info,
        Self::InfoEgg,
        Self::Skills,
        Self::BattleMoves,
        Self::ContestMoves,
    ];

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Info => "page_info",
            Self::InfoEgg => "page_info_egg",
            Self::Skills => "page_skills",
            Self::BattleMoves => "page_battle_moves",
            Self::ContestMoves => "page_contest_moves",
        }
    }
}

/// One of the move-effect tilemaps (`src/graphics.c:1253-1255`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryScreenMoveEffect {
    /// `gSummaryScreen_MoveEffect_Battle_Tilemap`.
    Battle,
    /// `gSummaryScreen_MoveEffect_Contest_Tilemap`.
    Contest,
    /// `gSummaryScreen_MoveEffect_Cancel_Tilemap`.
    Cancel,
}

impl SummaryScreenMoveEffect {
    /// Every effect map in upstream declaration order.
    pub const ALL: [Self; 3] = [Self::Battle, Self::Contest, Self::Cancel];

    /// The map's entry count as stored: 80, 80, and 64 `u16` entries.
    #[must_use]
    pub const fn entry_count(self) -> usize {
        match self {
            Self::Battle | Self::Contest => 80,
            Self::Cancel => 64,
        }
    }

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Battle => "effect_battle",
            Self::Contest => "effect_contest",
            Self::Cancel => "effect_cancel",
        }
    }
}

/// Every summary-screen graphic, borrowing the pack for `'a`.
///
/// [`AssetPack::summary_screen_graphics`] returns it with every entry validated.
#[derive(Debug, Clone, Copy)]
pub struct SummaryScreenGraphicsHandle<'a> {
    sheets: [ImageRef<'a>; 4],
    palettes: [PaletteRef<'a>; 3],
    pages: [&'a [u8]; 5],
    effects: [&'a [u8]; 3],
    /// The 10x2 status screen as little-endian 16-bit entries.
    pub status_tilemap: &'a [u8],
}

impl<'a> SummaryScreenGraphicsHandle<'a> {
    /// The 4bpp sheet `sheet` names.
    #[must_use]
    pub fn sheet(&self, sheet: SummaryScreenSheet) -> ImageRef<'a> {
        self.sheets[sheet as usize]
    }

    /// The palette `palette` names.
    #[must_use]
    pub fn palette(&self, palette: SummaryScreenPalette) -> PaletteRef<'a> {
        self.palettes[palette as usize]
    }

    /// The 32x32 screen of `page` as little-endian 16-bit entries.
    #[must_use]
    pub fn page_tilemap(&self, page: SummaryScreenPage) -> &'a [u8] {
        self.pages[page as usize]
    }

    /// The tilemap of `effect` as little-endian 16-bit entries.
    #[must_use]
    pub fn move_effect_tilemap(&self, effect: SummaryScreenMoveEffect) -> &'a [u8] {
        self.effects[effect as usize]
    }
}

fn malformed(id: String, detail: String) -> PackError {
    PackError::MalformedSummaryScreenGraphicsAsset { id, detail }
}

impl AssetPack {
    /// Bundle every summary-screen graphic.
    ///
    /// # Errors
    ///
    /// The first failure of the individual readers: [`PackError::UnknownAsset`]
    /// or [`PackError::WrongKind`] for a missing or wrong-kind entry,
    /// [`PackError::MalformedSummaryScreenGraphicsAsset`] when a shape differs
    /// from upstream.
    pub fn summary_screen_graphics(&self) -> Result<SummaryScreenGraphicsHandle<'_>, PackError> {
        Ok(SummaryScreenGraphicsHandle {
            sheets: [
                self.summary_screen_sheet(SummaryScreenSheet::Tiles)?,
                self.summary_screen_sheet(SummaryScreenSheet::MoveSelect)?,
                self.summary_screen_sheet(SummaryScreenSheet::AButton)?,
                self.summary_screen_sheet(SummaryScreenSheet::BButton)?,
            ],
            palettes: [
                self.summary_screen_palette(SummaryScreenPalette::Tiles)?,
                self.summary_screen_palette(SummaryScreenPalette::MoveSelect)?,
                self.summary_screen_palette(SummaryScreenPalette::Markings)?,
            ],
            pages: [
                self.summary_screen_page_tilemap(SummaryScreenPage::Info)?,
                self.summary_screen_page_tilemap(SummaryScreenPage::InfoEgg)?,
                self.summary_screen_page_tilemap(SummaryScreenPage::Skills)?,
                self.summary_screen_page_tilemap(SummaryScreenPage::BattleMoves)?,
                self.summary_screen_page_tilemap(SummaryScreenPage::ContestMoves)?,
            ],
            effects: [
                self.summary_screen_move_effect_tilemap(SummaryScreenMoveEffect::Battle)?,
                self.summary_screen_move_effect_tilemap(SummaryScreenMoveEffect::Contest)?,
                self.summary_screen_move_effect_tilemap(SummaryScreenMoveEffect::Cancel)?,
            ],
            status_tilemap: self.summary_screen_status_tilemap()?,
        })
    }

    /// One 4bpp sheet, checked against its declared size and one 16-colour bank.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedSummaryScreenGraphicsAsset`] when
    /// the image is not its sheet's size at 4 bits with one byte per pixel, or a
    /// pixel index falls outside one bank.
    pub fn summary_screen_sheet(
        &self,
        sheet: SummaryScreenSheet,
    ) -> Result<ImageRef<'_>, PackError> {
        self.bank_image(
            &format!("{IMAGE_PREFIX}/{}", sheet.pack_name()),
            sheet.size(),
            malformed,
        )
    }

    /// One palette, checked against its colour count.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedSummaryScreenGraphicsAsset`] when
    /// the colour count or its bytes differ from upstream's.
    pub fn summary_screen_palette(
        &self,
        palette: SummaryScreenPalette,
    ) -> Result<PaletteRef<'_>, PackError> {
        self.sized_palette(
            &format!("{PALETTE_PREFIX}/{}", palette.pack_name()),
            palette.color_count(),
            malformed,
        )
    }

    /// One page's 32x32 screen entries.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedSummaryScreenGraphicsAsset`] when
    /// its length is not 2,048 bytes.
    pub fn summary_screen_page_tilemap(&self, page: SummaryScreenPage) -> Result<&[u8], PackError> {
        self.summary_screen_raw(page.pack_name(), PAGE_TILEMAP_BYTES)
    }

    /// One move-effect tilemap, every stored entry.
    ///
    /// # Errors
    ///
    /// As [`AssetPack::summary_screen_page_tilemap`], for the effect's stored
    /// length.
    pub fn summary_screen_move_effect_tilemap(
        &self,
        effect: SummaryScreenMoveEffect,
    ) -> Result<&[u8], PackError> {
        self.summary_screen_raw(effect.pack_name(), effect.entry_count() * ENTRY_BYTES)
    }

    /// The 10x2 status screen entries.
    ///
    /// # Errors
    ///
    /// As [`AssetPack::summary_screen_page_tilemap`], for a length other than 40.
    pub fn summary_screen_status_tilemap(&self) -> Result<&[u8], PackError> {
        self.summary_screen_raw("status_tilemap", STATUS_TILEMAP_BYTES)
    }

    fn summary_screen_raw(&self, name: &str, expected_len: usize) -> Result<&[u8], PackError> {
        let id = format!("{RAW_PREFIX}/{name}");
        let bytes = self.raw(&id)?;
        if bytes.len() != expected_len {
            return Err(malformed(
                id,
                format!("is {} bytes: expected {expected_len}", bytes.len()),
            ));
        }
        Ok(bytes)
    }
}
