//! Draws the Options screen's two windows and seven rows
//! (`pokeemerald/src/option_menu.c:90-109,374-395,421-441,618-665`).

use assets::fonts::{FontGlyphSheet, FontId, GlyphSources, OwnedFontGlyphSheet, OwnedGlyphSources};
use assets::pack::{AssetPack, PackError};
use engine::save::{BattleStyle, ButtonMode, SoundMode};
use engine::settings::GameSettings;
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window as msgwin;
use engine::text::Token;
use rendering::{Bgr555, Framebuffer, PaletteColorTransform, PaletteFadeBlend, Rgb888};

use super::OptionsRow;
use crate::textbox::{self, Coverage, FrameAssets};

const TILE_SIZE_PX: i32 = msgwin::TILE_SIZE.cast_signed();
const _: () = assert!(msgwin::TILE_SIZE == 8);

/// `sOptionMenuWinTemplates[WIN_HEADER]` (`option_menu.c:90-98`):
/// tilemap left, top, width, height.
const HEADER_WINDOW: (i32, i32, i32, i32) = (2, 1, 26, 2);
/// `sOptionMenuWinTemplates[WIN_OPTIONS]` (`option_menu.c:99-107`).
const LIST_WINDOW: (i32, i32, i32, i32) = (2, 5, 26, 14);

const ROW_HEIGHT_PX: i32 = 16;
const LABEL_X: i32 = 8;
const TEXT_TOP_OFFSET_PX: i32 = 1;
/// The left edge of the first choice column (`option_menu.c:469`).
const CHOICE_LEFT_X: i32 = 104;
/// The right edge choices align to (`option_menu.c:493`).
const CHOICE_RIGHT_X: i32 = 198;
/// The frame number's x (`option_menu.c:566`).
const FRAME_NUMBER_X: i32 = 128;
/// `WIN_RANGE(16, DISPLAY_WIDTH - 16)` (`option_menu.c:374-379`).
const HIGHLIGHT_X: (i32, i32) = (16, 224);
/// `WIN_RANGE(index * 16 + 40, index * 16 + 56)`.
const HIGHLIGHT_TOP_PX: i32 = 40;
/// `REG_BLDY` (`option_menu.c:187`).
const DARKEN_AMOUNT: u8 = 4;
/// Where the three-way rows put the middle choice (`option_menu.c:464-467`).
const MIDDLE_WIDTH_BIAS: i32 = 94;

// The labels and choices share `option_menu_text.pal`; the screen draws them
// with the font's default fg 2 / shadow 3, and a selected choice swaps in
// TEXT_COLOR_RED / LIGHT_RED (`option_menu.c:380-386`,
// `graphics/interface/option_menu_text.pal`, `text.c:131-140`). The data is
// fixed in upstream's source, so it is spelled out here as main_menu.rs does
// for its header colours.
const LABEL_FG: Rgb888 = Bgr555::from_channels(31, 22, 10).to_rgb888();
const LABEL_SHADOW: Rgb888 = Bgr555::from_channels(24, 15, 0).to_rgb888();
const SELECTED_FG: Rgb888 = Bgr555::from_channels(31, 17, 16).to_rgb888();
const SELECTED_SHADOW: Rgb888 = Bgr555::from_channels(31, 6, 3).to_rgb888();
const UNSELECTED_FG: Rgb888 = Bgr555::from_channels(9, 9, 9).to_rgb888();
const UNSELECTED_SHADOW: Rgb888 = Bgr555::from_channels(26, 26, 25).to_rgb888();
const WINDOW_FILL: Rgb888 = Bgr555::from_channels(31, 31, 31).to_rgb888();
/// `sOptionMenuBg_Pal` (`option_menu.c:135`), the backdrop colour.
const BACKDROP: Rgb888 = Bgr555::from_channels(17, 18, 31).to_rgb888();

