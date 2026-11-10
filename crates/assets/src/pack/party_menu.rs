//! Typed party-menu graphics (upstream `graphics/party_menu/`,
//! `src/graphics.c:1111-1117`, `src/data/party_menu.h`).
//!
//! The party screen draws one background (a 4bpp tile sheet over eleven palette
//! banks, with its screen), the menu Poké Ball and held-item sprite sheets, the
//! small Poké Ball sheet the Confirm and Cancel buttons draw, five slot-panel
//! tilemaps, and two button tilemaps. This module returns each record
//! validated against upstream's shape; it draws nothing. Pack ids stay private so
//! scene code composes no strings, and every lookup re-validates shapes, since a
//! hand-built or corrupt pack can bypass extraction.
//!
//! The map encodings differ and are preserved as upstream builds them: the
//! background screen (`bg.bin`) and the two buttons are 16-bit screen entries,
//! while the five slot panels are `u8` tile indices (`INCBIN_U8`), so a slot is
//! never read as screen entries.

use super::{AssetPack, ImageRef, PackError, PaletteRef, PalettedImage};

const IMAGE_PREFIX: &str = "party-menu/image";
const PALETTE_PREFIX: &str = "party-menu/palette";
const RAW_PREFIX: &str = "party-menu/raw";

/// The background sheet: the first 62 tiles of `bg.png` as an 8-wide strip
/// (`-num_tiles 62`, `src/graphics.c:1111`).
const BACKGROUND_SHEET_SIZE: (u32, u32) = (8, 62 * 8);
/// `bg.png`'s embedded palette: eleven 16-colour banks
/// (`LoadPalette(gPartyMenuBg_Pal, 0, 11 * 16)`-sized, `party_menu.c:737-750`).
const BACKGROUND_PALETTE_COLORS: u16 = 11 * 16;
/// `bg.bin`: a 32x32 screen of 2-byte entries.
const BACKGROUND_TILEMAP_BYTES: usize = 32 * 32 * 2;
/// `pokeball.png`: thirty-two 4bpp tiles, 32x64.
const POKE_BALL_SHEET_SIZE: (u32, u32) = (32, 64);
/// `hold_icons.png`: two 4bpp tiles, 8x16.
const HELD_ITEM_SHEET_SIZE: (u32, u32) = (8, 16);
/// `pokeball_small.png`: twenty-four 4bpp tiles, 16x96
/// (`gPartyMenuPokeballSmall_Gfx`, `src/graphics.c:1116`).
const POKE_BALL_SMALL_SHEET_SIZE: (u32, u32) = (16, 96);
const BIT_DEPTH: u8 = 4;
/// Colours in one 4bpp bank, the size of the Poké Ball and held-item palettes.
const BANK_COLORS: u16 = 16;
/// A button tilemap: 7x2 screen entries of 2 bytes.
const BUTTON_TILEMAP_BYTES: usize = 7 * 2 * 2;

/// One party slot panel's tilemap, declared in `src/data/party_menu.h:565-569`
/// as `u8` tile indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyMenuSlot {
    /// `sSlotTilemap_Main`: the first slot, with its HP bar.
    Main,
    /// `sSlotTilemap_MainNoHP`: the first slot without an HP bar.
    MainNoHp,
    /// `sSlotTilemap_Wide`: a wide slot, with its HP bar.
    Wide,
    /// `sSlotTilemap_WideNoHP`: a wide slot without an HP bar.
    WideNoHp,
    /// `sSlotTilemap_WideEmpty`: an empty wide slot.
    WideEmpty,
}

impl PartyMenuSlot {
    /// Every slot panel in upstream declaration order.
    pub const ALL: [Self; 5] = [
        Self::Main,
        Self::MainNoHp,
        Self::Wide,
        Self::WideNoHp,
        Self::WideEmpty,
    ];

