//! Main menu (I-3, issue #216): the no-save-present path of upstream
//! `pokeemerald/src/main_menu.c`, matching `HAS_NO_SAVED_GAME`'s real
//! geometry, palettes, window frame, and WIN0+`BLDY` darkened-background
//! selection highlight -- not the compact single-item placeholder issue
//! #149/#158 originally shipped (see issue #216's operator playtest
//! evidence for the delta this corrects).
//!
//! # Scope: `HAS_NO_SAVED_GAME` only, both its items
//!
//! Upstream's main menu renders a different item list depending on
//! `gSaveFileStatus` (`Task_MainMenuCheckSaveFile`, `main_menu.c:623-682`):
//! `HAS_NO_SAVED_GAME` shows `NEW GAME`/`OPTION`
//! (`main_menu.c:512`, `HandleMainMenuInput`'s `case HAS_NO_SAVED_GAME`
//! switch, `main_menu.c:952-963`). This crate has no save file on disk yet
//! (`engine::save::store::SaveStore` is not wired into any runtime load
//! path), so `HAS_NO_SAVED_GAME` is the only reachable case -- `CONTINUE`
//! (needs a save to continue) and `MYSTERY GIFT`/`MYSTERY EVENTS` (need a
//! wireless adapter this platform crate doesn't model) are not rendered.
//! [`MainMenuScene`] renders both `HAS_NO_SAVED_GAME` items --
//! [`MainMenuItem::NewGame`] and [`MainMenuItem::Option`] -- with real
//! Up/Down selection (`main_menu.c:903-925`, no wrap). `OPTION`'s own
//! destination screen (`ACTION_OPTION`, `main_menu.c:963`) is a separate,
//! not-yet-built settings screen, out of this issue's scope per its scope
//! notes; [`crate::flow::advance_scene`] swallows an A press on
//! [`MainMenuItem::Option`] instead of launching the intro, so selecting it
//! is inert rather than wrong.
//!
//! # Geometry (`main_menu.c:259-309`)
//!
//! Both items are `MENU_WIDTH` (26) x `MENU_HEIGHT_WIN0`/`MENU_HEIGHT_WIN1`
//! (2) content tiles wide, left-aligned at `MENU_LEFT` (tile 2): `NEW GAME`
//! at `MENU_TOP_WIN0` (tile 1), `OPTION` at `MENU_TOP_WIN1` (tile 5) -- see
//! [`MainMenuItem::top_tile`]. [`draw_item`] fills each content rect, then
//! draws [`AssetPack::text_window_frame`]'s (frame [`FRAME_ID`]) selectable
//! border (`window::border_tiles`, matching upstream's own
//! `DrawMainMenuWindowBorder` -- see `engine::text::window`'s module docs)
//! around the label glyphs.
//! Labels print at the window-local pixel origin upstream's own
//! `AddTextPrinterParameterized3(_, FONT_NORMAL, 0, 1, ...)` calls use
//! (`main_menu.c:786-787`): `x=0`, `y=1`, not a hand-picked inset.
//!
//! # Palettes -- extracted background, hardcoded header text
//!
//! `InitMainMenu` loads `graphics/interface/main_menu_bg.pal` into BG
//! palette bank 0 and `graphics/interface/main_menu_text.pal` into bank 15
//! (`main_menu.c:577-578`) -- but the *content fill and header text* colours
//! `Task_DisplayMainMenu` actually shows for `HAS_NO_SAVED_GAME` never come
//! from `main_menu_text.pal`'s own file bytes: they are three individual
//! `LoadPalette` calls patching bank 15 at runtime, unconditionally,
//! regardless of `gTasks[taskId].tMenuType`
//! (`main_menu.c:755-765`) --
//!
//! | index | patched colour       | source line          | role (`sTextColor_Headers`, `main_menu.c:409`) |
//! |-------|-----------------------|-----------------------|--------------------------------------------------|
//! | `0xA` | `RGB_WHITE`           | `main_menu.c:758`     | `col[0]` bg -- also `FillWindowPixelBuffer`'s `PIXEL_FILL(0xA)` content fill (`main_menu.c:784-785`) |
//! | `0xB` | `RGB(12, 12, 12)`     | `main_menu.c:761`     | `col[1]` fg |
//! | `0xC` | `RGB(26, 26, 25)`     | `main_menu.c:764`     | `col[2]` shadow |
//!
//! [`HEADER_TEXT_BG`]/[`HEADER_TEXT_FG`]/[`HEADER_TEXT_SHADOW`] are these
//! same three literals (translating a table of constants is fine,
//! `no-verbatim`), not read from the pack -- there is nothing in the pack
//! *to* read them from; the file's own bytes at those indices are
//! placeholders `InitMainMenu` always overwrites before the first frame
//! draws. Only palette bank 0's index 0 (the visible BG0 backdrop
//! everywhere no window covers) is real, extracted, per-pack colour --
//! [`MainMenuScene::from_pack`] reads it via the generic
//! [`AssetPack::palette`] accessor (`interface/palette/main_menu_bg`, see
//! `xtask::extract`'s module docs for why the sibling text palette isn't
//! extracted yet).
//!
//! # Selection highlight -- WIN0 + `BLDY` darken, reused, not reinvented
//!
//! `Task_DisplayMainMenu` arms `WIN0`/`WINOUT`/`BLDCNT`/`BLDY`
//! (`main_menu.c:745-753`) so BG0 outside the selected item's `WIN0`
//! rectangle darkens by `BLDY`'s `EVY=7` sixteenths, while
//! `HighlightSelectedMainMenuItem` (`main_menu.c:1170-1187`) slides that
//! rectangle's `WIN0V` to the selected item on every selection change. This
//! crate already ports that exact hardware colour-math formula --
//! [`rendering::darken`], verified against `mgba`'s own `_darken` (see
//! `rendering::effects`' module docs) -- so [`darken_outside`] reuses it
//! directly rather than re-deriving the coefficient math here
//! `(oop-boundaries)`.
//!
//! **The backdrop is never darkened.** `BLDCNT`'s first-target set is
//! `BLDCNT_EFFECT_DARKEN | BLDCNT_TGT1_BG0` (`main_menu.c:751`): BG0 alone,
//! *not* `BLDCNT_TGT1_BD` (`include/gba/io_reg.h:595`), so the darken only
//! ever applies where BG0's own pixel is the topmost non-transparent one.
//! Outside the two menu windows BG0 is fully transparent -- `InitMainMenu`
//! `DmaFill16`s all of VRAM to zero (`main_menu.c:572`) and only those two
//! windows' tiles are ever written back -- so everywhere no window covers,
//! the *backdrop* colour is what shows, at full brightness, both inside and
//! outside `WIN0` (confirmed against issue #216's own mGBA capture: the
//! lavender backdrop between and below the boxes reads identically to the
//! backdrop inside the highlight). [`draw_item`] therefore records which
//! pixels its window draws actually painted, in a [`textbox::Coverage`]
//! map, and [`darken_outside`] darkens only those -- mirroring the
//! BG-vs-backdrop blend-target distinction this workspace's own compositor
//! already models (`LayerKind::Backdrop` gated on
//! `LayerTargets::backdrop`, `crates/rendering/src/compositor.rs`).
//!
//! [`highlight_rect`] itself
//! mirrors `MENU_WIN_HCOORDS`/`MENU_WIN_VCOORDS(n)` (`main_menu.c:283-284`)
//! pixel-for-pixel, not the `WindowRange`-wrap-semantics hardware register
//! encoding those macros compile to -- this scene never needs a wrapped
//! window, so a plain half-open pixel rectangle is the direct, honest
//! equivalent `(behavioral-fidelity)`.
//!
//! # Rendering pipeline
//!
//! Menu labels are rendered once at construction time via a throwaway
//! [`engine::text::render::Printer`] at
//! [`engine::text::render::TextSpeed::Instant`] (menu labels are never
//! paced/typed character-by-character upstream, unlike dialogue -- see
//! [`crate::intro`] for the paced path) and blitted with
//! [`crate::textbox`]'s helpers, the same way [`crate::intro::IntroScene`]
//! draws its dialogue box -- [`textbox::blit_glyphs_colored`] instead of
//! [`textbox::blit_glyphs`], since this scene has its own accurate header
//! colour scheme (module docs above) rather than that function's generic
//! "always legible" stand-in.