const fn glyph_colors(fg: Rgb888, shadow: Rgb888) -> [Option<Rgb888>; 4] {
    [None, Some(fg), Some(shadow), None]
}
const LABEL_COLORS: [Option<Rgb888>; 4] = glyph_colors(LABEL_FG, LABEL_SHADOW);
const SELECTED_COLORS: [Option<Rgb888>; 4] = glyph_colors(SELECTED_FG, SELECTED_SHADOW);
const UNSELECTED_COLORS: [Option<Rgb888>; 4] = glyph_colors(UNSELECTED_FG, UNSELECTED_SHADOW);

/// An error that prevents building the Options screen's assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionsChromeError {
    /// The asset pack could not be loaded or queried.
    Pack(PackError),
    /// The normal font sheet could not be decoded.
    Font(assets::AssetError),
}

impl std::fmt::Display for OptionsChromeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "options: {err}"),
            Self::Font(err) => write!(f, "options: {err}"),
        }
    }
}

impl std::error::Error for OptionsChromeError {}

impl From<PackError> for OptionsChromeError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<assets::AssetError> for OptionsChromeError {
    fn from(err: assets::AssetError) -> Self {
        Self::Font(err)
    }
}

/// A rendered string and its `GetStringWidth`.
pub(super) struct Label {
    glyphs: Vec<RevealedGlyph>,
    pub(super) width: i32,
}

/// The font and every selectable window frame, so a frame change previews
/// without reloading (`option_menu.c:520-523`).
pub(super) struct OptionsChrome {
    font_sources: OwnedGlyphSources,
    frames: Vec<FrameAssets>,
}

impl OptionsChrome {
    pub(super) fn from_pack(pack: &AssetPack) -> Result<Self, OptionsChromeError> {
        let frames = (0..super::WINDOW_FRAMES_COUNT)
            .map(|id| Ok(FrameAssets::from_handle(pack.text_window_frame(id)?)))
            .collect::<Result<Vec<_>, PackError>>()?;
        Ok(Self {
            font_sources: GlyphSources::try_from_fn(|font| {
                Ok::<_, OptionsChromeError>(OwnedFontGlyphSheet::new(pack.font(font)?)?)
            })?,
            frames,
        })
    }