    /// The panel's width and height in tiles; its map holds one byte per tile.
    #[must_use]
    pub const fn size_in_tiles(self) -> (usize, usize) {
        match self {
            Self::Main | Self::MainNoHp => (10, 7),
            Self::Wide | Self::WideNoHp | Self::WideEmpty => (18, 3),
        }
    }

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Main => "slot_main",
            Self::MainNoHp => "slot_main_no_hp",
            Self::Wide => "slot_wide",
            Self::WideNoHp => "slot_wide_no_hp",
            Self::WideEmpty => "slot_wide_empty",
        }
    }
}

/// One of the two button tilemaps (`src/data/party_menu.h:109-110`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyMenuButton {
    /// `sConfirmButton_Tilemap`.
    Confirm,
    /// `sCancelButton_Tilemap`.
    Cancel,
}

impl PartyMenuButton {
    /// Both buttons in upstream declaration order.
    pub const ALL: [Self; 2] = [Self::Confirm, Self::Cancel];

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Confirm => "confirm_button",
            Self::Cancel => "cancel_button",
        }
    }
}

/// The party-menu background, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct PartyMenuBackground<'a> {
    /// Tile sheet: the first 62 tiles of `bg.png`, 8x496 at 4bpp. Each screen
    /// entry's palette bank selects within [`palette`](Self::palette).
    pub sheet: ImageRef<'a>,
    /// All eleven 16-colour banks of `bg.png`'s palette (176 colours).
    pub palette: PaletteRef<'a>,
    /// The 32x32 screen (`bg.bin`) as little-endian 16-bit entries.
    pub tilemap: &'a [u8],
}

/// Every party-menu graphic, borrowing the pack for `'a`.
///
/// [`AssetPack::party_menu`] returns it with every entry validated.
#[derive(Debug, Clone, Copy)]
pub struct PartyMenuHandle<'a> {
    /// Background sheet, palette and screen.
    pub background: PartyMenuBackground<'a>,
    /// The menu Poké Ball sprite sheet, 32x64, with its 16-colour palette.
    pub poke_ball: PalettedImage<'a>,
    /// The small Poké Ball sheet, 16x96, that the Confirm and Cancel sprites
    /// draw when both are present (`src/party_menu.c:4147-4150`). Its sprite
    /// template uses the Poké Ball palette (`src/data/party_menu.h:990`), so it
    /// has none of its own: colour it with [`poke_ball`](Self::poke_ball)'s.
    pub poke_ball_small: ImageRef<'a>,
    /// The held-item icon sheet, 8x16, with its 16-colour palette.
    pub held_item: PalettedImage<'a>,
    /// The five slot-panel tilemaps in [`PartyMenuSlot::ALL`] order.
    pub slots: [&'a [u8]; 5],
    /// The confirm button's 7x2 screen entries.
    pub confirm_button: &'a [u8],
    /// The cancel button's 7x2 screen entries.
    pub cancel_button: &'a [u8],
}

impl<'a> PartyMenuHandle<'a> {
    /// The tilemap of `slot`.
    #[must_use]
    pub fn slot(&self, slot: PartyMenuSlot) -> &'a [u8] {
        self.slots[slot as usize]
    }

    /// The tilemap of `button`.
    #[must_use]
    pub fn button(&self, button: PartyMenuButton) -> &'a [u8] {
        match button {
            PartyMenuButton::Confirm => self.confirm_button,
            PartyMenuButton::Cancel => self.cancel_button,
        }
    }
}

fn malformed(id: String, detail: String) -> PackError {
    PackError::MalformedPartyMenuAsset { id, detail }
}

