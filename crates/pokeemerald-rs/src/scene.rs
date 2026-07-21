//! The boot shell's placeholder scene (I-1 slice 1): a deterministic tiled
//! background plus a handful of overlapping sprites, composed through the
//! real `rendering` pipeline ([`compose_frame`]) so platform -> render ->
//! present is proven end-to-end before any real game content exists.
//!
//! Every asset here is synthesized in code, not loaded from a ROM (asset
//! loading is `assets`' job, out of scope for this slice)
//! `(behavioral-fidelity)`. [`BootScene::compose`] hands back a plain
//! [`Framebuffer`], with no dependency on `platform` at all, so this module
//! is fully headless-unit-testable; converting that `Framebuffer` into a
//! `platform`-ready frame is [`crate::frame`]'s job.

use rendering::{
    compose_frame, BgLayer, BgSlot, Bgr555, BitDepth, Framebuffer, OamEntry, ObjShape, Palette,
    ScreenEntry, SpriteLayer, Tilemap, Tileset,
};

/// Background tilemap width, in 8x8 tiles -- exactly fills the framebuffer's
/// 240px width (`BG_WIDTH_TILES * 8 == 240`).
const BG_WIDTH_TILES: usize = 240 / 8;
/// Background tilemap height, in 8x8 tiles -- exactly fills the
/// framebuffer's 160px height (`BG_HEIGHT_TILES * 8 == 160`).
const BG_HEIGHT_TILES: usize = 160 / 8;

/// The background's BG priority: worse (higher-numbered) than every test
/// sprite's (`0..=2`, see [`build_sprites`]), so the sprites always draw in
/// front of it.
const BG_PRIORITY: u8 = 3;

/// Owns every rendering asset for the placeholder boot scene and knows how
/// to fold them into one [`Framebuffer`] via [`compose_frame`].
///
/// An owned type, not a free function, so the (cheap but non-trivial)
/// tile/palette/sprite setup happens once in [`BootScene::new`] rather than
/// being redone on every [`BootScene::compose`] call `(oop-boundaries)`.
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
    /// Build the deterministic placeholder scene: a two-tile checkerboard
    /// background (exercising [`BgLayer`]) plus three same-size,
    /// differently-colored, differently-prioritized sprites arranged in a
    /// diagonal, overlapping cascade (exercising [`compose_frame`]'s
    /// priority ordering).
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

    /// Composite the scene into a fresh [`Framebuffer`] through the real
    /// `rendering` priority compositor: the background at [`BG_PRIORITY`]
    /// (worse than every sprite), so the sprites always draw over it, and
    /// among themselves in ascending priority order (see
    /// [`build_sprites`]).
    #[must_use]
    pub fn compose(&self) -> Framebuffer {
        let bg_layer = BgLayer::new(&self.bg_tileset, &self.bg_palette, &self.bg_tilemap);
        let bg_slot = BgSlot::new(bg_layer, 0, BG_PRIORITY, 0, 0, true);
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

/// Build the two-tile checkerboard background: tile 0 (palette index 1)
/// alternating with tile 1 (palette index 2), tiling the whole 240x160
/// framebuffer.
fn build_background() -> (Tileset, Palette, Tilemap) {
    let mut tile_data = [0u8; 64]; // two 4bpp tiles, 32 bytes each.
    tile_data[..32].fill(0x11); // tile 0: every pixel -> palette index 1.
    tile_data[32..].fill(0x22); // tile 1: every pixel -> palette index 2.
    let bg_tileset = Tileset::decode(BitDepth::Bpp4, &tile_data)
        .expect("hand-built 64-byte 4bpp tile data always decodes");

    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(6, 10, 18); // muted steel blue.
    colors[2] = Bgr555::from_channels(4, 7, 12); // darker blue-gray.
    let bg_palette = Palette::new(colors);

    let mut entries = Vec::with_capacity(BG_WIDTH_TILES * BG_HEIGHT_TILES);
    for row in 0..BG_HEIGHT_TILES {
        for col in 0..BG_WIDTH_TILES {
            let tile_index = u16::from((col + row) % 2 == 1);
            entries.push(ScreenEntry::new(tile_index, false, false, 0));
        }
    }
    let bg_tilemap = Tilemap::new(BG_WIDTH_TILES, BG_HEIGHT_TILES, entries)
        .expect("entries.len() == BG_WIDTH_TILES * BG_HEIGHT_TILES by construction");

    (bg_tileset, bg_palette, bg_tilemap)
}

/// Build three 8x8 opaque sprites in a diagonal, overlapping cascade: OAM
/// index 0 (priority 0, red) draws in front of index 1 (priority 1, green),
/// which draws in front of index 2 (priority 2, blue) -- and all three draw
/// in front of the background ([`BG_PRIORITY`], worse than any of these).
/// Draw order in [`compose_frame`] depends only on each entry's `priority`
/// and OAM index, never on this `Vec`'s order.
fn build_sprites() -> (Tileset, Palette, Vec<OamEntry>) {
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32])
        .expect("hand-built 32-byte 4bpp tile data always decodes");

    // Channel values chosen so the dominant channel hits full brightness
    // (5-bit 31 -> 8-bit 255) and the others land on the same clean value
    // (5-bit 4 -> 8-bit 33), giving unit tests exact, easy-to-eyeball
    // expected colors.
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[15] = Bgr555::from_channels(31, 4, 4); // bank 0, index 15: red.
    colors[Palette::BANK_LEN + 15] = Bgr555::from_channels(4, 31, 4); // bank 1: green.
    colors[2 * Palette::BANK_LEN + 15] = Bgr555::from_channels(4, 4, 31); // bank 2: blue.
    let sprite_palette = Palette::new(colors);

    let sprites = vec![
        sprite_entry(100, 76, 0, 0), // red, frontmost.
        sprite_entry(104, 80, 1, 1), // green, overlaps red's bottom-right corner.
        sprite_entry(108, 84, 2, 2), // blue, overlaps green's bottom-right corner.
    ];

    (sprite_tileset, sprite_palette, sprites)
}

