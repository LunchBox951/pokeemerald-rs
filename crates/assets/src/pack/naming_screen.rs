//! Typed naming-screen graphics (upstream `src/naming_screen.c`,
//! `graphics/naming_screen/`).
//!
//! Pack ids stay private so scene code composes no strings or palette-bank
//! offsets. Every lookup re-validates shapes, since a hand-built or corrupt
//! pack can bypass extraction.

use super::{AssetPack, PackError, PaletteRef, PalettedImage};

const IMAGE_PREFIX: &str = "naming-screen/image";
const PALETTE_PREFIX: &str = "naming-screen/palette";
const RAW_PREFIX: &str = "naming-screen/raw";
/// The player icon's sheets are the object-event sprite manifest's.
const SPRITE_PREFIX: &str = "sprite";
const SPRITE_PALETTE_PREFIX: &str = "sprite/palette";

/// Every naming-screen sheet and palette is 4bpp.
const BIT_DEPTH: u8 = 4;
/// Colours in one 4bpp bank, the size of every naming-screen palette.
const BANK_COLORS: u16 = 16;
const PALETTE_ENTRY_BYTES: usize = 2;
/// A 32x20 tilemap of 2-byte entries.
const TILEMAP_BYTES: usize = 32 * 20 * 2;
/// `GetTextWindowPalette(2)` (`naming_screen.c:1885`).
const TEXT_WINDOW_PALETTE_INDEX: u8 = 2;
/// A Brendan or May walking sheet: nine 16x32 frames
/// (`gObjectEventPic_BrendanNormal`, `-mwidth 2 -mheight 4`).
const WALKING_SHEET_SIZE: (u32, u32) = (144, 32);

/// Every naming-screen graphic, borrowing the pack for `'a`.
///
/// [`AssetPack::naming_screen`] returns it with every entry validated. Each
/// sheet is already paired with the palette upstream binds it to
/// (`sSpriteTemplate_*`, `sSpritePalettes`); every palette is also exposed on
/// its own, since the keyboard palettes load by bank and the page-swap button
/// recolours with the active page.
#[derive(Debug, Clone, Copy)]
pub struct NamingScreenHandle<'a> {
    /// Background tilemap (`background.bin`), 32x20 entries.
    pub background_tilemap: &'a [u8],
    /// Upper-case keyboard tilemap (`keyboard_upper.bin`), 32x20 entries.
    pub keyboard_upper_tilemap: &'a [u8],
    /// Lower-case keyboard tilemap (`keyboard_lower.bin`), 32x20 entries.
    pub keyboard_lower_tilemap: &'a [u8],
    /// Symbols keyboard tilemap (`keyboard_symbols.bin`), 32x20 entries.
    pub keyboard_symbols_tilemap: &'a [u8],
    /// Menu tiles (`menu.png`), 24x128, with the menu palette.
    pub menu: PalettedImage<'a>,
    /// Page-swap frame sprite, 40x32, with the buttons palette.
    pub page_swap_frame: PalettedImage<'a>,
    /// Page-swap button sprite, 32x16, with the upper-page palette it is
    /// created with; the active page recolours it.
    pub page_swap_button: PalettedImage<'a>,
    /// Page-swap label for the upper-case page, 40x8, with the buttons palette.
    pub page_swap_upper: PalettedImage<'a>,
    /// Page-swap label for the lower-case page, 40x8, with the buttons palette.
    pub page_swap_lower: PalettedImage<'a>,
    /// Page-swap label for the symbols page, 40x8, with the buttons palette.
    pub page_swap_others: PalettedImage<'a>,
    /// BACK button sprite, 40x24, with the buttons palette.
    pub back_button: PalettedImage<'a>,
    /// OK button sprite, 40x24, with the buttons palette.
    pub ok_button: PalettedImage<'a>,
    /// Cursor sprite, 8x40 (its first five tiles in tile order), with the
    /// cursor palette.
    pub cursor: PalettedImage<'a>,
    /// Squished cursor sprite, 8x40, with the cursor palette.
    pub cursor_squished: PalettedImage<'a>,
    /// Filled cursor sprite, 8x40, with the cursor palette.
    pub cursor_filled: PalettedImage<'a>,
    /// Input arrow sprite, 8x8, with the others-page palette.
    pub input_arrow: PalettedImage<'a>,
    /// Input underscore sprite, 8x8, with the others-page palette.
    pub underscore: PalettedImage<'a>,
    /// PC icon, powered off, 16x24, with the menu palette.
    pub pc_icon_off: PalettedImage<'a>,
    /// PC icon, powered on, 16x24, with the menu palette.
    pub pc_icon_on: PalettedImage<'a>,
    /// Brendan's walking sheet (144x32) with his object-event palette: the
    /// player icon `GetRivalAvatarGraphicsIdByStateIdAndGender` selects for a
    /// male save.
    pub player_icon_brendan: PalettedImage<'a>,
    /// May's walking sheet (144x32) with her object-event palette.
    pub player_icon_may: PalettedImage<'a>,
    /// `menu.pal`, 16 colours: background bank 0 and the PC icon.
    pub menu_palette: PaletteRef<'a>,
    /// `page_swap_upper.pal`, 16 colours.
    pub page_swap_upper_palette: PaletteRef<'a>,
    /// `page_swap_lower.pal`, 16 colours.
    pub page_swap_lower_palette: PaletteRef<'a>,
    /// `page_swap_others.pal`, 16 colours: also the input arrow and underscore.
    pub page_swap_others_palette: PaletteRef<'a>,
    /// `buttons.pal`, 16 colours: the page-swap frame, labels, BACK and OK.
    pub buttons_palette: PaletteRef<'a>,
    /// `cursor.pal`, 16 colours.
    pub cursor_palette: PaletteRef<'a>,
    /// `keyboard.pal`, 16 colours: background bank 10.
    pub keyboard_palette: PaletteRef<'a>,
    /// Text-window palette 2 (`GetTextWindowPalette(2)`), 16 colours:
    /// background bank 11.
    pub text_window_palette: PaletteRef<'a>,
}

