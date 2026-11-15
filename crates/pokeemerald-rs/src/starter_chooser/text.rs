//! The chooser's BG0 windows: the label, the instruction/confirmation box and
//! the YES/NO menu (`pokeemerald/src/starter_choose.c:63-97`, `:570-604`).
//!
//! BG0 sits above every other layer and is not a colour-effect target, so it
//! is painted straight over the composed frame, the same way the main menu
//! and start menu draw their windows.

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::AssetPack;
use assets::SpeciesNames;
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window as msgwin;
use engine::text::Token;
use rendering::{Framebuffer, Rgb888, WindowRange, WindowRect};

use super::state::{BottomText, ConfirmRow};
use super::{StarterChoice, StarterChooserError};
use crate::textbox::{self, FrameAssets};

/// `gText_BirchInTrouble` and `gText_ConfirmStarterChoice` (`strings.c:177-178`).
const INSTRUCTIONS: &str = "PROF. BIRCH is in trouble!\nRelease a POKéMON and rescue him!";
const CONFIRMATION: &str = "Do you choose this POKéMON?";
/// `sStarterLabelWindowTemplate`'s category suffix (`international_string_util.c:86-90`).
const CATEGORY_SUFFIX: &str = " POKéMON";
/// The Pokédex category of each starter, in [`StarterChoice::ALL`] order
/// (`pokedex_entries.h:3027-3029`, `:3063-3065`, `:3099-3101`). The pack
/// carries no Pokédex payload yet, so the three strings live here.
const CATEGORIES: [&str; 3] = ["WOOD GECKO", "CHICK", "MUD FISH"];
const SELECTOR_ARROW: char = '▶';

const TILE: i32 = msgwin::TILE_SIZE.cast_signed();

/// The instruction/confirmation window, tile origin and size (`:63-71`).
const BOTTOM_WINDOW: Window = Window::new(3, 15, 24, 4);
/// The YES/NO window (`:72-80`).
const MENU_WINDOW: Window = Window::new(24, 9, 5, 4);
/// The label window's size; its origin follows the selection (`:81-97`).
const LABEL_WIDTH_TILES: i32 = 13;
const LABEL_HEIGHT_TILES: i32 = 4;
/// Label tile origins in [`StarterChoice::ALL`] order (`:111-115`).
const LABEL_TILE_ORIGINS: [(i32, i32); 3] = [(0, 9), (16, 10), (8, 4)];
const LABEL_CONTENT_WIDTH_PX: i32 = 104;

const PRINTER_ORIGIN: (i32, i32) = (0, 1);
const MENU_LABEL_X: i32 = 8;
const MENU_ROW_HEIGHT: i32 = 16;
const CATEGORY_Y: i32 = 1;
const NAME_Y: i32 = 17;

/// Message-box palette indices: window fill, then the label's white and
/// light-gray shadow, then the standard dark-gray foreground
/// (`characters.h:234-237`, `starter_choose.c:151`).
const FILL_INDEX: usize = 1;
const STANDARD_FOREGROUND_INDEX: usize = 2;
const SHADOW_INDEX: usize = 3;
const LABEL_FOREGROUND_INDEX: usize = 1;

/// A BG0 window in tiles.
#[derive(Debug, Clone, Copy)]
struct Window {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

impl Window {
    const fn new(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    const fn origin_px(self) -> (i32, i32) {
        (self.left * TILE, self.top * TILE)
    }

    const fn size_px(self) -> (i32, i32) {
        (self.width * TILE, self.height * TILE)
    }
}

/// `WIN0` for a label: its window grown by half a tile horizontally, as the
/// byte registers wrap (`starter_choose.c:596-603`).
pub(super) fn label_window_rect(choice: StarterChoice) -> WindowRect {
    let (left, top) = LABEL_TILE_ORIGINS[choice.ordinal()];
    let register = |value: i32| u8::try_from(value.rem_euclid(256)).unwrap_or(0);
    WindowRect::new(
        WindowRange::new(
            register(left * TILE - TILE / 2),
            register((left + LABEL_WIDTH_TILES) * TILE + TILE / 2),
        ),
        WindowRange::new(
            register(top * TILE),
            register((top + LABEL_HEIGHT_TILES) * TILE),
        ),
    )
}

/// The palette-derived colours one composition paints BG0 with.
pub(super) struct TextColors {
    border: Vec<Rgb888>,
    fill: Rgb888,
    label_glyphs: [Option<Rgb888>; 4],
    standard_glyphs: [Option<Rgb888>; 4],
}

#[derive(Debug)]
struct LabelGlyphs {
    category: Vec<RevealedGlyph>,
    name: Vec<RevealedGlyph>,
}

/// Every string and frame the BG0 windows draw, rendered at load.
#[derive(Debug)]
pub(super) struct TextAssets {
    border: FrameAssets,
    message: FrameAssets,
    instructions: Vec<RevealedGlyph>,
    confirmation: Vec<RevealedGlyph>,
    yes: Vec<RevealedGlyph>,
    no: Vec<RevealedGlyph>,
    cursors: [Vec<RevealedGlyph>; 2],
    labels: [LabelGlyphs; 3],
}

impl TextAssets {
    pub(super) fn from_pack(
        pack: &AssetPack,
        window_frame: u8,
    ) -> Result<Self, StarterChooserError> {
        let normal = OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?;
        let narrow = OwnedFontGlyphSheet::new(pack.font(FontId::Narrow)?)?;
        let border = FrameAssets::from_handle(pack.text_window_frame(window_frame)?);
        let message = FrameAssets::from_handle(pack.message_box()?);

        let names = SpeciesNames::new();
        let mut labels = Vec::with_capacity(StarterChoice::ALL.len());
        for choice in StarterChoice::ALL {
            let category = format!("{}{CATEGORY_SUFFIX}", CATEGORIES[choice.ordinal()]);
            let name = names.name(choice.species_id())?;
            labels.push(LabelGlyphs {
                category: centered(&narrow, &category, CATEGORY_Y),
                name: centered(&normal, name, NAME_Y),
            });
        }
        let Ok(labels) = <[LabelGlyphs; 3]>::try_from(labels) else {
            unreachable!("one label was built per starter")
        };

        let arrow = SELECTOR_ARROW.to_string();
        Ok(Self {
            instructions: render(&normal, INSTRUCTIONS, PRINTER_ORIGIN),
            confirmation: render(&normal, CONFIRMATION, PRINTER_ORIGIN),
            yes: render(&normal, "YES", (MENU_LABEL_X, PRINTER_ORIGIN.1)),
            no: render(
                &normal,
                "NO",
                (MENU_LABEL_X, PRINTER_ORIGIN.1 + MENU_ROW_HEIGHT),
            ),
            cursors: [
                render(&normal, &arrow, (0, PRINTER_ORIGIN.1)),
                render(&normal, &arrow, (0, PRINTER_ORIGIN.1 + MENU_ROW_HEIGHT)),
            ],
            labels,
            border,
            message,
        })
    }