    /// The instant Normal-font printer for `tokens`, carrying every modelled
    /// Latin source.
    pub(super) fn label_printer(&self, tokens: Vec<Token>) -> Printer<FontGlyphSheet<'_>> {
        Printer::with_sources(
            tokens,
            self.font_sources.sheets(),
            FontId::Normal,
            TextSpeed::Instant,
            (0, 0),
        )
    }

    pub(super) fn label(&self, text: &str) -> Label {
        let mut tokens: Vec<Token> = text.chars().map(Token::Char).collect();
        tokens.push(Token::End);
        let ticks = tokens.len();
        let mut printer = self.label_printer(tokens);
        let mut glyphs = Vec::new();
        for _ in 0..=ticks {
            match printer.tick(PrinterInput::none()) {
                TickEvent::Glyph(g) => glyphs.push(*g),
                TickEvent::Finished => break,
                _ => {}
            }
        }
        let width = glyphs
            .iter()
            .map(|g| i32::from(g.glyph.advance_width))
            .sum();
        Label { glyphs, width }
    }

    /// `GetWindowFrameTilesPal`: an unnameable frame id draws frame 0
    /// (`pokeemerald/src/text_window.c`), without being written back.
    fn frame(&self, id: u8) -> &FrameAssets {
        let index = if usize::from(id) < self.frames.len() {
            usize::from(id)
        } else {
            0
        };
        &self.frames[index]
    }

    pub(super) fn compose(
        &self,
        settings: GameSettings,
        selected: OptionsRow,
        fade: Option<PaletteFadeBlend>,
    ) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(BACKDROP);
        let frame = self.frame(settings.window_frame_type);

        // BG1: both borders and the header window, never darkened.
        let mut bg1 = Coverage::recording();
        draw_window_fill_and_border(&mut fb, &mut bg1, HEADER_WINDOW, frame);
        draw_window_border(&mut fb, LIST_WINDOW, frame);
        let header = self.label("OPTION");
        draw_text(
            &mut fb,
            &mut bg1,
            HEADER_WINDOW,
            (LABEL_X, TEXT_TOP_OFFSET_PX),
            &header,
            &LABEL_COLORS,
        );

        // BG0: the list window, darkened outside the highlighted row.
        let mut bg0 = Coverage::recording();
        let (left, top, width, height) = LIST_WINDOW;
        textbox::fill_rect_tracked(
            &mut fb,
            &mut bg0,
            (left * TILE_SIZE_PX, top * TILE_SIZE_PX),
            width * TILE_SIZE_PX,
            height * TILE_SIZE_PX,
            WINDOW_FILL,
        );
        self.draw_rows(&mut fb, &mut bg0, settings);
        // The CPU palette fade rewrites palette entries, so it precedes the
        // hardware BLDY darken (`pokeemerald/src/palette.c:442-449`).
        if let Some(blend) = fade {
            apply_fade(&mut fb, blend);
        }
        darken_outside_row(&mut fb, &bg0, selected);
        fb
    }

    fn draw_rows(&self, fb: &mut Framebuffer, bg0: &mut Coverage, settings: GameSettings) {
        for (row, name) in OptionsRow::ALL.into_iter().zip(ROW_NAMES) {
            let label = self.label(name);
            let y = row_y(row) + TEXT_TOP_OFFSET_PX;
            draw_text(fb, bg0, LIST_WINDOW, (LABEL_X, y), &label, &LABEL_COLORS);
        }
        let mut choice = |row: OptionsRow, text: &str, x: ChoiceX, selected: bool| {
            let label = self.label(text);
            let x = match x {
                ChoiceX::At(x) => x,
                ChoiceX::RightAligned => CHOICE_RIGHT_X - label.width,
            };
            let colors = if selected {
                &SELECTED_COLORS
            } else {
                &UNSELECTED_COLORS
            };
            draw_text(
                fb,
                bg0,
                LIST_WINDOW,
                (x, row_y(row) + TEXT_TOP_OFFSET_PX),
                &label,
                colors,
            );
        };

        // `TextSpeed_DrawChoices` (`option_menu.c:420-441`). A raw value
        // upstream cannot name selects nothing (upstream would index past
        // its three-entry style array).
        let speeds = ["SLOW", "MID", "FAST"];
        self.draw_three_way(
            &mut choice,
            OptionsRow::TextSpeed,
            speeds,
            usize::from(settings.text_speed),
        );
        // `BattleScene_DrawChoices` (`:443-460`) -- `selection` is the
        // OFF flag, so ON is style 0.
        two_way(
            &mut choice,
            OptionsRow::BattleScene,
            ["ON", "OFF"],
            usize::from(settings.battle_scene_off),
        );
        two_way(
            &mut choice,
            OptionsRow::BattleStyle,
            ["SHIFT", "SET"],
            usize::from(settings.battle_style == BattleStyle::Set),
        );
        two_way(
            &mut choice,
            OptionsRow::Sound,
            ["MONO", "STEREO"],
            usize::from(settings.sound == SoundMode::Stereo),
        );
        let button = match settings.button_mode {
            ButtonMode::Normal => 0,
            ButtonMode::Lr => 1,
            ButtonMode::LEqualsA => 2,
            ButtonMode::Other(_) => usize::MAX,
        };
        self.draw_three_way(
            &mut choice,
            OptionsRow::ButtonMode,
            ["NORMAL", "LR", "L=A"],
            button,
        );
        // `FrameType_DrawChoices` (`:541-570`): "TYPE" plain, then the
        // one-based number selected.
        choice(
            OptionsRow::FrameType,
            "TYPE",
            ChoiceX::At(CHOICE_LEFT_X),
            false,
        );
        let number = u16::from(settings.window_frame_type) + 1;
        choice(
            OptionsRow::FrameType,
            &number.to_string(),
            ChoiceX::At(FRAME_NUMBER_X),
            true,
        );
    }

    /// The three-choice layout: first left-aligned at 104, last right
    /// aligned at 198, the middle at `(w0 - (w1 - 94) - w2) / 2 + 104`
    /// (`option_menu.c:434-440,654-662`).
    fn draw_three_way(
        &self,
        choice: &mut impl FnMut(OptionsRow, &str, ChoiceX, bool),
        row: OptionsRow,
        names: [&str; 3],
        selected: usize,
    ) {
        let [first, middle, last] = names.map(|name| self.label(name).width);
        let middle_x = (first - (middle - MIDDLE_WIDTH_BIAS) - last) / 2 + CHOICE_LEFT_X;
        choice(row, names[0], ChoiceX::At(CHOICE_LEFT_X), selected == 0);
        choice(row, names[1], ChoiceX::At(middle_x), selected == 1);
        choice(row, names[2], ChoiceX::RightAligned, selected == 2);
    }
}

