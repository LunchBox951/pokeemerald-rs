//! Main menu (I-3, issue #149): the no-save-present path of upstream
//! `pokeemerald/src/main_menu.c`, reduced to the single item this workspace
//! has a destination for.
//!
//! # Scope: `HAS_NO_SAVED_GAME` only, and only its `NEW GAME` item
//!
//! Upstream's main menu renders a different item list depending on
//! `gSaveFileStatus` (`Task_MainMenuCheckSaveFile`, `main_menu.c:623-682`):
//! `HAS_NO_SAVED_GAME` shows `NEW GAME`/`OPTION`
//! (`main_menu.c:512`, `HandleMainMenuInput`'s `case HAS_NO_SAVED_GAME`
//! switch, `main_menu.c:952-963`). This crate has no save file on disk yet
//! (`engine::save::store::SaveStore` is not wired into any runtime load
//! path), so `HAS_NO_SAVED_GAME` is the only reachable case -- `CONTINUE`
//! (needs a save to continue), `MYSTERY GIFT`/`MYSTERY EVENTS` (need a
//! wireless adapter this platform crate doesn't model), and `OPTION` (its
//! own settings screen, out of this issue's scope per the issue brief) are
//! not rendered. [`MainMenuScene`] therefore shows the single `NEW GAME`
//! label with no cursor/selection state to track -- pressing A always means
//! "start the intro."
//!
//! # Rendering
//!
//! A small text window, `AssetPack::text_window_frame(0)`'s selectable
//! border (`window::border_tiles`, matching upstream's own
//! `DrawTextBorderOuter`-shaped `STD_WINDOW` frames the item list sits in --
//! see `engine::text::window`'s module docs) around the label glyphs,
//! rendered once at construction time via a throwaway
//! [`engine::text::render::Printer`] at
//! [`engine::text::render::TextSpeed::Instant`] (menu labels are never
//! paced/typed character-by-character upstream, unlike dialogue -- see
//! [`crate::intro`] for the paced path) and blitted with
//! [`crate::textbox`]'s helpers, the same way
//! [`crate::intro::IntroScene`] draws its dialogue box.

use assets::fonts::{FontGlyphSheet, FontId};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window as msgwin;
use engine::text::Token;
use rendering::{Framebuffer, Rgb888};

use crate::textbox::{self, FrameAssets};

/// The label's window content rect, in tilemap cells (module docs) -- chosen
/// to sit comfortably inside the 240x160 screen with room for
/// [`msgwin::border_tiles`]'s one-tile ring.
const BOX_LEFT: i32 = 4;
const BOX_TOP: i32 = 16;
const BOX_WIDTH: i32 = 12;
const BOX_HEIGHT: i32 = 2;

/// Small inset (pixels) between the box's own border and the label glyphs.
const LABEL_INSET_X: i32 = 4;
const LABEL_INSET_Y: i32 = 2;

/// `engine::text::window::TILE_SIZE` (8px tiles), as a plain `i32` literal
/// for the tilemap -> screen-pixel conversion in [`MainMenuScene::compose`].
const TILE_PX: i32 = 8;
const _: () = assert!(msgwin::TILE_SIZE == 8);

/// `text_window_frame`'s zero-based selectable-frame id this menu uses
/// (`main_menu.c`'s `MAIN_MENU_BORDER_TILE`/`LoadMainMenuWindowFrameTiles`
/// selects a specific frame graphic; this slice always uses frame `0`, the
/// first of the 20 selectable options, rather than modeling the player's
/// chosen window-frame option -- out of this issue's scope).
const FRAME_ID: u8 = 0;

/// Why building [`MainMenuScene`] failed.
///
/// Concrete per-crate-boundary enum `(oop-boundaries)`, mirroring
/// [`crate::title::TitleSceneError`]/[`crate::overworld::OverworldSceneError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainMenuSceneError {
    /// Loading or reading the asset pack failed -- most commonly
    /// [`PackError::NotFound`] (see
    /// [`MainMenuSceneError::is_pack_missing`]).
    Pack(PackError),
    /// The font glyph sheet fetched from the pack didn't decode.
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
    /// Whether this is specifically the "no pack on disk" diagnostic --
    /// mirrors [`crate::title::TitleSceneError::is_pack_missing`].
    #[must_use]
    pub const fn is_pack_missing(&self) -> bool {
        matches!(self, Self::Pack(PackError::NotFound(_)))
    }
}

/// The no-save-present main menu (module docs): a single `NEW GAME` label in
/// a selectable text window. No cursor state -- pressing A always confirms
/// the only item.
#[derive(Debug)]
pub struct MainMenuScene {
    frame: FrameAssets,
    glyphs: Vec<RevealedGlyph>,
}

