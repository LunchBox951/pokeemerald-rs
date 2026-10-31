//! The naming screen's pack-backed page compositor (I-3).
//!
//! [`NamingScene`] draws one still frame of `naming_screen.c`'s display from
//! the typed [`assets::pack::NamingScreenHandle`]: the menu and window
//! background, the entry title and characters, the avatar, the page-swap,
//! Back and OK buttons, the cursor, the entry markers, and the keyboard pages.
//! A caller states which page is visible and which is on deck
//! ([`PageSelection`]) and what the screen shows ([`NamingVisuals`]); the scene
//! holds no input state, advances no animation, and plays no sound, so
//! cursor movement, page-swap timing, blinking, button flashing, text editing,
//! and confirmation stay with the controller that consumes it.
//!
//! Layer arrangement (`NamingScreen_InitBGs`, `:497-535`): BG0 holds the
//! controls banner, BG1 and BG2 the two keyboard pages, BG3 the background and
//! entry. Upstream always draws two pages -- the current one and the one a
//! swap would show -- and the cursor alpha-blends against whichever is in
//! front (`BLDCNT` target 2 is BG1 and BG2, `BLDALPHA` 12/8).
//!
//! The banner's `{DPAD_NONE}`, `{A_BUTTON}` and `{B_BUTTON}` icons come from
//! `keypad_icons.png`, which the asset pack does not carry yet, so the banner
//! reserves each icon's eight pixels and draws only the words.

mod error;
mod sprites;
mod state;
mod text;

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::NamingScreenHandle;
use assets::{AssetPack, PaletteRef};
use engine::text::Token;
use rendering::{
    compose_frame_with_effects, BgLayer, BgSlot, Bgr555, BitDepth, ColorEffect, EffectsConfig,
    FrameEffects, Framebuffer, LayerTargets, Palette, RenderError, ScreenEntry, SpriteLayer,
    Tilemap, Tileset,
};

pub use error::NamingSceneError;
pub use sprites::CursorVariant;
pub use state::{
    FeedbackPose, KeyboardLayer, NamingAvatar, NamingVisuals, PageSelection, PLAYER_TITLE,
};

use super::{KeyboardPage, KEY_ROWS};
use sprites::{input_base, SpriteArt, SpriteScene};
use text::{clear_token, text_tokens, TextColors, WindowBuffer, TILE_BYTES};

/// Tilemap entries per side of a 32x32 screenblock.
const MAP_SIDE: usize = 32;
/// A window's tile rectangle and the first tile it owns (`sWindowTemplates`).
#[derive(Debug, Clone, Copy)]
struct WindowSpec {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    base_block: u16,
    palette: u8,
}

/// `WIN_KB_PAGE_1`.
const KEYBOARD_1: WindowSpec = window(3, 10, 19, 8, 0x030, 10);
/// `WIN_KB_PAGE_2`.
const KEYBOARD_2: WindowSpec = window(3, 10, 19, 8, 0x0C8, 10);
/// `WIN_TEXT_ENTRY`.
const ENTRY: WindowSpec = window(8, 6, 17, 2, 0x030, 10);
/// `WIN_TEXT_ENTRY_BOX`, the title.
const TITLE: WindowSpec = window(8, 4, 17, 2, 0x052, 10);
/// `WIN_BANNER`.
const BANNER: WindowSpec = window(0, 0, 30, 2, 0x074, 11);

const fn window(
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    base_block: u16,
    palette: u8,
) -> WindowSpec {
    WindowSpec {
        left,
        top,
        width,
        height,
        base_block,
        palette,
    }
}

/// Background palette banks (`LoadPalettes`).
const KEYBOARD_BANK: usize = 10;
const TEXT_WINDOW_BANK: usize = 11;
/// `TEXT_DYNAMIC_COLOR_6`, the banner's fill and background.
const BANNER_FILL: u8 = 15;
/// `PIXEL_FILL(1)` fills the entry and title windows.
const ENTRY_FILL: u8 = 1;
/// The default font colours (`gFonts[FONT_NORMAL]`): foreground 2, shadow 3.
const ENTRY_COLORS: TextColors = TextColors {
    foreground: 2,
    shadow: 3,
};
/// `TEXT_COLOR_WHITE` over `TEXT_COLOR_DARK_GRAY` on keyboards and banner.
const KEY_COLORS: TextColors = TextColors {
    foreground: 1,
    shadow: 2,
};
/// `BLDALPHA_BLEND(12, 8)`.
const CURSOR_EVA: u8 = 12;
const CURSOR_EVB: u8 = 8;
/// The banner text origin (`PrintControls`).
const BANNER_TEXT_ORIGIN: (i32, i32) = (2, 1);
/// Pixels `{DPAD_NONE}`, `{A_BUTTON}` and `{B_BUTTON}` advance.
const KEYPAD_ICON_WIDTH: u8 = 8;
/// The title window's text origin (`DrawNormalTextEntryBox`).
const TITLE_TEXT_ORIGIN: (i32, i32) = (8, 1);
/// Pixels from the keyboard window's left edge to the window of the entry
/// (`inputCharBaseXPos - 0x40`).
const ENTRY_WINDOW_LEFT_PX: i32 = 64;

