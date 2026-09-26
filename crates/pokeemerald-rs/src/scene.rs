//! A deterministic tiled scene for headless boot checks.

use rendering::{
    compose_frame, BgLayer, BgSlot, Bgr555, BitDepth, Framebuffer, OamEntry, ObjShape, Palette,
    ScreenEntry, SpriteLayer, Tilemap, Tileset,
};

const BACKGROUND_WIDTH_TILES: usize = Framebuffer::WIDTH / BitDepth::TILE_DIM;
const BACKGROUND_HEIGHT_TILES: usize = Framebuffer::HEIGHT / BitDepth::TILE_DIM;
const LIGHT_BACKGROUND_TILE_INDEX: u16 = 0;
const DARK_BACKGROUND_TILE_INDEX: u16 = 1;
const LIGHT_BACKGROUND_PALETTE_INDEX: u8 = 1;
const DARK_BACKGROUND_PALETTE_INDEX: u8 = 2;
const LIGHT_BACKGROUND_COLOR: Bgr555 = Bgr555::from_channels(6, 10, 18);
const DARK_BACKGROUND_COLOR: Bgr555 = Bgr555::from_channels(4, 7, 12);
const BACKGROUND_LAYER_INDEX: u8 = 0;
const BACKGROUND_PRIORITY: u8 = 3;
const BACKGROUND_PALETTE_BANK: u8 = 0;
const BACKGROUND_SCROLL: u16 = 0;

const SOLID_SPRITE_TILE_INDEX: u16 = 0;
const SOLID_SPRITE_PALETTE_INDEX: u8 = 15;
const SQUARE_8X8_OBJ_SIZE: u8 = 0;
const FRONT_SPRITE_COLOR: Bgr555 = Bgr555::from_channels(31, 4, 4);
const MIDDLE_SPRITE_COLOR: Bgr555 = Bgr555::from_channels(4, 31, 4);
const BACK_SPRITE_COLOR: Bgr555 = Bgr555::from_channels(4, 4, 31);

#[derive(Clone, Copy)]
struct SpritePlacement {
    x: u16,
    y: u8,
    palette_bank: u8,
    priority: u8,
}

const FRONT_SPRITE: SpritePlacement = SpritePlacement {
    x: 100,
    y: 76,
    palette_bank: 0,
    priority: 0,
};
const MIDDLE_SPRITE: SpritePlacement = SpritePlacement {
    x: 104,
    y: 80,
    palette_bank: 1,
    priority: 1,
};
const BACK_SPRITE: SpritePlacement = SpritePlacement {
    x: 108,
    y: 84,
    palette_bank: 2,
    priority: 2,
};

/// The assets and sprite placements in the synthetic boot scene.
#[derive(Debug)]
pub struct BootScene {
    bg_tileset: Tileset,
    bg_palette: Palette,
    bg_tilemap: Tilemap,
    sprite_tileset: Tileset,
    sprite_palette: Palette,
    sprites: Vec<OamEntry>,
}

impl BootScene {
    /// Creates the deterministic checkerboard and overlapping sprites.
    #[must_use]
    pub fn new() -> Self {
        let (bg_tileset, bg_palette, bg_tilemap) = build_background();
        let (sprite_tileset, sprite_palette, sprites) = build_sprites();
        Self {
            bg_tileset,
            bg_palette,
            bg_tilemap,
            sprite_tileset,
            sprite_palette,
            sprites,
        }
    }

    /// Composes the scene into a fresh framebuffer.
    #[must_use]
    pub fn compose(&self) -> Framebuffer {
        let bg_layer = BgLayer::new(&self.bg_tileset, &self.bg_palette, &self.bg_tilemap);
        let bg_slot = BgSlot::new(
            bg_layer,
            BACKGROUND_LAYER_INDEX,
            BACKGROUND_PRIORITY,
            BACKGROUND_SCROLL,
            BACKGROUND_SCROLL,
            true,
        );
        let sprites = SpriteLayer::new(
            &self.sprites,
            &self.sprite_tileset,
            &self.sprite_tileset,
            &self.sprite_palette,
        );
        compose_frame(&sprites, &[bg_slot])
    }
}

impl Default for BootScene {
    fn default() -> Self {
        Self::new()
    }
}

fn build_background() -> (Tileset, Palette, Tilemap) {
    let tile_data = [
        solid_4bpp_tile(LIGHT_BACKGROUND_PALETTE_INDEX),
        solid_4bpp_tile(DARK_BACKGROUND_PALETTE_INDEX),
    ]
    .concat();
    let bg_tileset = Tileset::decode(BitDepth::Bpp4, &tile_data)
        .expect("the two complete 4bpp tiles must decode");

    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[usize::from(LIGHT_BACKGROUND_PALETTE_INDEX)] = LIGHT_BACKGROUND_COLOR;
    colors[usize::from(DARK_BACKGROUND_PALETTE_INDEX)] = DARK_BACKGROUND_COLOR;
    let bg_palette = Palette::new(colors);

    let mut entries = Vec::with_capacity(BACKGROUND_WIDTH_TILES * BACKGROUND_HEIGHT_TILES);
    for row in 0..BACKGROUND_HEIGHT_TILES {
        for column in 0..BACKGROUND_WIDTH_TILES {
            let tile_index = if (column + row) % 2 == 0 {
                LIGHT_BACKGROUND_TILE_INDEX
            } else {
                DARK_BACKGROUND_TILE_INDEX
            };
            entries.push(ScreenEntry::new(
                tile_index,
                false,
                false,
                BACKGROUND_PALETTE_BANK,
            ));
        }
    }
    let bg_tilemap = Tilemap::new(BACKGROUND_WIDTH_TILES, BACKGROUND_HEIGHT_TILES, entries)
        .expect("the checkerboard has one entry per background tile");

    (bg_tileset, bg_palette, bg_tilemap)
}