impl<'a> NamingScreenHandle<'a> {
    /// The six palettes upstream's `gNamingScreenMenu_Pal` stacks and
    /// `LoadPalettes` loads into background banks 0 through 5: menu, the three
    /// page-swap pages, buttons, then cursor.
    #[must_use]
    pub const fn menu_palette_banks(&self) -> [PaletteRef<'a>; 6] {
        [
            self.menu_palette,
            self.page_swap_upper_palette,
            self.page_swap_lower_palette,
            self.page_swap_others_palette,
            self.buttons_palette,
            self.cursor_palette,
        ]
    }
}

impl AssetPack {
    /// Bundle every naming-screen sheet, palette, and tilemap, plus the
    /// Brendan and May walking sheets the player icon shows.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedNamingScreenAsset`] when an
    /// image's dimensions, bit depth, payload length, or pixel indices, a
    /// palette's colour count or payload length, or a tilemap's length differs
    /// from upstream; [`PackError::MalformedTextWindowPalette`] for a bad
    /// text-window palette 2.
    pub fn naming_screen(&self) -> Result<NamingScreenHandle<'_>, PackError> {
        Ok(NamingScreenHandle {
            background_tilemap: self.naming_screen_tilemap("background")?,
            keyboard_upper_tilemap: self.naming_screen_tilemap("keyboard_upper")?,
            keyboard_lower_tilemap: self.naming_screen_tilemap("keyboard_lower")?,
            keyboard_symbols_tilemap: self.naming_screen_tilemap("keyboard_symbols")?,
            menu: self.naming_screen_sheet("menu", (24, 128), "menu")?,
            page_swap_frame: self.naming_screen_sheet("page_swap_frame", (40, 32), "buttons")?,
            page_swap_button: self.naming_screen_sheet(
                "page_swap_button",
                (32, 16),
                "page_swap_upper",
            )?,
            page_swap_upper: self.naming_screen_sheet("page_swap_upper", (40, 8), "buttons")?,
            page_swap_lower: self.naming_screen_sheet("page_swap_lower", (40, 8), "buttons")?,
            page_swap_others: self.naming_screen_sheet("page_swap_others", (40, 8), "buttons")?,
            back_button: self.naming_screen_sheet("back_button", (40, 24), "buttons")?,
            ok_button: self.naming_screen_sheet("ok_button", (40, 24), "buttons")?,
            cursor: self.naming_screen_sheet("cursor", (8, 40), "cursor")?,
            cursor_squished: self.naming_screen_sheet("cursor_squished", (8, 40), "cursor")?,
            cursor_filled: self.naming_screen_sheet("cursor_filled", (8, 40), "cursor")?,
            input_arrow: self.naming_screen_sheet("input_arrow", (8, 8), "page_swap_others")?,
            underscore: self.naming_screen_sheet("underscore", (8, 8), "page_swap_others")?,
            pc_icon_off: self.naming_screen_sheet("pc_icon_off", (16, 24), "menu")?,
            pc_icon_on: self.naming_screen_sheet("pc_icon_on", (16, 24), "menu")?,
            player_icon_brendan: self.naming_screen_player_icon("brendan")?,
            player_icon_may: self.naming_screen_player_icon("may")?,
            menu_palette: self.naming_screen_palette("menu")?,
            page_swap_upper_palette: self.naming_screen_palette("page_swap_upper")?,
            page_swap_lower_palette: self.naming_screen_palette("page_swap_lower")?,
            page_swap_others_palette: self.naming_screen_palette("page_swap_others")?,
            buttons_palette: self.naming_screen_palette("buttons")?,
            cursor_palette: self.naming_screen_palette("cursor")?,
            keyboard_palette: self.naming_screen_palette("keyboard")?,
            text_window_palette: self.text_window_extra_palette(TEXT_WINDOW_PALETTE_INDEX)?,
        })
    }

    fn naming_screen_sheet(
        &self,
        stem: &str,
        shape: (u32, u32),
        palette: &str,
    ) -> Result<PalettedImage<'_>, PackError> {
        self.paired_image(
            &format!("{IMAGE_PREFIX}/{stem}"),
            &format!("{PALETTE_PREFIX}/{palette}"),
            shape,
            BIT_DEPTH,
            BANK_COLORS,
            |id, detail| PackError::MalformedNamingScreenAsset { id, detail },
        )
    }

    fn naming_screen_player_icon(&self, who: &str) -> Result<PalettedImage<'_>, PackError> {
        self.paired_image(
            &format!("{SPRITE_PREFIX}/{who}/walking"),
            &format!("{SPRITE_PALETTE_PREFIX}/{who}"),
            WALKING_SHEET_SIZE,
            BIT_DEPTH,
            BANK_COLORS,
            |id, detail| PackError::MalformedNamingScreenAsset { id, detail },
        )
    }

    fn naming_screen_palette(&self, stem: &str) -> Result<PaletteRef<'_>, PackError> {
        let id = format!("{PALETTE_PREFIX}/{stem}");
        let palette = self.palette(&id)?;
        if palette.color_count != BANK_COLORS
            || palette.raw.len() != usize::from(BANK_COLORS) * PALETTE_ENTRY_BYTES
        {
            return Err(PackError::MalformedNamingScreenAsset {
                id,
                detail: format!(
                    "declares {} colours in {} bytes: expected {BANK_COLORS} colours",
                    palette.color_count,
                    palette.raw.len()
                ),
            });
        }
        Ok(palette)
    }

    fn naming_screen_tilemap(&self, stem: &str) -> Result<&[u8], PackError> {
        let id = format!("{RAW_PREFIX}/{stem}");
        let bytes = self.raw(&id)?;
        if bytes.len() != TILEMAP_BYTES {
            return Err(PackError::MalformedNamingScreenAsset {
                id,
                detail: format!("is {} bytes: expected {TILEMAP_BYTES}", bytes.len()),
            });
        }
        Ok(bytes)
    }
}
