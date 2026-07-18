//! The cross-layer priority compositor: up to four regular BG layers plus
//! the sprite layer, combined into one frame (S-2 slice 2).
//!
//! Ports the BG/OBJ ordering rules verified against
//! `mgba/src/gba/renderers/video-software.c` and `software-obj.c`:
//!
//! - A **lower [`priority`](BgSlot::new) number composites in front**,
//!   regardless of layer kind (sprite or BG).
//! - At equal priority, a **sprite always beats a BG** — in `video-software.c`
//!   a BG's per-pixel order key is `(priority << OFFSET_PRIORITY) |
//!   (index << OFFSET_INDEX) | FLAG_IS_BACKGROUND`, strictly greater (so
//!   strictly *behind*, in the "smaller key wins" comparison the renderer
//!   uses) than a sprite's bare `priority << OFFSET_PRIORITY` key at the same
//!   priority.
//! - At equal priority among BGs, the **lower BG index wins** — the index
//!   term above breaks the tie in ascending order (BG0 in front of BG1, and
//!   so on).
//! - At equal priority among sprites, the **lower OAM index wins** — see
//!   [`SpriteLayer::resolve_pixel`](crate::sprite::SpriteLayer::resolve_pixel).
//!
//! Windows, blending, mosaic, and affine layers are out of scope (issue
//! #64) `(behavioral-fidelity)`.

use crate::bg::BgLayer;
use crate::framebuffer::Framebuffer;
use crate::sprite::SpriteLayer;

/// One of up to four regular BG layers participating in priority
/// composition, paired with the register-level state the GBA PPU consults
/// alongside the tile/palette data itself: which of BG0..BG3 this is
/// (breaks same-priority ties), the layer's priority, its current scroll
/// position, and whether it's enabled at all.
#[derive(Debug, Clone, Copy)]
pub struct BgSlot<'a> {
    layer: BgLayer<'a>,
    bg_index: u8,
    priority: u8,
    scroll_x: u16,
    scroll_y: u16,
    enabled: bool,
}

impl<'a> BgSlot<'a> {
    /// Build a BG slot. `bg_index` (`0..=3`, identifying BG0..BG3) and
    /// `priority` (`0..=3`) are masked to 2 bits, so this never panics.
    #[must_use]
    pub const fn new(
        layer: BgLayer<'a>,
        bg_index: u8,
        priority: u8,
        scroll_x: u16,
        scroll_y: u16,
        enabled: bool,
    ) -> Self {
        Self {
            layer,
            bg_index: bg_index & 0x03,
            priority: priority & 0x03,
            scroll_x,
            scroll_y,
            enabled,
        }
    }
}

/// A candidate pixel's ordering key: `(priority, layer_rank)`. Lower sorts
/// in front. A sprite's `layer_rank` is always `0`, strictly less than any
/// BG's `1 + bg_index` — so a sprite wins any same-priority tie against a
/// BG, and BGs break same-priority ties by ascending `bg_index`, matching
/// the ordering rules in the module docs.
type OrderKey = (u8, u8);

/// Composite up to four [`BgSlot`]s and one [`SpriteLayer`] into a new
/// [`Framebuffer`], applying the GBA's priority ordering rules (module
/// docs).
///
/// `bg_slots` need not have exactly four entries (a scene may use fewer BG
/// layers); disabled slots contribute nothing. Pixels covered by no enabled,
/// opaque layer are left at the framebuffer's default backdrop
/// ([`Rgb888::BLACK`](crate::palette::Rgb888::BLACK)) — a real backdrop
/// color register is out of scope for this slice.
#[must_use]
pub fn compose_frame(sprites: &SpriteLayer<'_>, bg_slots: &[BgSlot<'_>]) -> Framebuffer {
    let mut framebuffer = Framebuffer::new();
    for y in 0..framebuffer.height() {
        for x in 0..framebuffer.width() {
            let mut winner: Option<(OrderKey, _)> = sprites
                .resolve_pixel(x, y)
                .map(|pixel| ((pixel.priority, 0u8), pixel.color));

            for slot in bg_slots {
                if !slot.enabled {
                    continue;
                }
                let Some(color) = slot
                    .layer
                    .sample_scrolled(x, y, slot.scroll_x, slot.scroll_y)
                else {
                    continue;
                };
                let key = (slot.priority, 1 + slot.bg_index);
                winner = match winner {
                    Some((w_key, w_color)) if w_key <= key => Some((w_key, w_color)),
                    _ => Some((key, color)),
                };
            }

            if let Some((_, color)) = winner {
                framebuffer.set_pixel(x, y, color);
            }
        }
    }
    framebuffer
}

#[cfg(test)]
mod tests {
    use super::{compose_frame, BgSlot};
    use crate::oam::{OamEntry, ObjShape};
    use crate::palette::{Bgr555, Palette};
    use crate::sprite::SpriteLayer;
    use crate::tile::{BitDepth, Tileset};
    use crate::tilemap::{ScreenEntry, Tilemap};

