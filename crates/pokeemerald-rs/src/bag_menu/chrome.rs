//! Owned bag-screen graphics decoded once at open, and the pixel primitives
//! the composition draws them with.

use std::collections::BTreeMap;

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::items::ItemId;
use assets::pack::{AssetPack, PalettedImage};
use engine::bag_menu::BagStack;
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window as msgwin;
use engine::text::{Symbol, Token};
use rendering::{AffineMatrix, Framebuffer, Rgb888};

use super::BagScreenError;
use crate::textbox::{self, FrameAssets};

/// `ITEM_LIST_END`, the identity upstream gives the close row's icon.
const LIST_END: ItemId = ItemId(u16::MAX);
const TILE_PX: i32 = msgwin::TILE_SIZE.cast_signed();
const TILES_PER_SHEET_ROW: u32 = 16;
const TILEMAP_ROW_ENTRIES: usize = 32;
const TILEMAP_ENTRY_BYTES: usize = 2;
const TILE_INDEX_MASK: u16 = 0x03FF;
const H_FLIP_BIT: u16 = 0x0400;
const V_FLIP_BIT: u16 = 0x0800;
const PALETTE_BANK_SHIFT: u16 = 12;
const BANK_COLORS: usize = 16;
const WINDOW_TEXT_INDEX: usize = 1;
const WINDOW_SHADOW_INDEX: usize = 4;
const NORMAL_SHADOW_INDEX: usize = 3;
/// `floor(256 * sin(2*pi*k/256))` for `k` in 0..=64: the first quarter turn of
/// the 256-unit angle the rotating ball turns by, truncated as the hardware's
/// sine table is.
const QUARTER_SINE: [i16; 65] = [
    0, 6, 12, 18, 25, 31, 37, 43, 49, 56, 62, 68, 74, 80, 86, 92, 97, 103, 109, 115, 120, 126, 131,
    136, 142, 147, 152, 157, 162, 167, 171, 176, 181, 185, 189, 193, 197, 201, 205, 209, 212, 216,
    219, 222, 225, 228, 231, 234, 236, 238, 241, 243, 244, 246, 248, 249, 251, 252, 253, 254, 254,
    255, 255, 255, 256,
];
const QUARTER_TURN: u8 = 64;
const HALF_TURN: usize = 128;
const FULL_TURN: usize = 256;
/// Stands in a label for `{NO}`, the extra-symbol glyph `F9 08`.
pub(super) const NO_SYMBOL: char = '\u{E008}';
const NO_EXTRA_SYMBOL: u8 = 0x08;
/// Stands in a label for `CHAR_SPACER` (`0x77`), which `ConvertIntToDecimalStringN`
/// writes for a leading position of a right-aligned number
/// (`string_util.c:209`). Its narrow-font advance is 5 pixels, where a space
/// advances 3.
pub(super) const SPACER_SYMBOL: char = '\u{E077}';
/// `{POKEBLOCK}` in a string: the five glyphs `0x55`-`0x59`
/// (`charmap.txt:52`).
const POKEBLOCK_PLACEHOLDER: &str = "{POKEBLOCK}";
const POKEBLOCK_GLYPHS: [Symbol; 5] = [
    Symbol::Pokeblock1,
    Symbol::Pokeblock2,
    Symbol::Pokeblock3,
    Symbol::Pokeblock4,
    Symbol::Pokeblock5,
];

/// A palette-indexed picture with its palette decoded; index 0 is transparent.
#[derive(Debug)]
pub(super) struct Bitmap {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    palette: Vec<Rgb888>,
}

impl Bitmap {
    fn from_paletted(source: PalettedImage<'_>) -> Self {
        Self {
            width: source.image.width as usize,
            height: source.image.height as usize,
            pixels: source.image.pixels.to_vec(),
            palette: textbox::palette_colors(source.palette),
        }
    }