/// Pack art and decoded resources for the naming screen, built once.
#[derive(Debug)]
pub struct NamingScene {
    normal: OwnedFontGlyphSheet,
    bg_palette: Palette,
    obj_palettes: [Palette; 2],
    menu_tiles: Vec<u8>,
    background: Vec<ScreenEntry>,
    keyboards: [Vec<ScreenEntry>; 3],
    page_windows: [Vec<u8>; 3],
    banner: Vec<u8>,
    sprite_tiles: Tileset,
    art: SpriteArt,
    no_8bpp: Tileset,
}

const fn page_index(page: KeyboardPage) -> usize {
    match page {
        KeyboardPage::Upper => 0,
        KeyboardPage::Lower => 1,
        KeyboardPage::Symbols => 2,
    }
}

fn bank_into(colors: &mut [Bgr555; Palette::LEN], bank: usize, palette: PaletteRef<'_>) {
    for (slot, raw) in colors[bank * Palette::BANK_LEN..]
        .iter_mut()
        .zip(palette.colors())
        .take(Palette::BANK_LEN)
    {
        *slot = Bgr555::from_raw(raw);
    }
}

fn decode_tilemap(raw: &[u8]) -> Vec<ScreenEntry> {
    let mut entries: Vec<ScreenEntry> = raw
        .chunks_exact(2)
        .map(|pair| ScreenEntry::from_raw(u16::from_le_bytes([pair[0], pair[1]])))
        .collect();
    entries.resize(MAP_SIDE * MAP_SIDE, ScreenEntry::from_raw(0));
    entries
}

/// `PutWindowTilemap`: the window's own tiles over its rectangle.
fn put_window(entries: &mut [ScreenEntry], spec: WindowSpec) {
    for dy in 0..spec.height {
        for dx in 0..spec.width {
            let offset = u16::try_from(dy * spec.width + dx).unwrap_or(0);
            entries[(spec.top + dy) * MAP_SIDE + spec.left + dx] =
                ScreenEntry::new(spec.base_block + offset, false, false, spec.palette);
        }
    }
}

fn tileset(parts: &[&[u8]]) -> Result<Tileset, RenderError> {
    Tileset::decode(BitDepth::Bpp4, &parts.concat())
}

fn menu_tile_bytes(handle: &NamingScreenHandle<'_>) -> Vec<u8> {
    let image = handle.menu.image;
    let stride = image.width as usize;
    let (tiles_wide, tiles_high) = (stride / 8, image.height as usize / 8);
    let mut bytes = Vec::with_capacity(tiles_wide * tiles_high * TILE_BYTES);
    for tile_y in 0..tiles_high {
        for tile_x in 0..tiles_wide {
            for row in 0..8 {
                let start = (tile_y * 8 + row) * stride + tile_x * 8;
                for pair in image.pixels[start..start + 8].chunks_exact(2) {
                    bytes.push(pair[0] | (pair[1] << 4));
                }
            }
        }
    }
    bytes
}

impl NamingScene {
    /// Decodes every naming-screen graphic and both fonts from `pack`.
    ///
    /// # Errors
    ///
    /// [`NamingSceneError::Pack`] for a missing or malformed naming-screen or
    /// font entry, [`NamingSceneError::Font`] for a font sheet that does not
    /// decode, and [`NamingSceneError::UnencodableCharacter`] if a keyboard
    /// character has no encoding.
    pub fn from_pack(pack: &AssetPack) -> Result<Self, NamingSceneError> {
        let handle = pack.naming_screen()?;
        let normal = OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?;
        let small = OwnedFontGlyphSheet::new(pack.font(FontId::Small)?)?;
        Self::from_handle(&handle, normal, &small)
    }