use assets::fonts::{FontGlyphSheet, FontId};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window as msgwin;
use engine::text::Token;
use rendering::{Bgr555, Framebuffer, Rgb888};

use crate::textbox::{self, FrameAssets};

/// `MENU_LEFT` (`main_menu.c:259`): every item's content rect starts at
/// this tile column.
const MENU_LEFT: i32 = 2;
/// `MENU_TOP_WIN0` (`main_menu.c:260`): [`MainMenuItem::NewGame`]'s content
/// rect top tile row.
const MENU_TOP_NEW_GAME: i32 = 1;
/// `MENU_TOP_WIN1` (`main_menu.c:261`): [`MainMenuItem::Option`]'s content
/// rect top tile row.
const MENU_TOP_OPTION: i32 = 5;
/// `MENU_WIDTH` (`main_menu.c:267`): every item's content rect width, in
/// tiles.
const MENU_WIDTH: i32 = 26;
/// `MENU_HEIGHT_WIN0`/`MENU_HEIGHT_WIN1` (`main_menu.c:268-269`, identical
/// for both `HAS_NO_SAVED_GAME` items): every item's content rect height,
/// in tiles.
const MENU_HEIGHT: i32 = 2;
/// `MENU_SHADOW_PADDING` (`main_menu.c:281`): the 1px inset
/// [`highlight_rect`] applies on every edge of the bordered box, matching
/// `MENU_WIN_HCOORDS`/`MENU_WIN_VCOORDS`'s own `+ MENU_SHADOW_PADDING` /
/// `- MENU_SHADOW_PADDING` terms (`main_menu.c:283-284`).
const SHADOW_PADDING: i32 = 1;
/// `BLDY`'s `EVY` coefficient `Task_DisplayMainMenu` arms
/// (`main_menu.c:753`, `SetGpuReg(REG_OFFSET_BLDY, 7)`), fed straight to
/// [`rendering::darken`].
const DARKEN_EVY: u8 = 7;