    /// The colour at a pixel, or `None` off the picture or at index 0.
    fn color_at(&self, x: i32, y: i32) -> Option<Rgb888> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        if x >= self.width || y >= self.height {
            return None;
        }
        match *self.pixels.get(y * self.width + x)? {
            0 => None,
            index => self.palette.get(usize::from(index)).copied(),
        }
    }

    /// Draws the `rows`-tall band of rows starting at `first_row`, with its
    /// top-left at `origin`.
    pub(super) fn blit_rows(
        &self,
        fb: &mut Framebuffer,
        origin: (i32, i32),
        first_row: usize,
        rows: usize,
    ) {
        for y in 0..rows {
            for x in 0..self.width {
                let Some(&index) = self.pixels.get((first_row + y) * self.width + x) else {
                    continue;
                };
                if index == 0 {
                    continue;
                }
                if let Some(&color) = self.palette.get(usize::from(index)) {
                    put_at(fb, origin, (x, y), color);
                }
            }
        }
    }
}

/// `sin` of a 256-unit angle in 8.8 fixed point.
fn sine(angle: u8) -> i16 {
    let at = usize::from(angle);
    match at {
        0..=64 => QUARTER_SINE[at],
        65..=128 => QUARTER_SINE[HALF_TURN - at],
        129..=192 => -QUARTER_SINE[at - HALF_TURN],
        _ => -QUARTER_SINE[FULL_TURN - at],
    }
}

/// The matrix that turns a sprite visibly by `angle` (a positive angle is
/// counter-clockwise), mapping screen offsets to source offsets as the
/// hardware does.
fn rotation(angle: u8) -> AffineMatrix {
    let (sin, cos) = (sine(angle), sine(angle.wrapping_add(QUARTER_TURN)));
    AffineMatrix::new(cos, -sin, sin, cos)
}

/// Draws one pixel at `origin` plus an unsigned offset.
fn put_at(fb: &mut Framebuffer, origin: (i32, i32), offset: (usize, usize), color: Rgb888) {
    let shift = |base: i32, by: usize| base.saturating_add(i32::try_from(by).unwrap_or(i32::MAX));
    put(
        fb,
        shift(origin.0, offset.0),
        shift(origin.1, offset.1),
        color,
    );
}

fn put(fb: &mut Framebuffer, x: i32, y: i32, color: Rgb888) {
    if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) {
        if x < Framebuffer::WIDTH && y < Framebuffer::HEIGHT {
            fb.set_pixel(x, y, color);
        }
    }
}

/// Every graphic and font the screen draws.
#[derive(Debug)]
pub(super) struct BagChrome {
    normal: OwnedFontGlyphSheet,
    narrow: OwnedFontGlyphSheet,
    window: FrameAssets,
    message: FrameAssets,
    tile_sheet: Bitmap,
    tilemap: Vec<u8>,
    /// Both 16-colour banks of the screen palette, in bank order.
    screen_palette: Vec<Rgb888>,
    bag: Bitmap,
    ball: Bitmap,
    pub(super) hm_mark: Bitmap,
    pub(super) registered_mark: Bitmap,
    icons: BTreeMap<u16, Bitmap>,
}

