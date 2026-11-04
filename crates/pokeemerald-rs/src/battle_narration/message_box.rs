//! The battle message window: `B_WIN_MSG` over the battle textbox background.
//!
//! Battle text prints into a window on BG0 whose surroundings come from the
//! battle textbox tilemap (`LoadBattleTextboxAndBackground`,
//! `src/battle_bg.c:859`-`:865`), not from the field dialogue frame, and it
//! uses the battle window's own palette indices rather than the field ones.

use assets::pack::{BattleTextboxHandle, ImageRef};
use engine::text::window::MessageBoxLayout;
use rendering::{
    BgLayer, Bgr555, BitDepth, Framebuffer, Palette, RenderError, Rgb888, ScreenEntry, Tilemap,
    Tileset,
};

use crate::textbox::{self, WindowOp};

/// `sStandardBattleWindowTemplates[B_WIN_MSG]` (`src/battle_bg.c:165`-`:172`).
pub(super) const MESSAGE_WINDOW: MessageBoxLayout = MessageBoxLayout {
    tilemap_left: 2,
    tilemap_top: 15,
    content_width: 26,
    content_height: 4,
};

/// `sTextOnWindowsInfo_Normal[B_WIN_MSG]` places text at `x = 0`, `y = 1`
/// (`src/battle_message.c:1483`-`:1484`).
pub(super) const PRINTER_ORIGIN: (i32, i32) = (0, 1);

/// The window's `fgColor`, `bgColor`, and `shadowColor`
/// (`src/battle_message.c:1486`-`:1488`), indices into palette bank 0.
const FOREGROUND_INDEX: u8 = 1;
const BACKGROUND_INDEX: u8 = 15;
const SHADOW_INDEX: u8 = 6;

const TILE_PX: i32 = 8;
/// Glyph pixel roles a font sheet uses: 1 is the body, 2 the shadow.
const GLYPH_FOREGROUND_ROLE: usize = 1;
const GLYPH_SHADOW_ROLE: usize = 2;
const TEXTBOX_MAP_SIDE_TILES: (usize, usize) = (64, 32);
const BYTES_PER_MAP_ENTRY: usize = 2;
const TILE_DIM: usize = 8;

/// The battle textbox graphics, decoded once.
#[derive(Debug, Clone)]
pub(super) struct BattleTextbox {
    tileset: Tileset,
    palette: Palette,
    tilemap: Tilemap,
}

impl BattleTextbox {
    /// Decodes the pack's battle textbox: the 128x128 4bpp tile sheet, the two
    /// palette banks, and the 64x32 tilemap.
    pub(super) fn from_handle(handle: BattleTextboxHandle<'_>) -> Result<Self, RenderError> {
        let mut colors = [Bgr555::default(); Palette::LEN];
        for (slot, raw) in colors.iter_mut().zip(handle.palette.colors()) {
            *slot = Bgr555::from_raw(raw);
        }
        let entries = handle
            .tilemap
            .chunks_exact(BYTES_PER_MAP_ENTRY)
            .map(|pair| ScreenEntry::from_raw(u16::from_le_bytes([pair[0], pair[1]])))
            .collect();
        Ok(Self {
            tileset: Tileset::decode(BitDepth::Bpp4, &packed_tiles(handle.image))?,
            palette: Palette::new(colors),
            tilemap: Tilemap::new(TEXTBOX_MAP_SIDE_TILES.0, TEXTBOX_MAP_SIDE_TILES.1, entries)?,
        })
    }

    #[cfg(test)]
    pub(super) const fn from_parts(tileset: Tileset, palette: Palette, tilemap: Tilemap) -> Self {
        Self {
            tileset,
            palette,
            tilemap,
        }
    }

    /// Draws the textbox, blanks the message window to its background, and
    /// replays `ops` inside it.
    pub(super) fn compose(&self, framebuffer: &mut Framebuffer, ops: &[WindowOp]) {
        BgLayer::new(&self.tileset, &self.palette, &self.tilemap).composite(framebuffer);
        let (origin, size) = window_rect();
        textbox::fill_rect(
            framebuffer,
            origin,
            size.0,
            size.1,
            self.color(BACKGROUND_INDEX),
        );
        let mut roles = [None; 4];
        roles[GLYPH_FOREGROUND_ROLE] = Some(self.color(FOREGROUND_INDEX));
        roles[GLYPH_SHADOW_ROLE] = Some(self.color(SHADOW_INDEX));
        textbox::compose_window_ops(framebuffer, ops, origin, size, &roles);
    }

    fn color(&self, index: u8) -> Rgb888 {
        self.palette.bank_color(0, index).to_rgb888()
    }
}

/// The window's screen origin and pixel size.
fn window_rect() -> ((i32, i32), (i32, i32)) {
    let layout = MESSAGE_WINDOW;
    (
        (layout.tilemap_left * TILE_PX, layout.tilemap_top * TILE_PX),
        (
            layout.content_width * TILE_PX,
            layout.content_height * TILE_PX,
        ),
    )
}

/// Repacks an indexed image into 4bpp tiles in row-major tile order, the left
/// pixel in the low nibble. The pack validates 4-bit indices and tile-aligned
/// dimensions.
fn packed_tiles(image: ImageRef<'_>) -> Vec<u8> {
    let width = usize::try_from(image.width).unwrap_or(0);
    let height = usize::try_from(image.height).unwrap_or(0);
    let mut packed = Vec::with_capacity(width * height / 2);
    for tile_row in 0..height / TILE_DIM {
        for tile_col in 0..width / TILE_DIM {
            for y in 0..TILE_DIM {
                let start = (tile_row * TILE_DIM + y) * width + tile_col * TILE_DIM;
                for pair in image.pixels[start..start + TILE_DIM].chunks_exact(2) {
                    packed.push(pair[0] | (pair[1] << 4));
                }
            }
        }
    }
    packed
}