/// `engine::text::window::TILE_SIZE` (8px tiles), as a plain `i32` literal
/// for the tilemap -> screen-pixel conversion this module needs throughout.
const TILE_PX: i32 = 8;
const _: () = assert!(msgwin::TILE_SIZE == 8);

/// `text_window_frame`'s zero-based selectable-frame id this menu uses:
/// `GetWindowFrameTilesPal(gSaveBlock2Ptr->optionsWindowFrameType)`
/// (`main_menu.c:2191-2193`) reads the player's chosen window-frame option,
/// which is `0` (`WINDOW_FRAME_TYPE_0`) for a zeroed, fresh save block --
/// exactly the state this crate's no-save path is always in (module docs).
const FRAME_ID: u8 = 0;

/// The pack id [`MainMenuScene::from_pack`] reads for BG palette bank 0's
/// index 0 -- the visible backdrop colour everywhere no window covers
/// (module docs' palette section). `xtask::extract`'s module docs own the
/// extraction side.
const BACKGROUND_PALETTE_ID: &str = "interface/palette/main_menu_bg";

/// `sTextColor_Headers[0]` after `InitMainMenu`'s runtime patch: `RGB_WHITE`
/// at bank-15 index `0xA` (`main_menu.c:758`) -- both the label's `col[0]`
/// background and `FillWindowPixelBuffer`'s own content fill
/// (`main_menu.c:784-785`, module docs).
const HEADER_TEXT_BG: Rgb888 = Bgr555::from_channels(31, 31, 31).to_rgb888();
/// `sTextColor_Headers[1]`: `RGB(12, 12, 12)` at bank-15 index `0xB`
/// (`main_menu.c:761`) -- the header label's foreground colour.
const HEADER_TEXT_FG: Rgb888 = Bgr555::from_channels(12, 12, 12).to_rgb888();
/// `sTextColor_Headers[2]`: `RGB(26, 26, 25)` at bank-15 index `0xC`
/// (`main_menu.c:764`) -- the header label's shadow colour.
const HEADER_TEXT_SHADOW: Rgb888 = Bgr555::from_channels(26, 26, 25).to_rgb888();