/// Where a choice sits horizontally in the list window.
#[derive(Clone, Copy)]
enum ChoiceX {
    At(i32),
    /// `GetStringRightAlignXOffset(.., 198)` (`option_menu.c:493`).
    RightAligned,
}

fn two_way(
    choice: &mut impl FnMut(OptionsRow, &str, ChoiceX, bool),
    row: OptionsRow,
    names: [&str; 2],
    selected: usize,
) {
    choice(row, names[0], ChoiceX::At(CHOICE_LEFT_X), selected == 0);
    choice(row, names[1], ChoiceX::RightAligned, selected == 1);
}

/// `sOptionMenuItemsNames` (`option_menu.c:80-89`), `strings.c:1475-1496`.
const ROW_NAMES: [&str; 7] = [
    "TEXT SPEED",
    "BATTLE SCENE",
    "BATTLE STYLE",
    "SOUND",
    "BUTTON MODE",
    "FRAME",
    "CANCEL",
];

const fn row_y(row: OptionsRow) -> i32 {
    row as i32 * ROW_HEIGHT_PX
}

/// `DrawBgWindowFrames` plus the window's content fill
/// (`option_menu.c:618-665`, `FillWindowPixelBuffer(.., PIXEL_FILL(1))`).
fn draw_window_fill_and_border(
    fb: &mut Framebuffer,
    coverage: &mut Coverage,
    window: (i32, i32, i32, i32),
    frame: &FrameAssets,
) {
    let (left, top, width, height) = window;
    textbox::fill_rect_tracked(
        fb,
        coverage,
        (left * TILE_SIZE_PX, top * TILE_SIZE_PX),
        width * TILE_SIZE_PX,
        height * TILE_SIZE_PX,
        WINDOW_FILL,
    );
    draw_window_border(fb, window, frame);
}

fn draw_window_border(
    fb: &mut Framebuffer,
    (left, top, width, height): (i32, i32, i32, i32),
    frame: &FrameAssets,
) {
    let tiles = msgwin::border_tiles(left, top, width, height);
    textbox::blit_frame_tiles(fb, &tiles, frame.image(), &frame.palette);
}

fn draw_text(
    fb: &mut Framebuffer,
    coverage: &mut Coverage,
    (left, top, width, height): (i32, i32, i32, i32),
    (x, y): (i32, i32),
    label: &Label,
    colors: &[Option<Rgb888>; 4],
) {
    textbox::blit_glyphs_colored_tracked(
        fb,
        coverage,
        &label.glyphs,
        (left * TILE_SIZE_PX + x, top * TILE_SIZE_PX + y),
        (width * TILE_SIZE_PX - x, height * TILE_SIZE_PX - y),
        colors,
    );
}

fn apply_fade(fb: &mut Framebuffer, blend: PaletteFadeBlend) {
    for y in 0..Framebuffer::HEIGHT {
        for x in 0..Framebuffer::WIDTH {
            if let Some(color) = fb.pixel(x, y) {
                fb.set_pixel(x, y, blend.transform(color));
            }
        }
    }
}