impl BagChrome {
    /// Decodes the fixed graphics and the icon of every item in `stacks`
    /// plus the close row's.
    ///
    /// # Errors
    ///
    /// [`BagScreenError`] when a pack entry is missing or malformed or a font
    /// sheet does not decode.
    pub(super) fn from_pack(
        pack: &AssetPack,
        window_frame: u8,
        female: bool,
        stacks: impl Iterator<Item = BagStack>,
    ) -> Result<Self, BagScreenError> {
        let bag = pack.bag_assets()?;
        let palette = if female {
            bag.screen_female_palette
        } else {
            bag.screen_male_palette
        };
        let mut icons = BTreeMap::new();
        for item in std::iter::once(LIST_END).chain(stacks.map(|stack| stack.item)) {
            if let std::collections::btree_map::Entry::Vacant(slot) = icons.entry(item.0) {
                slot.insert(Bitmap::from_paletted(bag.item_icon(item)?));
            }
        }
        let screen_palette = textbox::palette_colors(palette);
        // The marks index the list window's palette, which the pack does not
        // carry; the screen palette's second bank is the nearest it supplies.
        let flat = |image: assets::pack::ImageRef<'_>| Bitmap {
            width: image.width as usize,
            height: image.height as usize,
            pixels: image.pixels.to_vec(),
            palette: screen_palette
                .get(BANK_COLORS..)
                .unwrap_or_default()
                .to_vec(),
        };
        Ok(Self {
            normal: OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?,
            narrow: OwnedFontGlyphSheet::new(pack.font(FontId::Narrow)?)?,
            window: FrameAssets::from_handle(pack.text_window_frame(window_frame)?),
            message: FrameAssets::from_handle(pack.message_box()?),
            tile_sheet: flat(bag.screen),
            tilemap: bag.screen_tilemap.to_vec(),
            screen_palette: screen_palette.clone(),
            bag: Bitmap::from_paletted(if female { bag.female } else { bag.male }),
            ball: Bitmap::from_paletted(bag.rotating_ball),
            hm_mark: flat(bag.hm),
            registered_mark: flat(bag.registered_select),
            icons,
        })
    }

    /// The icon decoded for `item`, or the close row's for `None`.
    pub(super) fn icon(&self, item: Option<ItemId>) -> Option<&Bitmap> {
        self.icons.get(&item.unwrap_or(LIST_END).0)
    }

    /// Draws the 30x20-tile screen tilemap (`item_menu.c:726-733`'s `menu.bin`);
    /// index 0 stays transparent so the supplied frame shows through.
    pub(super) fn draw_background(&self, fb: &mut Framebuffer) {
        for row in 0..Framebuffer::HEIGHT / msgwin::TILE_SIZE as usize {
            for col in 0..Framebuffer::WIDTH / msgwin::TILE_SIZE as usize {
                let at = (row * TILEMAP_ROW_ENTRIES + col) * TILEMAP_ENTRY_BYTES;
                if let Some(pair) = self.tilemap.get(at..at + TILEMAP_ENTRY_BYTES) {
                    self.draw_tile(fb, (col, row), u16::from_le_bytes([pair[0], pair[1]]));
                }
            }
        }
    }

    /// Draws one tilemap entry (tile, flips, palette bank) at a tile cell.
    pub(super) fn draw_tile(&self, fb: &mut Framebuffer, cell: (usize, usize), entry: u16) {
        let tile = u32::from(entry & TILE_INDEX_MASK);
        let bank = usize::from(entry >> PALETTE_BANK_SHIFT);
        let (tile_x, tile_y) = (
            (tile % TILES_PER_SHEET_ROW) as usize,
            (tile / TILES_PER_SHEET_ROW) as usize,
        );
        for y in 0..8usize {
            for x in 0..8usize {
                let sx = if entry & H_FLIP_BIT == 0 { x } else { 7 - x };
                let sy = if entry & V_FLIP_BIT == 0 { y } else { 7 - y };
                let at = (tile_y * 8 + sy) * self.tile_sheet.width + tile_x * 8 + sx;
                let Some(&index) = self.tile_sheet.pixels.get(at) else {
                    continue;
                };
                if index == 0 {
                    continue;
                }
                if let Some(&color) = self
                    .screen_palette
                    .get(bank * BANK_COLORS + usize::from(index))
                {
                    put_at(fb, (0, 0), (cell.0 * 8 + x, cell.1 * 8 + y), color);
                }
            }
        }
    }

    /// Draws the bag picture: frame 0 is the idle bag, frames 1-5 the pockets.
    pub(super) fn draw_bag(&self, fb: &mut Framebuffer, origin: (i32, i32), frame: usize) {
        let side = self.bag.width;
        if (frame + 1) * side <= self.bag.height {
            self.bag.blit_rows(fb, origin, frame * side, side);
        }
    }

    /// Draws the rotating ball turned by `angle` about its centre: each
    /// pixel of the square is looked up from the source under the rotation,
    /// as the hardware's affine sprites do, so no turn accumulates error.
    pub(super) fn draw_ball(&self, fb: &mut Framebuffer, top_left: (i32, i32), angle: u8) {
        let ball = &self.ball;
        let half = i32::try_from(ball.width / 2).unwrap_or(0);
        let matrix = rotation(angle);
        for y in -half..half {
            for x in -half..half {
                let (source_x, source_y) = matrix.apply(x, y);
                let column = half + (source_x >> AffineMatrix::FRAC_BITS);
                let row = half + (source_y >> AffineMatrix::FRAC_BITS);
                if let Some(color) = ball.color_at(column, row) {
                    put(fb, top_left.0 + half + x, top_left.1 + half + y, color);
                }
            }
        }
    }

    /// Draws a standard bordered window at a tilemap rectangle.
    pub(super) fn draw_window(&self, fb: &mut Framebuffer, tiles: (i32, i32, i32, i32)) {
        let (left, top, width, height) = tiles;
        let fill = self
            .message
            .palette
            .get(1)
            .copied()
            .unwrap_or(Rgb888::BLACK);
        textbox::fill_rect(
            fb,
            (left * TILE_PX, top * TILE_PX),
            width * TILE_PX,
            height * TILE_PX,
            fill,
        );
        let border = msgwin::border_tiles(left, top, width, height);
        textbox::blit_frame_tiles(fb, &border, self.window.image(), &self.window.palette);
    }

    /// The pocket-name colours: screen palette bank 1 index 1 text over index 4
    /// shadow (`COLORID_POCKET_NAME`, `item_menu.c:387`).
    pub(super) fn window_text_colors(&self) -> [Option<Rgb888>; 4] {
        self.bank_1_colors(WINDOW_SHADOW_INDEX)
    }

    /// The list, cursor, and description colours (`COLORID_NORMAL`,
    /// `item_menu.c:386`): bank 1 index 1 text over index 3 shadow.
    pub(super) fn normal_text_colors(&self) -> [Option<Rgb888>; 4] {
        self.bank_1_colors(NORMAL_SHADOW_INDEX)
    }

    fn bank_1_colors(&self, shadow: usize) -> [Option<Rgb888>; 4] {
        let color = |index: usize| self.screen_palette.get(BANK_COLORS + index).copied();
        [None, color(WINDOW_TEXT_INDEX), color(shadow), None]
    }

    /// The message-box glyph colours every list and window text uses.
    pub(super) fn text_colors(&self) -> [Option<Rgb888>; 4] {
        self.message.glyph_colors()
    }

    pub(super) fn normal_text(&self, text: &str) -> Vec<RevealedGlyph> {
        render(&self.normal, text)
    }

    pub(super) fn narrow_text(&self, text: &str) -> Vec<RevealedGlyph> {
        render(&self.narrow, text)
    }
}