    fn from_handle(
        handle: &NamingScreenHandle<'_>,
        normal: OwnedFontGlyphSheet,
        small: &OwnedFontGlyphSheet,
    ) -> Result<Self, NamingSceneError> {
        let mut bg = [Bgr555::default(); Palette::LEN];
        for (bank, palette) in handle.menu_palette_banks().into_iter().enumerate() {
            bank_into(&mut bg, bank, palette);
        }
        bank_into(&mut bg, KEYBOARD_BANK, handle.keyboard_palette);
        bank_into(&mut bg, TEXT_WINDOW_BANK, handle.text_window_palette);

        let mut obj = [Bgr555::default(); Palette::LEN];
        for (bank, palette) in handle.menu_palette_banks().into_iter().enumerate() {
            bank_into(&mut obj, bank, palette);
        }
        bank_into(&mut obj, 6, handle.buttons_palette);
        bank_into(&mut obj, 7, handle.buttons_palette);
        let mut brendan = obj;
        bank_into(&mut brendan, 8, handle.player_icon_brendan.palette);
        let mut may = obj;
        bank_into(&mut may, 8, handle.player_icon_may.palette);

        let (sprite_tiles, art) = SpriteArt::build(handle)?;
        let page_windows = [
            Self::page_window(&normal, KeyboardPage::Upper)?,
            Self::page_window(&normal, KeyboardPage::Lower)?,
            Self::page_window(&normal, KeyboardPage::Symbols)?,
        ];
        let mut banner = WindowBuffer::filled(BANNER.width, BANNER.height, BANNER_FILL);
        banner.print(small, banner_tokens(), BANNER_TEXT_ORIGIN, KEY_COLORS);
        Ok(Self {
            normal,
            bg_palette: Palette::new(bg),
            obj_palettes: [Palette::new(brendan), Palette::new(may)],
            menu_tiles: menu_tile_bytes(handle),
            background: decode_tilemap(handle.background_tilemap),
            keyboards: [
                decode_tilemap(handle.keyboard_upper_tilemap),
                decode_tilemap(handle.keyboard_lower_tilemap),
                decode_tilemap(handle.keyboard_symbols_tilemap),
            ],
            page_windows,
            banner: banner.tile_bytes(),
            sprite_tiles,
            art,
            no_8bpp: Tileset::decode(BitDepth::Bpp8, &[])?,
        })
    }

    /// `PrintKeyboardKeys`: a page's window, filled and printed.
    fn page_window(
        normal: &OwnedFontGlyphSheet,
        page: KeyboardPage,
    ) -> Result<Vec<u8>, NamingSceneError> {
        let mut window =
            WindowBuffer::filled(KEYBOARD_1.width, KEYBOARD_1.height, page.fill_color_index());
        let columns = usize::from(page.column_count());
        for (row, line) in page.rows().iter().enumerate().take(usize::from(KEY_ROWS)) {
            let mut tokens = Vec::new();
            for (gap, c) in page.clear_gaps(row).iter().zip(line.chars().take(columns)) {
                tokens.push(clear_token(*gap));
                tokens.extend(text_tokens(&c.to_string())?);
            }
            let y =
                i32::try_from(row * usize::from(super::page_spec::KEY_ROW_HEIGHT) + 1).unwrap_or(0);
            window.print(normal, tokens, (0, y), KEY_COLORS);
        }
        Ok(window.tile_bytes())
    }

    /// Draws one frame: `selection`'s two pages and `visuals` through the
    /// production compositor.
    ///
    /// Rendering is pure: it reads the arguments and the decoded art and
    /// changes nothing, so equal inputs give equal frames.
    ///
    /// # Errors
    ///
    /// [`NamingSceneError::UnencodableCharacter`] for a title or entry
    /// character with no Gen 3 encoding.
    pub fn render(
        &self,
        selection: PageSelection,
        visuals: &NamingVisuals,
    ) -> Result<Framebuffer, NamingSceneError> {
        let layers = self.layers(selection, visuals)?;
        let palette = &self.bg_palette;
        let bg_slots = [
            BgSlot::new(
                BgLayer::new(&layers.banner_tiles, palette, &layers.maps[0]),
                0,
                0,
                0,
                0,
                true,
            ),
            BgSlot::new(
                BgLayer::new(&layers.keyboard_tiles, palette, &layers.maps[1]),
                1,
                selection.priority_of(KeyboardLayer::Bg1),
                0,
                0,
                true,
            ),
            BgSlot::new(
                BgLayer::new(&layers.keyboard_tiles, palette, &layers.maps[2]),
                2,
                selection.priority_of(KeyboardLayer::Bg2),
                0,
                0,
                true,
            ),
            BgSlot::new(
                BgLayer::new(&layers.entry_tiles, palette, &layers.maps[3]),
                3,
                3,
                0,
                0,
                true,
            ),
        ];

        let avatar = match visuals.avatar {
            NamingAvatar::Brendan => 0,
            NamingAvatar::May => 1,
        };
        let entries = self.art.oam(&sprite_scene(selection, visuals, avatar));
        let sprites = SpriteLayer::new(
            &entries,
            &self.sprite_tiles,
            &self.no_8bpp,
            &self.obj_palettes[avatar],
        );
        Ok(compose_frame_with_effects(
            &sprites,
            &bg_slots,
            &self.effects(),
        ))
    }