/// The glyph-index -> colour mapping [`textbox::blit_glyphs_colored`] uses
/// for every label this scene draws: upstream's fixed bg/fg/shadow/box font
/// palette order (`assets::fonts`' module docs) mapped through
/// [`HEADER_TEXT_BG`]/[`HEADER_TEXT_FG`]/[`HEADER_TEXT_SHADOW`]. Index 0
/// (bg) is `None` (transparent) rather than an explicit
/// [`HEADER_TEXT_BG`] paint: [`draw_item`] already fills the whole content
/// rect that colour first (`textbox::fill_rect_tracked`, mirroring
/// upstream's own
/// `FillWindowPixelBuffer` call *before* `AddTextPrinterParameterized3`,
/// `main_menu.c:784-787`), so painting it again per-glyph would be
/// redundant, not incorrect. Index 3 (box) is likewise `None`: no
/// `AddTextPrinterParameterized3` caller in this path ever sets a box
/// colour (upstream defaults it to `TEXT_COLOR_TRANSPARENT`).
const HEADER_GLYPH_COLORS: [Option<Rgb888>; 4] =
    [None, Some(HEADER_TEXT_FG), Some(HEADER_TEXT_SHADOW), None];

/// One `HAS_NO_SAVED_GAME` menu item (module docs). No `CONTINUE`/`MYSTERY
/// GIFT`/`MYSTERY EVENTS` variants -- see the module docs' scope section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuItem {
    /// `sWindowTemplates_MainMenu[0]` (`main_menu.c:287-296`):
    /// `gText_MainMenuNewGame` ("NEW GAME", `strings.c:24`). Confirming this
    /// item starts the (already-ported) intro sequence.
    NewGame,
    /// `sWindowTemplates_MainMenu[1]` (`main_menu.c:297-306`):
    /// `gText_MainMenuOption` ("OPTION", `strings.c:26`). Its own settings
    /// screen is out of this issue's scope (module docs).
    Option,
}

impl MainMenuItem {
    /// This item's content rect top tile row (`main_menu.c:260-261`).
    const fn top_tile(self) -> i32 {
        match self {
            Self::NewGame => MENU_TOP_NEW_GAME,
            Self::Option => MENU_TOP_OPTION,
        }
    }

    /// This item's upstream label text (`strings.c:24`/`26`).
    const fn label(self) -> &'static str {
        match self {
            Self::NewGame => "NEW GAME",
            Self::Option => "OPTION",
        }
    }
}

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

/// One item's precomputed label glyphs (module docs' "Rendering pipeline").
#[derive(Debug)]
struct ItemGlyphs {
    item: MainMenuItem,
    glyphs: Vec<RevealedGlyph>,
}

/// The no-save-present main menu (module docs): both `HAS_NO_SAVED_GAME`
/// items in a selectable text window each, with real Up/Down selection and
/// the upstream WIN0+`BLDY` darkened-background highlight.
#[derive(Debug)]
pub struct MainMenuScene {
    frame: FrameAssets,
    background: Rgb888,
    items: [ItemGlyphs; 2],
    selected: MainMenuItem,
}

impl MainMenuScene {
    /// Decode the window frame, the BG0 backdrop colour, and both item
    /// labels out of an already-loaded `pack`.
    ///
    /// # Errors
    ///
    /// [`MainMenuSceneError::Pack`] if the pack or the needed frame/font/
    /// palette entries are missing; [`MainMenuSceneError::Font`] if the
    /// font sheet doesn't decode (unreachable against a real pack).
    pub fn from_pack(pack: &AssetPack) -> Result<Self, MainMenuSceneError> {
        let frame = FrameAssets::from_handle(pack.text_window_frame(FRAME_ID)?);
        let background = textbox::palette_colors(pack.palette(BACKGROUND_PALETTE_ID)?)
            .first()
            .copied()
            .unwrap_or(Rgb888::BLACK);

        let font_image = pack.font(FontId::Normal)?;
        let sheet = FontGlyphSheet::new(font_image)?;
        let items = [
            ItemGlyphs {
                item: MainMenuItem::NewGame,
                glyphs: render_label(MainMenuItem::NewGame.label(), sheet),
            },
            ItemGlyphs {
                item: MainMenuItem::Option,
                glyphs: render_label(MainMenuItem::Option.label(), sheet),
            },
        ];

        Ok(Self {
            frame,
            background,
            items,
            selected: MainMenuItem::NewGame,
        })
    }

