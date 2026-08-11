//! The start menu's windows: geometry, decoded chrome, and the Yes/No
//! window (I-6, issue #232).
//!
//! Split out of [`super`] so that file keeps only the menu *state machine*
//! (`one module = one concept` `(oop-boundaries)`): everything here is
//! about where a window sits and what colour its pixels are, transcribed
//! from `pokeemerald/src/menu.c`'s window templates and
//! `src/start_menu.c`'s printer calls -- see [`super`]'s own geometry
//! table for the full citation list.

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::AssetPack;
use engine::text::render::{Printer, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window as msgwin;
use engine::text::Token;
use platform::{ButtonState, Buttons};
use rendering::{Framebuffer, Rgb888};

use crate::overworld::NpcDialog;
use crate::textbox::{self, FrameAssets};

use super::StartMenuError;

/// `AddStartMenuWindow`'s `tilemapLeft` (`pokeemerald/src/menu.c:493`).
pub(super) const MENU_TILEMAP_LEFT: i32 = 22;
/// `AddStartMenuWindow`'s `tilemapTop` (`:493`).
pub(super) const MENU_TILEMAP_TOP: i32 = 1;
/// `AddStartMenuWindow`'s `width`, in tiles (`:493`).
pub(super) const MENU_WIDTH: i32 = 7;
/// `numActions * 2 + 2` (`:493`): each item is two tiles tall, plus a
/// one-tile margin at each end.
pub(super) fn menu_height(items: usize) -> i32 {
    i32::try_from(items).unwrap_or(0) * 2 + 2
}
/// `PrintStartMenuActions`' window-local label origin: `x = 8`,
/// `y = (index << 4) + 9` (`start_menu.c:472-473`).
pub(super) const LABEL_ORIGIN: (i32, i32) = (8, 9);
/// `InitMenuNormal(GetStartMenuWindowId(), FONT_NORMAL, 0, 9, 16, ...)`
/// (`start_menu.c:511`): the cursor's own window-local `(left, top)`.
pub(super) const CURSOR_ORIGIN: (i32, i32) = (0, 9);
/// `sMenu.optionHeight` for both this menu (`:511`) and the Yes/No menu
/// (`src/menu.c:1566`): one item every 16 pixels.
pub(super) const OPTION_HEIGHT_PX: i32 = 16;

/// `sYesNo_WindowTemplates` (`pokeemerald/src/menu.c:98-107`).
pub(super) const YES_NO_TILEMAP_LEFT: i32 = 21;
/// See [`YES_NO_TILEMAP_LEFT`].
pub(super) const YES_NO_TILEMAP_TOP: i32 = 9;
/// See [`YES_NO_TILEMAP_LEFT`].
pub(super) const YES_NO_WIDTH: i32 = 5;
/// See [`YES_NO_TILEMAP_LEFT`].
pub(super) const YES_NO_HEIGHT: i32 = 4;
/// `CreateYesNoMenu`'s `gText_YesNo` printer origin (`src/menu.c:1628-1631`)
/// and `InitMenuInUpperLeftCorner`'s cursor `top` (`:1561`) -- the label at
/// `(8, 1)`, the cursor at `(0, 1)`.
pub(super) const YES_NO_LABEL_ORIGIN: (i32, i32) = (8, 1);
/// See [`YES_NO_LABEL_ORIGIN`].
pub(super) const YES_NO_CURSOR_ORIGIN: (i32, i32) = (0, 1);

/// `gText_SelectorArrow3` (`pokeemerald/src/strings.c:1468`): the glyph
/// `Menu_MoveCursor` prints at the selected row (`src/menu.c:945`).
pub(super) const SELECTOR_ARROW: char = '▶';

/// `text_window_frame`'s zero-based frame id, the same
/// `WINDOW_FRAME_TYPE_0` [`crate::main_menu`] uses -- `DrawStdWindowFrame`
/// draws `STD_WINDOW_BASE_TILE_NUM`, whose tiles are loaded from
/// `GetWindowFrameTilesPal(gSaveBlock2Ptr->optionsWindowFrameType)` and
/// default to frame 0 on a fresh save block.
const FRAME_ID: u8 = 0;

/// `FillWindowPixelBuffer(windowId, PIXEL_FILL(1))`
/// (`DrawStdWindowFrame`, `src/menu.c:228`): the window-content fill is
/// palette index 1 of the standard frame's own palette.
const CONTENT_FILL_INDEX: usize = 1;
/// `gFontInfos[FONT_NORMAL].fgColor` (`src/text.c:137`).
const FONT_FG_INDEX: usize = 2;
/// `gFontInfos[FONT_NORMAL].shadowColor` (`src/text.c:139`).
const FONT_SHADOW_INDEX: usize = 3;

/// `engine::text::window::TILE_SIZE` as a plain `i32`, for this module's
/// tilemap -> pixel conversions (mirrors [`crate::main_menu`]'s own).
const TILE_PX: i32 = 8;
const _: () = assert!(msgwin::TILE_SIZE == 8);

/// The decoded chrome every window this menu opens draws itself from: the
/// font sheet, the standard window frame, and the field message box's own
/// frame.
///
/// Owned once per opened menu rather than re-read per message: upstream
/// loads all of it into VRAM when the menu opens
/// (`LoadMessageBoxAndBorderGfx`) and every window from then on is a
/// tilemap write.
#[derive(Debug)]
pub(crate) struct StartMenuChrome {
    pub(super) sheet: OwnedFontGlyphSheet,
    /// `GetWindowFrameTilesPal(...)`, drawn by `DrawStdWindowFrame`.
    pub(super) std_frame: FrameAssets,
    /// The dialogue frame `ShowSaveMessage`'s box draws.
    pub(super) message_frame: FrameAssets,
}

impl StartMenuChrome {
    /// Copy the three pack entries out of an already-loaded `pack`.
    ///
    /// # Errors
    ///
    /// [`StartMenuError::Pack`] if any entry is missing or malformed;
    /// [`StartMenuError::Font`] if the font sheet doesn't decode.
    pub(super) fn from_pack(pack: &AssetPack) -> Result<Self, StartMenuError> {
        Ok(Self {
            sheet: OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?,
            std_frame: FrameAssets::from_handle(pack.text_window_frame(FRAME_ID)?),
            message_frame: FrameAssets::from_handle(pack.message_box()?),
        })
    }

    /// A field message box printing `tokens` -- `ShowSaveMessage`'s window
    /// ([`NpcDialog::new`]'s own docs on why the two share a type).
    pub(super) fn message_box(&self, tokens: Vec<Token>) -> NpcDialog {
        NpcDialog::new(self.sheet.clone(), self.message_frame.clone(), tokens)
    }

    /// `AddTextPrinterParameterized`'s effect for a fixed menu label:
    /// every glyph, revealed at once (menu labels are never paced --
    /// [`crate::main_menu`]'s "Rendering pipeline" docs own that rationale).
    pub(super) fn render_label(&self, text: &str) -> Vec<RevealedGlyph> {
        let mut tokens: Vec<Token> = text
            .chars()
            .map(|c| {
                if c == '\n' {
                    Token::Newline
                } else {
                    Token::Char(c)
                }
            })
            .collect();
        tokens.push(Token::End);
        let ticks = tokens.len();
        let mut printer = Printer::new(tokens, self.sheet.sheet(), TextSpeed::Instant, (0, 0));
        let mut glyphs = Vec::new();
        for _ in 0..=ticks {
            match printer.tick(false) {
                TickEvent::Glyph(g) => glyphs.push(*g),
                TickEvent::Finished => break,
                _ => {}
            }
        }
        glyphs
    }

    /// The glyph-index -> colour mapping every window here prints with:
    /// `FONT_NORMAL`'s own default `fgColor`/`shadowColor` indices, read
    /// out of the standard frame's palette. Index 0 (background) and index
    /// 3 (box) stay transparent -- the window content rect is already
    /// filled [`CONTENT_FILL_INDEX`] before any glyph is drawn, exactly as
    /// `DrawStdWindowFrame`'s own `FillWindowPixelBuffer` does.
    fn glyph_colors(&self) -> [Option<Rgb888>; 4] {
        let color = |index: usize| self.std_frame.palette.get(index).copied();
        [None, color(FONT_FG_INDEX), color(FONT_SHADOW_INDEX), None]
    }

    /// [`CONTENT_FILL_INDEX`]'s real colour, or black for a frame palette
    /// too short to have one (unreachable against an extracted pack, whose
    /// window palettes are always a full bank).
    fn content_fill(&self) -> Rgb888 {
        self.std_frame
            .palette
            .get(CONTENT_FILL_INDEX)
            .copied()
            .unwrap_or(Rgb888::BLACK)
    }

    /// Draw one standard window: `DrawStdWindowFrame`'s content fill plus
    /// [`msgwin::border_tiles`]' border ring, at a tilemap rect.
    pub(super) fn draw_window(
        &self,
        fb: &mut Framebuffer,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
    ) {
        textbox::fill_rect(
            fb,
            (left * TILE_PX, top * TILE_PX),
            width * TILE_PX,
            height * TILE_PX,
            self.content_fill(),
        );
        let tiles = msgwin::border_tiles(left, top, width, height);
        textbox::blit_frame_tiles(fb, &tiles, self.std_frame.image(), &self.std_frame.palette);
    }

    /// Blit `glyphs` at a window-local `origin` inside the window whose
    /// content rect starts at tilemap `(left, top)` and is
    /// `width`x`height` tiles, clipped to that rect.
    pub(super) fn draw_text(
        &self,
        fb: &mut Framebuffer,
        window: (i32, i32, i32, i32),
        origin: (i32, i32),
        glyphs: &[RevealedGlyph],
    ) {
        let (left, top, width, height) = window;
        textbox::blit_glyphs_colored(
            fb,
            glyphs,
            (left * TILE_PX + origin.0, top * TILE_PX + origin.1),
            (width * TILE_PX - origin.0, height * TILE_PX - origin.1),
            &self.glyph_colors(),
        );
    }
}

/// Test-only: blank chrome -- an all-zero glyph sheet of the real shape and
/// two flat window frames -- so [`super`]'s own tests and
/// `crate::flow::save_continue_tests` can drive the production menu with no
/// extracted pack (mirrors
/// [`crate::overworld::dialog::synthetic_dialog`]).
#[cfg(test)]
impl StartMenuChrome {
    pub(super) fn synthetic() -> Self {
        use assets::fonts::FontImageRef;
        use assets::pack::ImageRef;

        let pixels = vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
        let image = ImageRef {
            width: assets::fonts::SHEET_WIDTH,
            height: assets::fonts::SHEET_HEIGHT,
            bit_depth: 2,
            pixels: &pixels,
        };
        let sheet = OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image))
            .expect("this is the exact real glyph-sheet shape");
        Self {
            sheet,
            std_frame: FrameAssets {
                pixels: vec![0u8; 24 * 24],
                width: 24,
                height: 24,
                palette: vec![Rgb888::BLACK; 16],
            },
            message_frame: FrameAssets {
                pixels: vec![0u8; 56 * 16],
                width: 56,
                height: 16,
                palette: vec![Rgb888::BLACK; 16],
            },
        }
    }
}