/// `HighlightOptionMenuItem` plus the BG0 darken
/// (`option_menu.c:374-379,181-187`): BG0 pixels outside WIN0 darken by
/// `BLDY`; the header, borders, and backdrop are not BG0.
fn darken_outside_row(fb: &mut Framebuffer, bg0: &Coverage, selected: OptionsRow) {
    let row_top = HIGHLIGHT_TOP_PX + row_y(selected);
    for y in 0..Framebuffer::HEIGHT {
        for x in 0..Framebuffer::WIDTH {
            if !bg0.is_painted(x, y) {
                continue;
            }
            let (Ok(px), Ok(py)) = (i32::try_from(x), i32::try_from(y)) else {
                continue;
            };
            let inside = (HIGHLIGHT_X.0..HIGHLIGHT_X.1).contains(&px)
                && (row_top..row_top + ROW_HEIGHT_PX).contains(&py);
            if inside {
                continue;
            }
            if let Some(color) = fb.pixel(x, y) {
                fb.set_pixel(x, y, rendering::darken(color, DARKEN_AMOUNT));
            }
        }
    }
}

#[cfg(test)]
impl OptionsChrome {
    /// A fixture whose font draws exactly one foreground pixel at each
    /// glyph's top-left corner, and whose frame `n` is solid in a colour
    /// unique to `n` ([`Self::synthetic_frame_color`]).
    pub(super) fn synthetic() -> Self {
        use assets::fonts::{FontImageRef, GLYPH_SIZE, SHEET_HEIGHT, SHEET_WIDTH};
        use assets::pack::ImageRef;

        let mut pixels = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
        for y in (0..SHEET_HEIGHT).step_by(GLYPH_SIZE as usize) {
            for x in (0..SHEET_WIDTH).step_by(GLYPH_SIZE as usize) {
                pixels[(y * SHEET_WIDTH + x) as usize] = 1;
            }
        }
        let image = ImageRef {
            width: SHEET_WIDTH,
            height: SHEET_HEIGHT,
            bit_depth: 2,
            pixels: &pixels,
        };
        let font_sources = GlyphSources::try_from_fn(|font| {
            OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(font, image))
        })
        .expect("this is the exact real glyph-sheet shape");
        let frames = (0..super::WINDOW_FRAMES_COUNT)
            .map(|id| {
                let mut palette = vec![Rgb888::BLACK; 16];
                palette[1] = Self::synthetic_frame_color(id);
                FrameAssets {
                    pixels: vec![1u8; 24 * 24],
                    width: 24,
                    height: 24,
                    palette,
                }
            })
            .collect();
        Self {
            font_sources,
            frames,
        }
    }

    pub(super) const fn synthetic_frame_color(id: u8) -> Rgb888 {
        Rgb888 {
            r: id * 8,
            g: 200,
            b: 100,
        }
    }
}

#[cfg(test)]
mod pack_tests {
    use super::OptionsChrome;
    use crate::pack_test_support::{
        assert_carries_every_font, image_entry, pack_bytes_with_fonts, palette_entry, ScratchClaim,
    };
    use assets::pack::AssetPack;

    /// A pack with the frames and every font sheet `OptionsChrome::from_pack`
    /// reads, written to a scratch path removed on drop while it still names
    /// the claimed file.
    struct ScratchPack(ScratchClaim);

    impl ScratchPack {
        fn new() -> Self {
            let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
            let mut entries = Vec::new();
            for id in 0..super::super::WINDOW_FRAMES_COUNT {
                entries.push(image_entry(
                    leak(format!("text-window/image/{}", id + 1)),
                    24,
                    24,
                    4,
                    1,
                ));
                entries.push(palette_entry(
                    leak(format!("text-window/palette/{}", id + 1)),
                    16,
                ));
            }
            let path = std::env::temp_dir().join(format!(
                "pokeemerald-rs-options-chrome-fonts-{}-{:?}.pack",
                std::process::id(),
                std::thread::current().id()
            ));
            Self(ScratchClaim::claim_with(
                path,
                pack_bytes_with_fonts(entries),
            ))
        }
    }

    #[test]
    fn a_label_printer_built_from_a_pack_carries_every_font() {
        let scratch = ScratchPack::new();
        let pack = AssetPack::load(scratch.0.path()).unwrap();
        let chrome = OptionsChrome::from_pack(&pack).expect("the pack carries all five fonts");
        assert_carries_every_font(&chrome.label_printer(Vec::new()));
    }
}