    /// The currently selected item (`main_menu.c`'s `tCurrItem`, decoded
    /// back to a [`MainMenuItem`]) -- [`crate::flow::advance_scene`] reads
    /// this to decide what an A press does.
    #[must_use]
    pub const fn selected(&self) -> MainMenuItem {
        self.selected
    }

    /// `HandleMainMenuInput`'s `DPAD_UP` arm (`main_menu.c:903-913`): moves
    /// the selection up one item, unless already on the first
    /// ([`MainMenuItem::NewGame`]) -- `tCurrItem > 0` never wraps.
    pub fn move_up(&mut self) {
        if self.selected == MainMenuItem::Option {
            self.selected = MainMenuItem::NewGame;
        }
    }

    /// `HandleMainMenuInput`'s `DPAD_DOWN` arm (`main_menu.c:915-925`):
    /// moves the selection down one item, unless already on the last
    /// ([`MainMenuItem::Option`]) -- `tCurrItem < tItemCount - 1` never
    /// wraps.
    pub fn move_down(&mut self) {
        if self.selected == MainMenuItem::NewGame {
            self.selected = MainMenuItem::Option;
        }
    }

    /// Composite the menu's background, both item windows, and the
    /// selection highlight into a fresh [`Framebuffer`]. Deterministic:
    /// identical output on every call for the same [`Self::selected`]
    /// (module docs -- no per-frame animation in this slice).
    #[must_use]
    pub fn compose(&self) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(self.background);

        // BG0's own painted pixels -- the only ones `BLDCNT_TGT1_BG0`
        // darkens (module docs' "Selection highlight" section).
        let mut bg0 = textbox::Coverage::recording();
        for item in &self.items {
            draw_item(&mut fb, &mut bg0, &self.frame, item);
        }

