//! Main-menu scene composition and selection.

use assets::fonts::{FontGlyphSheet, FontId};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window as msgwin;
use engine::text::Token;
use rendering::{Bgr555, Framebuffer, Rgb888};

use crate::textbox::{self, FrameAssets};

const MENU_LEFT: i32 = 2;
const MENU_WIDTH: i32 = 26;
const WINDOW_FRAME_THICKNESS_TILES: i32 = 1;
const HIGHLIGHT_SHADOW_PADDING_PX: i32 = 1;
const UNSELECTED_DARKEN_AMOUNT: u8 = 7;
const LABEL_TOP_OFFSET_PX: i32 = 1;

const TILE_SIZE_PX: i32 = 8;
const _: () = assert!(msgwin::TILE_SIZE == 8);

const FRAME_ID: u8 = 0;
const BACKGROUND_PALETTE_ID: &str = "interface/palette/main_menu_bg";

// The header colours are fixed because Task_DisplayMainMenu replaces the
// loaded palette before drawing (`pokeemerald/src/main_menu.c:755-765`).
const HEADER_TEXT_BG: Rgb888 = Bgr555::from_channels(31, 31, 31).to_rgb888();
const HEADER_TEXT_FG: Rgb888 = Bgr555::from_channels(12, 12, 12).to_rgb888();
const HEADER_TEXT_SHADOW: Rgb888 = Bgr555::from_channels(26, 26, 25).to_rgb888();

const HEADER_GLYPH_COLORS: [Option<Rgb888>; 4] =
    [None, Some(HEADER_TEXT_FG), Some(HEADER_TEXT_SHADOW), None];

mod items;
pub use items::{ItemWindow, MainMenuItem, MainMenuType};

/// An error that prevents main-menu construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainMenuSceneError {
    /// The asset pack could not be loaded or queried.
    Pack(PackError),
    /// The normal font sheet could not be decoded.
    Font(assets::AssetError),
}

impl std::fmt::Display for MainMenuSceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "main menu: {err}"),
            Self::Font(err) => write!(f, "main menu: {err}"),
        }
    }
}

impl std::error::Error for MainMenuSceneError {}

impl From<PackError> for MainMenuSceneError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<assets::AssetError> for MainMenuSceneError {
    fn from(err: assets::AssetError) -> Self {
        Self::Font(err)
    }
}

impl MainMenuSceneError {
    /// Returns whether the asset pack is missing.
    #[must_use]
    pub const fn is_pack_missing(&self) -> bool {
        matches!(self, Self::Pack(PackError::NotFound(_)))
    }
}

#[derive(Debug)]
struct RenderedMenuItem {
    item: MainMenuItem,
    window: ItemWindow,
    glyphs: Vec<RevealedGlyph>,
}

/// A selectable main-menu scene.
#[derive(Debug)]
pub struct MainMenuScene {
    frame: FrameAssets,
    background: Rgb888,
    menu_type: MainMenuType,
    rendered_items: Vec<RenderedMenuItem>,
    selected_index: usize,
}

impl MainMenuScene {
    /// Builds a menu with the default window frame.
    ///
    /// # Errors
    ///
    /// Returns an error if a required asset is missing or invalid.
    pub fn from_pack(
        pack: &AssetPack,
        menu_type: MainMenuType,
    ) -> Result<Self, MainMenuSceneError> {
        Self::from_pack_with_window_frame(pack, menu_type, FRAME_ID)
    }

    /// Builds a menu with the selected window frame.
    ///
    /// # Errors
    ///
    /// Returns an error if a required asset is missing or invalid.
    pub fn from_pack_with_window_frame(
        pack: &AssetPack,
        menu_type: MainMenuType,
        window_frame: u8,
    ) -> Result<Self, MainMenuSceneError> {
        let frame = FrameAssets::from_handle(pack.text_window_frame(window_frame)?);
        let background = textbox::palette_colors(pack.palette(BACKGROUND_PALETTE_ID)?)
            .first()
            .copied()
            .unwrap_or(Rgb888::BLACK);

        let font_image = pack.font(FontId::Normal)?;
        let sheet = FontGlyphSheet::new(font_image)?;

        Ok(Self::from_assets(frame, background, menu_type, sheet))
    }