/// The Yes/No window `DisplayYesNoMenuDefaultYes`/`WithDefault` opens
/// (`src/menu.c:464-472`), as a two-row cursor.
#[derive(Debug)]
pub(crate) struct YesNoMenu {
    /// `sMenu.cursorPos`: `0` = YES, `1` = NO.
    pub(super) cursor: u8,
}

impl YesNoMenu {
    /// `CreateYesNoMenu(..., initialCursorPos)` -- `default_no` selects
    /// upstream's `DisplayYesNoMenuWithDefault(1)`, the *scarier* prompt's
    /// default (`start_menu.c:1051`).
    pub(super) fn new(default_no: bool) -> Self {
        Self {
            cursor: u8::from(default_no),
        }
    }

    /// `Menu_ProcessInputNoWrap` (`src/menu.c:1013-1041`): A chooses the
    /// row under the cursor, B is `MENU_B_PRESSED` (which every caller here
    /// treats as NO), and Up/Down move without wrapping
    /// (`Menu_MoveCursorNoWrapAround`, `:964-978`) -- unlike the start
    /// menu's own cursor, which does wrap.
    ///
    /// Returns `Some(true)` for YES, `Some(false)` for NO or B, `None`
    /// while nothing has been chosen.
    pub(super) fn process_input(&mut self, buttons: ButtonState) -> Option<bool> {
        if buttons.is_newly_pressed(Buttons::A) {
            return Some(self.cursor == 0);
        }
        if buttons.is_newly_pressed(Buttons::B) {
            return Some(false);
        }
        if buttons.is_newly_pressed(Buttons::UP) {
            self.cursor = self.cursor.saturating_sub(1);
        } else if buttons.is_newly_pressed(Buttons::DOWN) {
            self.cursor = 1;
        }
        None
    }
}