        darken_outside(&mut fb, &bg0, highlight_rect(self.selected.top_tile()));

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

/// Draw one item's content fill, border, and label into `fb` (module docs'
/// "Geometry" and "Rendering pipeline" sections) -- `main_menu.c`'s
/// `FillWindowPixelBuffer` + `AddTextPrinterParameterized3` +
/// `DrawMainMenuWindowBorder` sequence for that item's window
/// (`main_menu.c:784-793`) -- recording every pixel it paints into `bg0`,
/// this scene's stand-in for BG0's own non-transparent pixels (module docs'
/// "Selection highlight" section).
fn draw_item(
    fb: &mut Framebuffer,
    bg0: &mut textbox::Coverage,
    frame: &FrameAssets,
    item: &ItemGlyphs,
) {
    let top = item.item.top_tile();
    let content_origin = (MENU_LEFT * TILE_PX, top * TILE_PX);
    let content_size = (MENU_WIDTH * TILE_PX, MENU_HEIGHT * TILE_PX);

    textbox::fill_rect_tracked(
        fb,
        bg0,
        content_origin,
        content_size.0,
        content_size.1,
        HEADER_TEXT_BG,
    );

    let tiles = msgwin::border_tiles(MENU_LEFT, top, MENU_WIDTH, MENU_HEIGHT);
    textbox::blit_frame_tiles_tracked(fb, bg0, &tiles, frame.image(), &frame.palette);

    // `AddTextPrinterParameterized3(_, FONT_NORMAL, 0, 1, ...)`
    // (`main_menu.c:786-787`): window-local pixel origin `(0, 1)`, not a
    // hand-picked inset. The clip is the content rect minus that same 1px
    // `y` offset, so the last glyph row a printer can reach is the content
    // rect's own last row and never the border row below it: upstream's
    // `CopyGlyphToWindow` clamps a glyph to `template->height * 8 -
    // currentY` rows (`src/text.c:611-612`). The binding constraint is the
    // glyph's own height -- `FONT_NORMAL`'s Latin glyphs are 15 rows
    // (`gCurGlyph.height = 15`, `src/text.c:1883`) -- and in this 16px-tall
    // window at `currentY == 1` the window clamp lands on the same 15, so
    // the subtraction below encodes both `(behavioral-fidelity)`.
    let label_origin = (content_origin.0, content_origin.1 + 1);
    let label_clip = (content_size.0, content_size.1 - 1);
    textbox::blit_glyphs_colored_tracked(
        fb,
        bg0,
        &item.glyphs,
        label_origin,
        label_clip,
        &HEADER_GLYPH_COLORS,
    );
}

/// `MENU_WIN_HCOORDS` / `MENU_WIN_VCOORDS(n)` (`main_menu.c:283-284`),
/// evaluated for the item whose content rect starts at tile row
/// `top_tile`: the pixel rectangle `HighlightSelectedMainMenuItem`'s `WIN0`
/// leaves at full brightness (module docs' "Selection highlight" section).
/// Returns `(x0, y0, x1, y1)`, half-open (`x1`/`y1` excluded) -- both items
/// share the same `x` bounds (`MENU_WIN_HCOORDS` doesn't depend on the
/// selected row), only `y` moves.
const fn highlight_rect(top_tile: i32) -> (i32, i32, i32, i32) {
    let x0 = (MENU_LEFT - 1) * TILE_PX + SHADOW_PADDING;
    let x1 = (MENU_LEFT + MENU_WIDTH + 1) * TILE_PX - SHADOW_PADDING;
    let y0 = (top_tile - 1) * TILE_PX + SHADOW_PADDING;
    let y1 = (top_tile + MENU_HEIGHT + 1) * TILE_PX - SHADOW_PADDING;
    (x0, y0, x1, y1)
}

/// Darken every *`bg0`-painted* pixel of `fb` outside `rect` by
/// [`DARKEN_EVY`] sixteenths via [`rendering::darken`] -- the observable
/// result of upstream's `WIN0`/`WINOUT`/`BLDCNT`/`BLDY` register
/// combination, applied directly to the already-composited frame rather
/// than re-deriving a per-layer hardware window pass (this pixel-blit scene
/// has no [`rendering::BgLayer`]/[`rendering::SpriteLayer`] values to run
/// one over -- see [`crate::textbox`]'s module docs for why menu/text
/// chrome is blitted rather than composited).
///
/// Skipping the pixels `bg0` did *not* record is the whole point, not an
/// optimisation: `BLDCNT`'s first-target set names BG0 only, never
/// `BLDCNT_TGT1_BD`, so the backdrop showing through everywhere BG0 is
/// transparent stays at full brightness even outside `rect` (module docs'
/// "Selection highlight" section).
fn darken_outside(fb: &mut Framebuffer, bg0: &textbox::Coverage, rect: (i32, i32, i32, i32)) {
    let (x0, y0, x1, y1) = rect;
    for y in 0..Framebuffer::HEIGHT {
        for x in 0..Framebuffer::WIDTH {
            if !bg0.is_painted(x, y) {
                continue;
            }
            let (Ok(xi), Ok(yi)) = (i32::try_from(x), i32::try_from(y)) else {
                continue;
            };
            if xi >= x0 && xi < x1 && yi >= y0 && yi < y1 {
                continue;
            }
            if let Some(color) = fb.pixel(x, y) {
                fb.set_pixel(x, y, rendering::darken(color, DARKEN_EVY));
            }
        }
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

/// Test-only: a [`MainMenuScene`] built the same synthetic way
/// [`tests`]'s own fixtures are -- a blank glyph sheet, a blank window
/// frame, and a fixed backdrop colour, no local pack needed -- so
/// [`crate::flow`]'s own tests can exercise selection/dispatch without one,
/// mirroring [`crate::intro::synthetic_finished_scene`].
#[cfg(test)]
pub(crate) fn synthetic_scene() -> MainMenuScene {
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

    MainMenuScene {
        frame,
        background: Rgb888::BLACK,
        items: [
            ItemGlyphs {
                item: MainMenuItem::NewGame,
                glyphs: render_label(MainMenuItem::NewGame.label(), sheet),
            },
            ItemGlyphs {
                item: MainMenuItem::Option,
                glyphs: render_label(MainMenuItem::Option.label(), sheet),
            },
        ],
        selected: MainMenuItem::NewGame,
    }
}

#[cfg(test)]
mod tests;