impl AssetPack {
    /// Bundle every party-menu graphic.
    ///
    /// # Errors
    ///
    /// The first failure of the individual readers: [`PackError::UnknownAsset`]
    /// or [`PackError::WrongKind`] for a missing or wrong-kind entry,
    /// [`PackError::MalformedPartyMenuAsset`] when a shape differs from upstream.
    pub fn party_menu(&self) -> Result<PartyMenuHandle<'_>, PackError> {
        Ok(PartyMenuHandle {
            background: self.party_menu_background()?,
            poke_ball: self.party_menu_poke_ball()?,
            poke_ball_small: self.party_menu_poke_ball_small()?,
            held_item: self.party_menu_held_item()?,
            slots: [
                self.party_menu_slot_tilemap(PartyMenuSlot::Main)?,
                self.party_menu_slot_tilemap(PartyMenuSlot::MainNoHp)?,
                self.party_menu_slot_tilemap(PartyMenuSlot::Wide)?,
                self.party_menu_slot_tilemap(PartyMenuSlot::WideNoHp)?,
                self.party_menu_slot_tilemap(PartyMenuSlot::WideEmpty)?,
            ],
            confirm_button: self.party_menu_button_tilemap(PartyMenuButton::Confirm)?,
            cancel_button: self.party_menu_button_tilemap(PartyMenuButton::Cancel)?,
        })
    }

    /// The background's tile sheet, its eleven-bank palette and its screen.
    ///
    /// # Errors
    ///
    /// As [`AssetPack::party_menu`], for the background's three entries.
    pub fn party_menu_background(&self) -> Result<PartyMenuBackground<'_>, PackError> {
        Ok(PartyMenuBackground {
            sheet: self.bank_image(
                &format!("{IMAGE_PREFIX}/bg"),
                BACKGROUND_SHEET_SIZE,
                malformed,
            )?,
            palette: self.sized_palette(
                &format!("{PALETTE_PREFIX}/bg"),
                BACKGROUND_PALETTE_COLORS,
                malformed,
            )?,
            tilemap: self.party_menu_raw("bg", BACKGROUND_TILEMAP_BYTES)?,
        })
    }

    /// The menu Poké Ball sheet with its palette.
    ///
    /// # Errors
    ///
    /// As [`AssetPack::party_menu`], for the sheet and its palette.
    pub fn party_menu_poke_ball(&self) -> Result<PalettedImage<'_>, PackError> {
        self.party_menu_paired("pokeball", POKE_BALL_SHEET_SIZE)
    }

    /// The small Poké Ball sheet, which shares
    /// [`party_menu_poke_ball`](Self::party_menu_poke_ball)'s palette.
    ///
    /// # Errors
    ///
    /// As [`AssetPack::party_menu`], for the sheet.
    pub fn party_menu_poke_ball_small(&self) -> Result<ImageRef<'_>, PackError> {
        self.bank_image(
            &format!("{IMAGE_PREFIX}/pokeball_small"),
            POKE_BALL_SMALL_SHEET_SIZE,
            malformed,
        )
    }

    /// The held-item icon sheet with its palette.
    ///
    /// # Errors
    ///
    /// As [`AssetPack::party_menu`], for the sheet and its palette.
    pub fn party_menu_held_item(&self) -> Result<PalettedImage<'_>, PackError> {
        self.party_menu_paired("hold_icons", HELD_ITEM_SHEET_SIZE)
    }

    /// One slot panel's `u8` tile-index map, one byte per tile of its grid.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedPartyMenuAsset`] when its length
    /// differs from the panel's grid.
    pub fn party_menu_slot_tilemap(&self, slot: PartyMenuSlot) -> Result<&[u8], PackError> {
        let (width, height) = slot.size_in_tiles();
        self.party_menu_raw(slot.pack_name(), width * height)
    }

    /// One button's 7x2 screen entries.
    ///
    /// # Errors
    ///
    /// As [`AssetPack::party_menu_slot_tilemap`].
    pub fn party_menu_button_tilemap(&self, button: PartyMenuButton) -> Result<&[u8], PackError> {
        self.party_menu_raw(button.pack_name(), BUTTON_TILEMAP_BYTES)
    }

    fn party_menu_paired(
        &self,
        name: &str,
        size: (u32, u32),
    ) -> Result<PalettedImage<'_>, PackError> {
        self.paired_image(
            &format!("{IMAGE_PREFIX}/{name}"),
            &format!("{PALETTE_PREFIX}/{name}"),
            size,
            BIT_DEPTH,
            BANK_COLORS,
            malformed,
        )
    }

    fn party_menu_raw(&self, name: &str, expected_len: usize) -> Result<&[u8], PackError> {
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