fn solid_4bpp_tile(palette_index: u8) -> [u8; BitDepth::Bpp4.tile_byte_len()] {
    assert!(usize::from(palette_index) < Palette::BANK_LEN);
    let paired_pixels = palette_index | (palette_index << 4);
    [paired_pixels; BitDepth::Bpp4.tile_byte_len()]
}

fn build_sprites() -> (Tileset, Palette, Vec<OamEntry>) {
    let tile_data = solid_4bpp_tile(SOLID_SPRITE_PALETTE_INDEX);
    let sprite_tileset =
        Tileset::decode(BitDepth::Bpp4, &tile_data).expect("the complete 4bpp tile must decode");

    let mut colors = [Bgr555::default(); Palette::LEN];
    set_sprite_color(&mut colors, FRONT_SPRITE, FRONT_SPRITE_COLOR);
    set_sprite_color(&mut colors, MIDDLE_SPRITE, MIDDLE_SPRITE_COLOR);
    set_sprite_color(&mut colors, BACK_SPRITE, BACK_SPRITE_COLOR);
    let sprite_palette = Palette::new(colors);

    let sprites = vec![
        sprite_entry(FRONT_SPRITE),
        sprite_entry(MIDDLE_SPRITE),
        sprite_entry(BACK_SPRITE),
    ];

    (sprite_tileset, sprite_palette, sprites)
}

fn set_sprite_color(
    colors: &mut [Bgr555; Palette::LEN],
    placement: SpritePlacement,
    color: Bgr555,
) {
    let bank_start = usize::from(placement.palette_bank) * Palette::BANK_LEN;
    colors[bank_start + usize::from(SOLID_SPRITE_PALETTE_INDEX)] = color;
}

const fn sprite_entry(placement: SpritePlacement) -> OamEntry {
    OamEntry::new(
        placement.x,
        placement.y,
        SOLID_SPRITE_TILE_INDEX,
        placement.palette_bank,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        SQUARE_8X8_OBJ_SIZE,
        placement.priority,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        BootScene, BACK_SPRITE, FRONT_SPRITE, FRONT_SPRITE_COLOR, LIGHT_BACKGROUND_COLOR,
        MIDDLE_SPRITE, MIDDLE_SPRITE_COLOR,
    };
    use rendering::{BitDepth, Framebuffer};

    #[test]
    fn compose_produces_a_full_native_resolution_framebuffer() {
        let framebuffer = BootScene::new().compose();
        assert_eq!(framebuffer.width(), Framebuffer::WIDTH);
        assert_eq!(framebuffer.height(), Framebuffer::HEIGHT);
    }

    #[test]
    fn composing_is_deterministic() {
        let scene = BootScene::new();
        let first = scene.compose();
        let second = scene.compose();
        assert_eq!(first.pixels(), second.pixels());
    }

    #[test]
    fn background_checkerboard_alternates_between_two_colors() {
        let framebuffer = BootScene::new().compose();
        let first_tile = framebuffer.pixel(0, 0);
        let second_tile = framebuffer.pixel(BitDepth::TILE_DIM, 0);
        let third_tile = framebuffer.pixel(BitDepth::TILE_DIM * 2, 0);

        assert_ne!(first_tile, second_tile);
        assert_eq!(first_tile, Some(LIGHT_BACKGROUND_COLOR.to_rgb888()));
        assert_eq!(third_tile, first_tile);
    }

    #[test]
    fn front_sprite_uses_its_palette_bank() {
        let framebuffer = BootScene::new().compose();
        assert_eq!(
            framebuffer.pixel(usize::from(FRONT_SPRITE.x), usize::from(FRONT_SPRITE.y)),
            Some(FRONT_SPRITE_COLOR.to_rgb888())
        );
    }

    #[test]
    fn front_sprite_wins_the_overlap_with_the_middle_sprite() {
        let framebuffer = BootScene::new().compose();
        assert_eq!(
            framebuffer.pixel(usize::from(MIDDLE_SPRITE.x), usize::from(MIDDLE_SPRITE.y)),
            Some(FRONT_SPRITE_COLOR.to_rgb888())
        );
    }

    #[test]
    fn middle_sprite_wins_the_overlap_with_the_back_sprite() {
        let framebuffer = BootScene::new().compose();
        let middle_bottom_right = (
            usize::from(MIDDLE_SPRITE.x) + BitDepth::TILE_DIM - 1,
            usize::from(MIDDLE_SPRITE.y) + BitDepth::TILE_DIM - 1,
        );

        assert!(middle_bottom_right.0 >= usize::from(BACK_SPRITE.x));
        assert!(middle_bottom_right.1 >= usize::from(BACK_SPRITE.y));
        assert_eq!(
            framebuffer.pixel(middle_bottom_right.0, middle_bottom_right.1),
            Some(MIDDLE_SPRITE_COLOR.to_rgb888())
        );
    }

    #[test]
    fn sprites_sit_in_front_of_the_background() {
        let framebuffer = BootScene::new().compose();
        let front_sprite_pixel =
            framebuffer.pixel(usize::from(FRONT_SPRITE.x), usize::from(FRONT_SPRITE.y));
        let matching_background_pixel = framebuffer.pixel(BitDepth::TILE_DIM, 0);

        assert_ne!(front_sprite_pixel, matching_background_pixel);
    }
}
