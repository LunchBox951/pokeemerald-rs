//! Draws the field start menu's item and confirmation windows.

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::AssetPack;
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window as msgwin;
use engine::text::Token;
use platform::{ButtonState, Buttons};
use rendering::{Framebuffer, Rgb888};

use crate::overworld::NpcDialog;
use crate::textbox::{self, FrameAssets};

use super::StartMenuError;

/// Tilemap column of the item window's content.
pub(super) const MENU_TILEMAP_LEFT: i32 = 22;
/// Tilemap row of the item window's content.
pub(super) const MENU_TILEMAP_TOP: i32 = 1;
/// Item-window content width in tiles.
pub(super) const MENU_WIDTH: i32 = 7;
const ITEM_HEIGHT_TILES: i32 = 2;
const ITEM_WINDOW_VERTICAL_MARGIN_TILES: i32 = 2;

/// Item-window content height in tiles.
pub(super) fn menu_height(items: usize) -> i32 {
    i32::try_from(items).unwrap_or(0) * ITEM_HEIGHT_TILES + ITEM_WINDOW_VERTICAL_MARGIN_TILES
}

/// Window-local pixel origin of the first item label.
pub(super) const LABEL_ORIGIN: (i32, i32) = (8, 9);
/// Window-local pixel origin of the item cursor.
pub(super) const CURSOR_ORIGIN: (i32, i32) = (0, 9);
/// Vertical distance between menu options in pixels.
pub(super) const OPTION_HEIGHT_PX: i32 = ITEM_HEIGHT_TILES * TILE_SIZE_PX;

/// Tilemap column of the confirmation window's content.
pub(super) const YES_NO_TILEMAP_LEFT: i32 = 21;
/// Tilemap row of the confirmation window's content.
pub(super) const YES_NO_TILEMAP_TOP: i32 = 9;
/// Confirmation-window content width in tiles.
pub(super) const YES_NO_WIDTH: i32 = 5;
/// Confirmation-window content height in tiles.
pub(super) const YES_NO_HEIGHT: i32 = 4;
/// Window-local pixel origin of the YES and NO labels.
pub(super) const YES_NO_LABEL_ORIGIN: (i32, i32) = (8, 1);
/// Window-local pixel origin of the confirmation cursor.
pub(super) const YES_NO_CURSOR_ORIGIN: (i32, i32) = (0, 1);

/// Glyph drawn beside the selected menu option.
pub(super) const SELECTOR_ARROW: char = '▶';

const DEFAULT_WINDOW_FRAME_ID: u8 = 0;
const CONTENT_FILL_PALETTE_INDEX: usize = 1;
const GLYPH_COLOR_COUNT: usize = 4;
const GLYPH_FOREGROUND_INDEX: usize = 1;
const GLYPH_SHADOW_INDEX: usize = 2;
const FONT_FOREGROUND_PALETTE_INDEX: usize = 2;
const FONT_SHADOW_PALETTE_INDEX: usize = 3;
const TILE_SIZE_PX: i32 = msgwin::TILE_SIZE.cast_signed();
const _: () = assert!(msgwin::TILE_SIZE == 8);

/// Assets used to draw every window owned by an open start menu.
#[derive(Debug)]
pub(crate) struct StartMenuChrome {
    font_sheet: OwnedFontGlyphSheet,
    pub(super) std_frame: FrameAssets,
    pub(super) message_frame: FrameAssets,
}

impl StartMenuChrome {
    /// Loads owned font and window assets from an asset pack.
    ///
    /// # Errors
    ///
    /// Returns [`StartMenuError::Pack`] when an entry is missing or malformed,
    /// or [`StartMenuError::Font`] when the font sheet does not decode.
    pub(super) fn from_pack(pack: &AssetPack) -> Result<Self, StartMenuError> {
        Ok(Self {
            font_sheet: OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?,
            std_frame: FrameAssets::from_handle(pack.text_window_frame(DEFAULT_WINDOW_FRAME_ID)?),
            message_frame: FrameAssets::from_handle(pack.message_box()?),
        })
    }

    /// Creates a field message box with the start menu's loaded assets.
    pub(super) fn message_box(&self, tokens: Vec<Token>, text_speed: TextSpeed) -> NpcDialog {
        NpcDialog::new(
            self.font_sheet.clone(),
            self.message_frame.clone(),
            tokens,
            text_speed,
        )
    }