impl MainMenuScene {
    /// Decode the window frame and render the `NEW GAME` label out of an
    /// already-loaded `pack`.
    ///
    /// # Errors
    ///
    /// [`MainMenuSceneError::Pack`] if the pack or the needed frame/font
    /// entries are missing; [`MainMenuSceneError::Font`] if the font sheet
    /// doesn't decode (unreachable against a real pack).
    pub fn from_pack(pack: &AssetPack) -> Result<Self, MainMenuSceneError> {
        let frame = FrameAssets::from_handle(pack.text_window_frame(FRAME_ID)?);

        let font_image = pack.font(FontId::Normal)?;
        let sheet = FontGlyphSheet::new(font_image)?;
        let glyphs = render_label("NEW GAME", sheet);

        Ok(Self { frame, glyphs })
    }

    /// Composite the menu's frame and label into a fresh [`Framebuffer`].
    /// Static: identical output on every call (no animation in this slice).
    #[must_use]
    pub fn compose(&self) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(Rgb888::BLACK);

        let tiles = msgwin::border_tiles(BOX_LEFT, BOX_TOP, BOX_WIDTH, BOX_HEIGHT);
        textbox::blit_frame_tiles(&mut fb, &tiles, self.frame.image(), &self.frame.palette);

        let origin = (
            BOX_LEFT * TILE_PX + LABEL_INSET_X,
            BOX_TOP * TILE_PX + LABEL_INSET_Y,
        );
        textbox::blit_glyphs(&mut fb, &self.glyphs, origin);

        fb
    }

    /// [`compose`](Self::compose), converted to `platform`'s
    /// presentation-ready pixel format -- mirrors
    /// [`crate::title::TitleScene::compose_frame`].
    #[must_use]
    pub fn compose_frame(&self) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose())
    }
}

/// Load the pack from its default location and build the menu out of it in
/// one step -- mirrors [`crate::title::load_default`].
///
/// # Errors
///
/// [`MainMenuSceneError::Pack`] with [`MainMenuSceneError::is_pack_missing`]
/// true if no pack has been extracted yet; see
/// [`MainMenuScene::from_pack`] for the other (real-pack-only) error cases.
pub fn load_default() -> Result<MainMenuScene, MainMenuSceneError> {
    let pack = AssetPack::load_default()?;
    MainMenuScene::from_pack(&pack)
}

/// Drive a throwaway [`Printer`] at [`TextSpeed::Instant`] to completion over
/// `label`'s plain-ASCII characters, collecting every revealed glyph
/// (module docs: menu labels are never paced/typed upstream).
fn render_label(label: &str, sheet: FontGlyphSheet<'_>) -> Vec<RevealedGlyph> {
    let mut tokens: Vec<Token> = label.chars().map(Token::Char).collect();
    tokens.push(Token::End);

    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));
    let mut glyphs = Vec::new();
    // One tick per token is always enough to exhaust the stream at Instant
    // speed (no reveal delay, no scroll/clear waits -- a plain label has
    // none of those tokens); +1 covers the terminal `Finished` tick.
    for _ in 0..=tokens_len(label) {
        match printer.tick(false) {
            TickEvent::Glyph(g) => glyphs.push(*g),
            TickEvent::Finished => break,
            _ => {}
        }
    }
    glyphs
}

/// Upper bound on ticks [`render_label`] needs: one per character plus the
/// terminator.
fn tokens_len(label: &str) -> usize {
    label.chars().count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::pack::ImageRef;

    #[test]
    fn render_label_reveals_one_glyph_per_character_left_to_right() {
        let pixels = vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
        let image = ImageRef {
            width: assets::fonts::SHEET_WIDTH,
            height: assets::fonts::SHEET_HEIGHT,
            bit_depth: 2,
            pixels: &pixels,
        };
        let sheet = FontGlyphSheet::new(assets::fonts::FontImageRef::new_for_tests(
            FontId::Normal,
            image,
        ))
        .unwrap();

        let glyphs = render_label("AB", sheet);
        assert_eq!(glyphs.len(), 2);
        assert_eq!(glyphs[0].x, 0);
        assert!(
            glyphs[1].x > glyphs[0].x,
            "glyphs must advance left to right"
        );
    }

    #[test]
    fn is_pack_missing_matches_not_found_only() {
        let err = MainMenuSceneError::Pack(PackError::NotFound(std::path::PathBuf::from("x")));
        assert!(err.is_pack_missing());
        let err = MainMenuSceneError::Pack(PackError::UnknownAsset("x".into()));
        assert!(!err.is_pack_missing());
    }

    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_composes_a_non_blank_menu_frame() {
        let scene = load_default().expect("run `cargo xtask extract` first");
        let fb = scene.compose();
        assert!(fb.pixels().iter().any(|&p| p != Rgb888::BLACK));
    }
}