/// One 8x8, single-tile, always-enabled sprite entry at `(x, y)`, drawing
/// through palette bank `palette_bank` at `priority`.
const fn sprite_entry(x: u16, y: u8, palette_bank: u8, priority: u8) -> OamEntry {
    OamEntry::new(
        x,
        y,
        0, // tile 0: the single opaque tile built in `build_sprites`.
        palette_bank,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0, // size 0 -> 8x8, matching the single-tile sprite tileset.
        priority,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::BootScene;
    use rendering::Rgb888;

    #[test]
    fn compose_produces_a_full_240x160_framebuffer() {
        let scene = BootScene::new();
        let fb = scene.compose();
        assert_eq!(fb.width(), 240);
        assert_eq!(fb.height(), 160);
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
        let scene = BootScene::new();
        let fb = scene.compose();
        // Far from every sprite (see `build_sprites`'s 100..116 x 76..92
        // footprint), so these pixels are pure background.
        let a = fb.pixel(0, 0).unwrap();
        let b = fb.pixel(8, 0).unwrap();
        assert_ne!(a, b, "adjacent background tiles must differ");
        assert_eq!(
            fb.pixel(16, 0).unwrap(),
            a,
            "the checkerboard repeats every 2 tiles"
        );
    }

    #[test]
    fn frontmost_sprite_wins_the_overlap() {
        let scene = BootScene::new();
        let fb = scene.compose();
        // (100, 76) is covered only by the frontmost (priority 0) sprite:
        // red, per `build_sprites`.
        assert_eq!(
            fb.pixel(100, 76),
            Some(Rgb888 {
                r: 255,
                g: 33,
                b: 33
            })
        );
    }

    #[test]
    fn middle_sprite_wins_where_the_frontmost_sprite_does_not_cover() {
        let scene = BootScene::new();
        let fb = scene.compose();
        // (104, 80) is covered by both the front (priority 0, x:100..108,
        // y:76..84) and middle (priority 1, x:104..112, y:80..88) sprites;
        // the lower-priority-number sprite (front, red) must win.
        assert_eq!(
            fb.pixel(104, 80),
            Some(Rgb888 {
                r: 255,
                g: 33,
                b: 33
            })
        );
        // (111, 87) is covered by the middle and back sprites but not the
        // front one (front's footprint ends at x=107, y=83); middle (green)
        // must win over back (blue).
        assert_eq!(
            fb.pixel(111, 87),
            Some(Rgb888 {
                r: 33,
                g: 255,
                b: 33
            })
        );
    }

    #[test]
    fn sprites_sit_in_front_of_the_background() {
        let scene = BootScene::new();
        let fb = scene.compose();
        // (100, 76): background tile parity is (100/8 + 76/8) % 2 == 1, so
        // without the sprite this pixel would be the second (darker) BG
        // color -- the same one visible at (8, 0), an unoccupied pixel with
        // matching parity. The frontmost sprite must occlude it instead.
        assert_ne!(fb.pixel(100, 76), fb.pixel(8, 0));
    }
}