    /// The colours of this load's palettes, each passed through `map` (the
    /// BG palette fade).
    pub(super) fn colors(&self, map: impl Fn(Rgb888) -> Rgb888) -> TextColors {
        let message = |index: usize| self.message.palette.get(index).copied().map(&map);
        TextColors {
            border: self.border.palette.iter().copied().map(&map).collect(),
            fill: message(FILL_INDEX).unwrap_or(Rgb888::BLACK),
            label_glyphs: [
                None,
                message(LABEL_FOREGROUND_INDEX),
                message(SHADOW_INDEX),
                None,
            ],
            standard_glyphs: [
                None,
                message(STANDARD_FOREGROUND_INDEX),
                message(SHADOW_INDEX),
                None,
            ],
        }
    }

    /// Paints the open windows over `fb`.
    pub(super) fn draw(
        &self,
        fb: &mut Framebuffer,
        colors: &TextColors,
        label: Option<StarterChoice>,
        bottom: Option<BottomText>,
        menu: Option<ConfirmRow>,
    ) {
        if let Some(choice) = label {
            self.draw_label(fb, colors, choice);
        }
        if let Some(bottom) = bottom {
            let glyphs = match bottom {
                BottomText::Instructions => &self.instructions,
                BottomText::Confirmation => &self.confirmation,
            };
            self.draw_window(fb, colors, BOTTOM_WINDOW, glyphs);
        }
        if let Some(row) = menu {
            self.draw_window(fb, colors, MENU_WINDOW, &self.yes);
            textbox::blit_glyphs_colored(
                fb,
                &self.no,
                MENU_WINDOW.origin_px(),
                MENU_WINDOW.size_px(),
                &colors.standard_glyphs,
            );
            textbox::blit_glyphs_colored(
                fb,
                &self.cursors[row as usize],
                MENU_WINDOW.origin_px(),
                MENU_WINDOW.size_px(),
                &colors.standard_glyphs,
            );
        }
    }

    fn draw_label(&self, fb: &mut Framebuffer, colors: &TextColors, choice: StarterChoice) {
        let (left, top) = LABEL_TILE_ORIGINS[choice.ordinal()];
        let window = Window::new(left, top, LABEL_WIDTH_TILES, LABEL_HEIGHT_TILES);
        let label = &self.labels[choice.ordinal()];
        for glyphs in [&label.category, &label.name] {
            textbox::blit_glyphs_colored(
                fb,
                glyphs,
                window.origin_px(),
                window.size_px(),
                &colors.label_glyphs,
            );
        }
    }

    /// `DrawStdFrameWithCustomTileAndPalette`: border, fill, then the text.
    fn draw_window(
        &self,
        fb: &mut Framebuffer,
        colors: &TextColors,
        window: Window,
        glyphs: &[RevealedGlyph],
    ) {
        let (width, height) = window.size_px();
        textbox::fill_rect(fb, window.origin_px(), width, height, colors.fill);
        let tiles = msgwin::border_tiles(window.left, window.top, window.width, window.height);
        textbox::blit_frame_tiles(fb, &tiles, self.border.image(), &colors.border);
        textbox::blit_glyphs_colored(
            fb,
            glyphs,
            window.origin_px(),
            window.size_px(),
            &colors.standard_glyphs,
        );
    }
}

/// Prints `text` instantly at a window-local origin.
fn render(sheet: &OwnedFontGlyphSheet, text: &str, origin: (i32, i32)) -> Vec<RevealedGlyph> {
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
    let mut printer = Printer::new(tokens, sheet.sheet(), TextSpeed::Instant, origin);
    let mut glyphs = Vec::new();
    for _ in 0..=ticks {
        match printer.tick(PrinterInput::none()) {
            TickEvent::Glyph(glyph) => glyphs.push(*glyph),
            TickEvent::Finished => break,
            _ => {}
        }
    }
    glyphs
}

/// Prints `text` centred in the label's 104 pixels
/// (`GetStringCenterAlignXOffset`, `international_string_util.c:13-34`).
fn centered(sheet: &OwnedFontGlyphSheet, text: &str, y: i32) -> Vec<RevealedGlyph> {
    let mut glyphs = render(sheet, text, (0, y));
    let width = glyphs
        .iter()
        .map(|glyph| glyph.x + i32::from(glyph.glyph.advance_width))
        .max()
        .unwrap_or(0);
    let offset = (LABEL_CONTENT_WIDTH_PX - width).max(0) / 2;
    for glyph in &mut glyphs {
        glyph.x += offset;
    }
    glyphs
}