    fn from_assets(
        frame: FrameAssets,
        background: Rgb888,
        menu_type: MainMenuType,
        sheet: FontGlyphSheet<'_>,
    ) -> Self {
        let rendered_items = menu_type
            .items()
            .iter()
            .map(|&item| RenderedMenuItem {
                item,
                window: menu_type
                    .window(item)
                    .expect("every item in a menu type's own list has a window in that list"),
                glyphs: render_label(item.label(), sheet),
            })
            .collect();

        Self {
            frame,
            background,
            menu_type,
            rendered_items,
            selected_index: 0,
        }
    }

    /// Returns the displayed menu type.
    #[must_use]
    pub const fn menu_type(&self) -> MainMenuType {
        self.menu_type
    }

    /// Returns the selected item.
    #[must_use]
    pub fn selected(&self) -> MainMenuItem {
        self.rendered_items[self.selected_index].item
    }

    /// Moves the selection up without wrapping.
    pub fn move_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    /// Moves the selection down without wrapping.
    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.rendered_items.len() {
            self.selected_index += 1;
        }
    }

    /// Composes the current menu frame.
    #[must_use]
    pub fn compose(&self) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(self.background);

        let mut painted_bg0 = textbox::Coverage::recording();
        for item in &self.rendered_items {
            draw_item(&mut fb, &mut painted_bg0, &self.frame, item);
        }

        darken_outside(
            &mut fb,
            &painted_bg0,
            highlight_rect(self.rendered_items[self.selected_index].window),
        );

        fb
    }

    /// Composes the current menu in the platform pixel format.
    #[must_use]
    pub fn compose_frame(&self) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose())
    }
}

fn draw_item(
    fb: &mut Framebuffer,
    painted_bg0: &mut textbox::Coverage,
    frame: &FrameAssets,
    item: &RenderedMenuItem,
) {
    let ItemWindow { top, height } = item.window;
    let content_origin = (MENU_LEFT * TILE_SIZE_PX, top * TILE_SIZE_PX);
    let content_size = (MENU_WIDTH * TILE_SIZE_PX, height * TILE_SIZE_PX);

    textbox::fill_rect_tracked(
        fb,
        painted_bg0,
        content_origin,
        content_size.0,
        content_size.1,
        HEADER_TEXT_BG,
    );

    let tiles = msgwin::border_tiles(MENU_LEFT, top, MENU_WIDTH, height);
    textbox::blit_frame_tiles_tracked(fb, painted_bg0, &tiles, frame.image(), &frame.palette);

    let label_origin = (content_origin.0, content_origin.1 + LABEL_TOP_OFFSET_PX);
    let label_bounds = (content_size.0, content_size.1 - LABEL_TOP_OFFSET_PX);
    textbox::blit_glyphs_colored_tracked(
        fb,
        painted_bg0,
        &item.glyphs,
        label_origin,
        label_bounds,
        &HEADER_GLYPH_COLORS,
    );
}

/// Returns the half-open pixel bounds of an item's selection highlight.
const fn highlight_rect(window: ItemWindow) -> (i32, i32, i32, i32) {
    let frame_left = MENU_LEFT - WINDOW_FRAME_THICKNESS_TILES;
    let frame_right = MENU_LEFT + MENU_WIDTH + WINDOW_FRAME_THICKNESS_TILES;
    let frame_top = window.top - WINDOW_FRAME_THICKNESS_TILES;
    let frame_bottom = window.top + window.height + WINDOW_FRAME_THICKNESS_TILES;

    (
        frame_left * TILE_SIZE_PX + HIGHLIGHT_SHADOW_PADDING_PX,
        frame_top * TILE_SIZE_PX + HIGHLIGHT_SHADOW_PADDING_PX,
        frame_right * TILE_SIZE_PX - HIGHLIGHT_SHADOW_PADDING_PX,
        frame_bottom * TILE_SIZE_PX - HIGHLIGHT_SHADOW_PADDING_PX,
    )
}