    /// Renders every glyph in a fixed menu label immediately.
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
        let mut printer = Printer::new(tokens, self.font_sheet.sheet(), TextSpeed::Instant, (0, 0));
        let mut glyphs = Vec::new();
        for _ in 0..=ticks {
            match printer.tick(PrinterInput::none()) {
                TickEvent::Glyph(g) => glyphs.push(*g),
                TickEvent::Finished => break,
                _ => {}
            }
        }
        glyphs
    }

    fn glyph_colors(&self) -> [Option<Rgb888>; GLYPH_COLOR_COUNT] {
        let palette_color = |index: usize| self.message_frame.palette.get(index).copied();
        let mut colors = [None; GLYPH_COLOR_COUNT];
        colors[GLYPH_FOREGROUND_INDEX] = palette_color(FONT_FOREGROUND_PALETTE_INDEX);
        colors[GLYPH_SHADOW_INDEX] = palette_color(FONT_SHADOW_PALETTE_INDEX);
        colors
    }

    // The game assigns standard-window borders and message-box content to
    // separate palette banks.
    fn content_fill_color(&self) -> Rgb888 {
        self.message_frame
            .palette
            .get(CONTENT_FILL_PALETTE_INDEX)
            .copied()
            .unwrap_or(Rgb888::BLACK)
    }

    /// Draws a standard window at a tilemap rectangle.
    pub(super) fn draw_window(
        &self,
        fb: &mut Framebuffer,
        left_tiles: i32,
        top_tiles: i32,
        width_tiles: i32,
        height_tiles: i32,
    ) {
        textbox::fill_rect(
            fb,
            (left_tiles * TILE_SIZE_PX, top_tiles * TILE_SIZE_PX),
            width_tiles * TILE_SIZE_PX,
            height_tiles * TILE_SIZE_PX,
            self.content_fill_color(),
        );
        let tiles = msgwin::border_tiles(left_tiles, top_tiles, width_tiles, height_tiles);
        textbox::blit_frame_tiles(fb, &tiles, self.std_frame.image(), &self.std_frame.palette);
    }

    /// Draws glyphs at a window-local pixel origin, clipped to its content.
    pub(super) fn draw_text(
        &self,
        fb: &mut Framebuffer,
        window_tiles: (i32, i32, i32, i32),
        origin_px: (i32, i32),
        glyphs: &[RevealedGlyph],
    ) {
        let (left_tiles, top_tiles, width_tiles, height_tiles) = window_tiles;
        textbox::blit_glyphs_colored(
            fb,
            glyphs,
            (
                left_tiles * TILE_SIZE_PX + origin_px.0,
                top_tiles * TILE_SIZE_PX + origin_px.1,
            ),
            (
                width_tiles * TILE_SIZE_PX - origin_px.0,
                height_tiles * TILE_SIZE_PX - origin_px.1,
            ),
            &self.glyph_colors(),
        );
    }
}

#[cfg(test)]
const FONT_SHEET_BIT_DEPTH: u8 = 2;
#[cfg(test)]
const STANDARD_FRAME_SIZE_TILES: (u32, u32) = (3, 3);
#[cfg(test)]
const MESSAGE_FRAME_SIZE_TILES: (u32, u32) = (7, 2);
#[cfg(test)]
const WINDOW_PALETTE_COLOR_COUNT: usize = 16;

#[cfg(test)]
impl StartMenuChrome {
    pub(super) fn synthetic() -> Self {
        use assets::fonts::FontImageRef;
        use assets::pack::ImageRef;

        let pixels = vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
        let image = ImageRef {
            width: assets::fonts::SHEET_WIDTH,
            height: assets::fonts::SHEET_HEIGHT,
            bit_depth: FONT_SHEET_BIT_DEPTH,
            pixels: &pixels,
        };
        let sheet = OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image))
            .expect("this is the exact real glyph-sheet shape");
        Self {
            font_sheet: sheet,
            std_frame: synthetic_frame(STANDARD_FRAME_SIZE_TILES),
            message_frame: synthetic_frame(MESSAGE_FRAME_SIZE_TILES),
        }
    }
}

#[cfg(test)]
fn synthetic_frame((width_tiles, height_tiles): (u32, u32)) -> FrameAssets {
    let width = width_tiles * msgwin::TILE_SIZE;
    let height = height_tiles * msgwin::TILE_SIZE;
    FrameAssets {
        pixels: vec![0u8; (width * height) as usize],
        width,
        height,
        palette: vec![Rgb888::BLACK; WINDOW_PALETTE_COLOR_COUNT],
    }
}

const YES_OPTION: u8 = 0;
const NO_OPTION: u8 = 1;

/// Selection state for the two-row YES/NO confirmation window.
#[derive(Debug)]
pub(crate) struct YesNoMenu {
    /// Zero-based selected row.
    pub(super) cursor: u8,
}

impl YesNoMenu {
    /// Creates a confirmation menu, optionally with NO selected.
    pub(super) fn new(default_no: bool) -> Self {
        Self {
            cursor: if default_no { NO_OPTION } else { YES_OPTION },
        }
    }

    /// Returns the selected answer after A or B, or `None` while waiting.
    pub(super) fn process_input(&mut self, buttons: ButtonState) -> Option<bool> {
        if buttons.is_newly_pressed(Buttons::A) {
            return Some(self.cursor == YES_OPTION);
        }
        if buttons.is_newly_pressed(Buttons::B) {
            return Some(false);
        }
        if buttons.is_newly_pressed(Buttons::UP) {
            self.cursor = self.cursor.saturating_sub(1);
        } else if buttons.is_newly_pressed(Buttons::DOWN) {
            self.cursor = NO_OPTION;
        }
        None
    }
}
