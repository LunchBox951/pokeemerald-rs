//! Window pixel buffers: the indexed bitmaps upstream's `AddWindow` windows
//! hold, with text printed into them through the engine [`Printer`].

use assets::fonts::OwnedFontGlyphSheet;
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::{char_to_byte, Token};

use super::NamingSceneError;

/// Pixels along one side of a tile.
pub(super) const TILE_PX: usize = 8;
/// `EXT_CTRL_CODE_CLEAR` (`include/constants/characters.h`).
const EXT_CTRL_CODE_CLEAR: u8 = 0x11;
/// Bytes of one packed 4bpp tile.
pub(super) const TILE_BYTES: usize = TILE_PX * TILE_PX / 2;
/// The glyph value that selects the foreground colour.
const GLYPH_FOREGROUND: u8 = 1;
/// The glyph value that selects the shadow colour.
const GLYPH_SHADOW: u8 = 2;

/// The window palette indices a print draws with (`TextPrinterTemplate`'s
/// `fgColor` and `shadowColor`). The background colour is the window's fill:
/// every print here uses a background equal to its window's fill, so glyph
/// pixels that name the background leave the buffer alone.
#[derive(Debug, Clone, Copy)]
pub(super) struct TextColors {
    pub foreground: u8,
    pub shadow: u8,
}

/// An indexed window bitmap, `width` by `height` tiles, one palette index per
/// pixel.
#[derive(Debug, Clone)]
pub(super) struct WindowBuffer {
    width_tiles: usize,
    height_tiles: usize,
    fill: u8,
    pixels: Vec<u8>,
}

impl WindowBuffer {
    /// `FillWindowPixelBuffer`: a window of `fill` pixels.
    pub(super) fn filled(width_tiles: usize, height_tiles: usize, fill: u8) -> Self {
        Self {
            width_tiles,
            height_tiles,
            fill,
            pixels: vec![fill; width_tiles * height_tiles * TILE_PX * TILE_PX],
        }
    }

    fn width_px(&self) -> usize {
        self.width_tiles * TILE_PX
    }

    fn height_px(&self) -> usize {
        self.height_tiles * TILE_PX
    }

    fn set(&mut self, x: i32, y: i32, value: u8) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        if x < self.width_px() && y < self.height_px() {
            let stride = self.width_px();
            self.pixels[y * stride + x] = value;
        }
    }

    fn blit(&mut self, glyph: &RevealedGlyph, colors: TextColors) {
        let side = assets::fonts::GLYPH_SIZE as usize;
        for (index, &value) in glyph.glyph.pixels.iter().enumerate() {
            let color = match value {
                GLYPH_FOREGROUND => colors.foreground,
                GLYPH_SHADOW => colors.shadow,
                _ => continue,
            };
            let (Ok(dx), Ok(dy)) = (i32::try_from(index % side), i32::try_from(index / side))
            else {
                continue;
            };
            self.set(glyph.x + dx, glyph.y + dy, color);
        }
    }

    /// Prints `tokens` at the window-local `origin` with `font`, revealing
    /// every glyph at once (`TEXT_SKIP_DRAW` prints). A `CLEAR` repaints the
    /// span it erases with the window's fill.
    pub(super) fn print(
        &mut self,
        font: &OwnedFontGlyphSheet,
        mut tokens: Vec<Token>,
        origin: (i32, i32),
        colors: TextColors,
    ) {
        tokens.push(Token::End);
        let ticks = tokens.len() + 1;
        let mut printer = Printer::new(tokens, font.sheet(), TextSpeed::Instant, origin);
        for _ in 0..ticks {
            match printer.tick(PrinterInput::none()) {
                TickEvent::Glyph(glyph) => self.blit(&glyph, colors),
                TickEvent::Finished => break,
                _ => {
                    if let Some(span) = printer.cleared_span() {
                        for y in span.y..span.y + span.height {
                            for x in span.x..span.x + span.width {
                                self.set(x, y, self.fill);
                            }
                        }
                    }
                }
            }
        }
    }

    /// The window as packed 4bpp tiles in `PutWindowTilemap` order: row-major
    /// across the window, as `BG` window tile `n` is `baseBlock + n`.
    pub(super) fn tile_bytes(&self) -> Vec<u8> {
        let stride = self.width_px();
        let mut bytes = Vec::with_capacity(self.width_tiles * self.height_tiles * TILE_BYTES);
        for tile_y in 0..self.height_tiles {
            for tile_x in 0..self.width_tiles {
                for row in 0..TILE_PX {
                    let start = (tile_y * TILE_PX + row) * stride + tile_x * TILE_PX;
                    for pair in self.pixels[start..start + TILE_PX].chunks_exact(2) {
                        bytes.push(pair[0] | (pair[1] << 4));
                    }
                }
            }
        }
        bytes
    }
}

/// `{CLEAR width}`.
pub(super) fn clear_token(width: u8) -> Token {
    Token::ExtCtrl {
        sub: EXT_CTRL_CODE_CLEAR,
        args: vec![width],
    }
}

/// `text`'s characters as tokens.
///
/// # Errors
///
/// [`NamingSceneError::UnencodableCharacter`] for a character with no Gen 3
/// encoding, which the printer would otherwise drop silently.
pub(super) fn text_tokens(text: &str) -> Result<Vec<Token>, NamingSceneError> {
    text.chars()
        .map(|c| {
            char_to_byte(c)
                .map(|_| Token::Char(c))
                .ok_or(NamingSceneError::UnencodableCharacter(c))
        })
        .collect()
}