/// Darkens painted BG0 pixels outside the highlight.
///
/// Upstream targets BG0 but not the backdrop, so transparent BG0 pixels keep
/// the backdrop colour (`pokeemerald/src/main_menu.c:745-753`).
fn darken_outside(
    fb: &mut Framebuffer,
    painted_bg0: &textbox::Coverage,
    highlight: (i32, i32, i32, i32),
) {
    let (left, top, right, bottom) = highlight;
    for y in 0..Framebuffer::HEIGHT {
        for x in 0..Framebuffer::WIDTH {
            if !painted_bg0.is_painted(x, y) {
                continue;
            }
            let (Ok(pixel_x), Ok(pixel_y)) = (i32::try_from(x), i32::try_from(y)) else {
                continue;
            };
            if pixel_x >= left && pixel_x < right && pixel_y >= top && pixel_y < bottom {
                continue;
            }
            if let Some(color) = fb.pixel(x, y) {
                fb.set_pixel(x, y, rendering::darken(color, UNSELECTED_DARKEN_AMOUNT));
            }
        }
    }
}

/// Loads a menu with the default window frame.
///
/// # Errors
///
/// Returns an error if the asset pack cannot build the menu.
pub fn load_default(menu_type: MainMenuType) -> Result<MainMenuScene, MainMenuSceneError> {
    load_default_with_window_frame(menu_type, FRAME_ID)
}

/// Loads a menu with the selected window frame.
///
/// # Errors
///
/// Returns an error if the asset pack cannot build the menu.
pub fn load_default_with_window_frame(
    menu_type: MainMenuType,
    window_frame: u8,
) -> Result<MainMenuScene, MainMenuSceneError> {
    load_with_window_frame(
        crate::pack_source::PackSource::Runtime,
        menu_type,
        window_frame,
    )
}

pub(crate) fn load_with_window_frame(
    source: crate::pack_source::PackSource,
    menu_type: MainMenuType,
    window_frame: u8,
) -> Result<MainMenuScene, MainMenuSceneError> {
    let pack = source.load()?;
    MainMenuScene::from_pack_with_window_frame(&pack, menu_type, window_frame)
}

fn render_label(label: &str, sheet: FontGlyphSheet<'_>) -> Vec<RevealedGlyph> {
    let mut tokens: Vec<Token> = label.chars().map(Token::Char).collect();
    tokens.push(Token::End);
    let tick_limit = tokens.len();

    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));
    let mut glyphs = Vec::with_capacity(label.chars().count());
    for _ in 0..tick_limit {
        match printer.tick(PrinterInput::none()) {
            TickEvent::Glyph(g) => glyphs.push(*g),
            TickEvent::Finished => break,
            _ => {}
        }
    }
    glyphs
}

#[cfg(test)]
pub(crate) fn synthetic_scene(menu_type: MainMenuType) -> MainMenuScene {
    use assets::fonts::FontImageRef;
    use assets::pack::ImageRef;

    let pixels = vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
    let image = ImageRef {
        width: assets::fonts::SHEET_WIDTH,
        height: assets::fonts::SHEET_HEIGHT,
        bit_depth: 2,
        pixels: &pixels,
    };
    let sheet = FontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image)).unwrap();

    let frame = FrameAssets {
        pixels: vec![0u8; 24 * 24],
        width: 24,
        height: 24,
        palette: vec![Rgb888::BLACK; 16],
    };

    MainMenuScene::from_assets(frame, Rgb888::BLACK, menu_type, sheet)
}

#[cfg(test)]
mod saved_game_tests;
#[cfg(test)]
mod tests;