    /// A fully opaque 1x1-tile (8x8px) BG layer using palette index 15 in
    /// bank 0, plus its owning tileset/palette/tilemap (kept alive by the
    /// caller for the lifetime of the returned [`crate::bg::BgLayer`]).
    fn opaque_bg_fixture(color_channel: u8) -> (Tileset, Palette, Tilemap) {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(color_channel, 0, 0);
        let palette = Palette::new(colors);
        let entries = vec![ScreenEntry::new(0, false, false, 0)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();
        (tileset, palette, tilemap)
    }

    fn empty_sprite_layer<'a>(entries: &'a [OamEntry], tileset: &'a Tileset) -> SpriteLayer<'a> {
        SpriteLayer::new(entries, tileset, tileset, &EMPTY_PALETTE)
    }

    // A palette of all-default (black, and every index resolves to the same
    // color) used only where sprite entries are empty and never sampled.
    static EMPTY_PALETTE: Palette = Palette::new([Bgr555::from_raw(0); Palette::LEN]);

    #[test]
    fn bg_vs_bg_lower_priority_number_wins() {
        let (tiles_x, palette_x, map_x) = opaque_bg_fixture(1);
        let (tiles_y, palette_y, map_y) = opaque_bg_fixture(2);
        let layer_a = crate::bg::BgLayer::new(&tiles_x, &palette_x, &map_x);
        let layer_b = crate::bg::BgLayer::new(&tiles_y, &palette_y, &map_y);

        // BG1 (worse priority 3) vs BG0 (better priority 0, but declared
        // second) -- priority must win over bg_index/declaration order.
        let slots = [
            BgSlot::new(layer_a, 1, 3, 0, 0, true),
            BgSlot::new(layer_b, 0, 0, 0, 0, true),
        ];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(2, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn bg_vs_bg_same_priority_lower_bg_index_wins() {
        let (tiles_x, palette_x, map_x) = opaque_bg_fixture(1); // will be bg_index 2
        let (tiles_y, palette_y, map_y) = opaque_bg_fixture(2); // will be bg_index 0
        let layer_a = crate::bg::BgLayer::new(&tiles_x, &palette_x, &map_x);
        let layer_b = crate::bg::BgLayer::new(&tiles_y, &palette_y, &map_y);

        // Same priority (1) for both; bg_index 0 must win over bg_index 2
        // despite being declared second in the slice.
        let slots = [
            BgSlot::new(layer_a, 2, 1, 0, 0, true),
            BgSlot::new(layer_b, 0, 1, 0, 0, true),
        ];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(2, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn disabled_bg_slot_contributes_nothing() {
        let (ts, pal, tm) = opaque_bg_fixture(9);
        let layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        let slots = [BgSlot::new(layer, 0, 0, 0, 0, false)];
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(fb.pixel(0, 0), Some(crate::palette::Rgb888::BLACK));
    }

    #[test]
    fn sprite_vs_bg_same_priority_sprite_wins() {
        let (ts, pal, tm) = opaque_bg_fixture(1);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        // BG at priority 2, best (lowest) bg_index (0) — still must lose to
        // a same-priority sprite.
        let slots = [BgSlot::new(bg_layer, 0, 2, 0, 0, true)];

        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
        let sprite_palette = Palette::new(sprite_colors);
        let entries = [OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            2, // same priority as the BG
            true,
        )];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 9, 0).to_rgb888())
        );
    }

    #[test]
    fn sprite_lower_priority_number_beats_a_better_indexed_bg() {
        let (ts, pal, tm) = opaque_bg_fixture(1);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        // BG at the best possible priority (0).
        let slots = [BgSlot::new(bg_layer, 0, 0, 0, 0, true)];

        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut sprite_colors = [Bgr555::default(); Palette::LEN];
        sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
        let sprite_palette = Palette::new(sprite_colors);
        // Sprite at a WORSE priority (3) than the BG (0) -- the BG must win.
        let entries = [OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            3,
            true,
        )];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(1, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn transparent_sprite_pixel_lets_the_bg_show_through() {
        let (ts, pal, tm) = opaque_bg_fixture(4);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        let slots = [BgSlot::new(bg_layer, 0, 3, 0, 0, true)];

        // A fully transparent (all index-0) sprite at the best priority --
        // it must not occlude the BG at all.
        let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0x00u8; 32]).unwrap();
        let sprite_palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let entries = [OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )];
        let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(4, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn better_sprites_transparent_hole_promotes_a_worse_sprite_over_the_bg() {
        // Finding 1: opaque sprite B (priority 2) sits under sprite A
        // (priority 0), whose texel at this pixel is a transparent hole; a BG
        // sits between them at priority 1. On hardware A's hole upgrades B's
        // stored OBJ order to priority 0, so the OBJ layer (still showing B's
        // color) beats the BG — even though B's own priority (2) is worse
        // than the BG's (1). Pre-fix the OBJ pixel carried priority 2 and the
        // BG wrongly won.
        let (ts, pal, tm) = opaque_bg_fixture(7);
        let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
        let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)]; // BG priority 1

        // A single shared 4bpp tileset: tile 0 fully opaque (B draws it),
        // tile 1 fully transparent (A draws it). B is OAM index 0 so it
        // writes first; A (index 1) then upgrades the order via its hole.
        let mut two_tiles = [0u8; 64];
        two_tiles[..32].copy_from_slice(&[0xFFu8; 32]); // tile 0 -> index 15 everywhere
        let shared = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0, 0, 9); // B's color (blue)
        let palette = Palette::new(colors);

        let b_opaque_prio2 = OamEntry::new(
            0,
            0,
            0, // tile 0 (opaque)
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            2,
            true,
        );
        let a_transparent_prio0 = OamEntry::new(
            0,
            0,
            1, // tile 1 (transparent)
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        );
        let entries = [b_opaque_prio2, a_transparent_prio0];
        let sprites = SpriteLayer::new(&entries, &shared, &shared, &palette);

        let fb = compose_frame(&sprites, &slots);
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0, 9).to_rgb888()),
            "B's color must beat the BG because A's hole upgrades it to priority 0"
        );
    }

    #[test]
    fn pixel_with_no_opaque_layer_stays_at_the_backdrop() {
        let entries: [OamEntry; 0] = [];
        let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);
        let fb = compose_frame(&sprites, &[]);
        assert_eq!(fb.pixel(0, 0), Some(crate::palette::Rgb888::BLACK));
    }
}