    /// `BLDCNT` and `BLDALPHA` (`NamingScreen_InitBGs`, `:524-526`): the
    /// cursor's forced blend, against BG1 and BG2.
    fn effects(&self) -> FrameEffects<'_> {
        FrameEffects {
            color: EffectsConfig {
                effect: ColorEffect::AlphaBlend,
                target1: LayerTargets {
                    bg: [false; 4],
                    obj: false,
                    backdrop: false,
                },
                target2: LayerTargets {
                    bg: [false, true, true, false],
                    obj: false,
                    backdrop: false,
                },
                eva: CURSOR_EVA,
                evb: CURSOR_EVB,
                evy: 0,
            },
            backdrop: self.bg_palette.color(0).to_rgb888(),
            ..FrameEffects::default()
        }
    }

    /// The tiles and tilemaps of the four backgrounds, with this frame's entry
    /// and title printed into their windows.
    fn layers(
        &self,
        selection: PageSelection,
        visuals: &NamingVisuals,
    ) -> Result<Layers, NamingSceneError> {
        let base = input_base(visuals.entry.len());
        let mut entry = WindowBuffer::filled(ENTRY.width, ENTRY.height, ENTRY_FILL);
        for (index, slot) in visuals.entry.iter().enumerate() {
            if let Some(c) = slot {
                let x = i32::try_from(index * 8).unwrap_or(0) + base - ENTRY_WINDOW_LEFT_PX;
                entry.print(
                    &self.normal,
                    text_tokens(&c.to_string())?,
                    (x, 1),
                    ENTRY_COLORS,
                );
            }
        }
        let mut title = WindowBuffer::filled(TITLE.width, TITLE.height, ENTRY_FILL);
        title.print(
            &self.normal,
            text_tokens(&visuals.title)?,
            TITLE_TEXT_ORIGIN,
            ENTRY_COLORS,
        );

        let page_on_bg1 = selection.page_on(KeyboardLayer::Bg1);
        let page_on_bg2 = selection.page_on(KeyboardLayer::Bg2);
        let keyboard_tiles = tileset(&[
            &self.menu_tiles,
            &self.page_windows[page_index(page_on_bg1)],
            &self.page_windows[page_index(page_on_bg2)],
        ])?;
        let entry_tiles = tileset(&[&self.menu_tiles, &entry.tile_bytes(), &title.tile_bytes()])?;
        let mut banner_bytes = vec![0; usize::from(BANNER.base_block) * TILE_BYTES];
        banner_bytes.extend_from_slice(&self.banner);
        let banner_tiles = Tileset::decode(BitDepth::Bpp4, &banner_bytes)?;

        let mut bg1 = self.keyboards[page_index(page_on_bg1)].clone();
        put_window(&mut bg1, KEYBOARD_1);
        let mut bg2 = self.keyboards[page_index(page_on_bg2)].clone();
        put_window(&mut bg2, KEYBOARD_2);
        let mut bg3 = self.background.clone();
        put_window(&mut bg3, ENTRY);
        put_window(&mut bg3, TITLE);
        let mut bg0 = vec![ScreenEntry::from_raw(0); MAP_SIDE * MAP_SIDE];
        put_window(&mut bg0, BANNER);
        let map = |entries| Tilemap::new(MAP_SIDE, MAP_SIDE, entries);
        Ok(Layers {
            banner_tiles,
            keyboard_tiles,
            entry_tiles,
            maps: [map(bg0)?, map(bg1)?, map(bg2)?, map(bg3)?],
        })
    }
}

/// The per-frame tiles and tilemaps of BG0 through BG3.
struct Layers {
    banner_tiles: Tileset,
    /// Shared by BG1 and BG2, as they share a character block.
    keyboard_tiles: Tileset,
    entry_tiles: Tileset,
    maps: [Tilemap; 4],
}

/// What the sprites show this frame.
fn sprite_scene(selection: PageSelection, visuals: &NamingVisuals, avatar: usize) -> SpriteScene {
    let cursor = visuals.cursor.and_then(|(cell, variant)| {
        let (x, y) = selection.visible().cursor_center(cell)?;
        Some(((i32::from(x) - 8, i32::from(y) - 8), variant))
    });
    SpriteScene {
        visible_page: selection.visible(),
        cursor,
        avatar,
        slots: visuals.entry.len(),
        arrow_dx: i32::from(visuals.feedback.arrow_dx),
        underscore_dy: visuals
            .feedback
            .underscore
            .map(|(slot, dy)| (slot, i32::from(dy))),
    }
}

/// `gText_MoveOkBack`, with each keypad icon's width reserved.
fn banner_tokens() -> Vec<Token> {
    let mut tokens = Vec::new();
    for words in ["MOVE  ", "OK  ", "BACK"] {
        tokens.push(clear_token(KEYPAD_ICON_WIDTH));
        tokens.extend(words.chars().map(Token::Char));
    }
    tokens
}

#[cfg(test)]
mod tests;