/// Total advance of rendered glyphs, in pixels.
pub(super) fn width_of(glyphs: &[RevealedGlyph]) -> i32 {
    glyphs
        .iter()
        .map(|g| i32::from(g.glyph.advance_width))
        .sum()
}

/// The tokens of `text`: `\n` breaks the line, [`NO_SYMBOL`] and
/// [`SPACER_SYMBOL`] stand for their glyphs, and `{POKEBLOCK}` expands to its
/// five glyphs.
pub(super) fn tokens_of(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix(POKEBLOCK_PLACEHOLDER) {
            tokens.extend(POKEBLOCK_GLYPHS.map(Token::Symbol));
            rest = after;
            continue;
        }
        tokens.push(match c {
            '\n' => Token::Newline,
            NO_SYMBOL => Token::ExtraSymbol(NO_EXTRA_SYMBOL),
            SPACER_SYMBOL => Token::Symbol(Symbol::Spacer),
            c => Token::Char(c),
        });
        rest = &rest[c.len_utf8()..];
    }
    tokens
}

/// Renders every glyph of `text` immediately.
fn render(font: &OwnedFontGlyphSheet, text: &str) -> Vec<RevealedGlyph> {
    let mut tokens = tokens_of(text);
    tokens.push(Token::End);
    let ticks = tokens.len();
    let mut printer = Printer::new(tokens, font.sheet(), TextSpeed::Instant, (0, 0));
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
