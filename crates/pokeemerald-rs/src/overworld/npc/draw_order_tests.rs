use super::*;

/// Upstream `SortSprites` (`sprite.c:413-415`) breaks an equal
/// priority/subpriority tie by `oam.y`, the lower-on-screen sprite first.
#[test]
fn order_by_depth_breaks_a_subpriority_tie_by_lower_screen_y() {
    let at = |y: u8, bank: u8| {
        SheetSprite::new(
            OamEntry::new(
                120,
                y,
                0,
                bank,
                rendering::BitDepth::Bpp4,
                false,
                false,
                rendering::ObjShape::Vertical,
                2,
                2,
                true,
            ),
            0,
        )
    };
    let player = at(64, 0);
    let npc = at(70, 1);
    assert_eq!(
        subpriority(player.oam(), 3),
        subpriority(npc.oam(), 3),
        "tie premise"
    );
    let ordered = order_by_depth(&[(player, 3), (npc, 3)]);
    assert_eq!(
        ordered[0].palette_bank(),
        1,
        "lower NPC must take OAM slot 0"
    );
}

/// Hardware retags a written pixel from a later transparent texel (`software-obj.c:120-125`).
#[test]
fn order_by_depth_keeps_transparent_higher_priority_entry_from_raising_player() {
    let at = |tile: u16, priority: u8| {
        SheetSprite::new(
            OamEntry::new(
                120,
                64,
                tile,
                0,
                rendering::BitDepth::Bpp4,
                false,
                false,
                rendering::ObjShape::Square,
                0,
                priority,
                true,
            ),
            tile,
        )
    };
    // Tile 0 is opaque (index 1); tile 1 is fully transparent.
    let mut bytes = vec![0x11_u8; 32];
    bytes.extend([0_u8; 32]);
    let tiles = rendering::Tileset::decode(rendering::BitDepth::Bpp4, &bytes).expect("tiles");
    let mut colors = [rendering::Bgr555::from_raw(0); rendering::Palette::LEN];
    colors[1] = rendering::Bgr555::from_raw(0x7FFF);
    let palette = rendering::Palette::new(colors);
    let player = at(0, 2);
    let npc = at(1, 1);
    let ordered: Vec<rendering::OamEntry> = order_by_depth(&[(player, 3), (npc, 4)])
        .into_iter()
        .map(SheetSprite::oam)
        .collect();
    let pixel = rendering::SpriteLayer::new(&ordered, &tiles, &tiles, &palette)
        .resolve_pixel(122, 66)
        .expect("player texel is opaque");
    assert_eq!(pixel.priority, 2, "player keeps its own OBJ priority");
}

/// A square OBJ of OAM `size` (1: 16x16, 2: 32x32) at OAM `y` whose
/// palette bank tags it.
fn square_at(y: u8, size: u8, bank: u8) -> SheetSprite {
    SheetSprite::new(
        OamEntry::new(
            120,
            y,
            0,
            bank,
            rendering::BitDepth::Bpp4,
            false,
            false,
            rendering::ObjShape::Square,
            size,
            2,
            true,
        ),
        0,
    )
}

/// A 16x16 OBJ at OAM `y` whose palette bank tags it.
fn tagged_at(y: u8, bank: u8) -> SheetSprite {
    square_at(y, 1, bank)
}

fn banks_in_order(sprites: &[DepthSprite]) -> Vec<u8> {
    order_sprites(sprites)
        .iter()
        .map(|sprite| sprite.palette_bank())
        .collect()
}

/// Upstream `SortSprites` maps `oam.y >= 160` to `y - 256` before the tie
/// (`sprite.c:385-389`), so a 32x32 sprite at top -8 (OAM 248) sorts behind
/// a 16x16 one at top +8 (`:413-415`). Both bottoms sit on row 24, so
/// `SetObjectSubpriorityByElevation` ties them (`event_object_movement.c:7773-7787`),
/// and they overlap on rows 8-23, where the earlier OAM slot is drawn.
#[test]
fn order_sprites_breaks_tie_by_signed_screen_y_across_top_boundary() {
    let elevation = 3;
    let tall = square_at(248, 2, 0);
    let short = tagged_at(8, 1);
    let tall_subpriority = subpriority_from_bottom(i32::from(tall.y()) + 32, elevation, 1);
    let short_subpriority = subpriority_from_bottom(i32::from(short.y()) + 16, elevation, 1);
    assert_eq!(tall_subpriority, short_subpriority, "tie premise");
    let above = DepthSprite::with_subpriority(tall, tall_subpriority);
    let below = DepthSprite::with_subpriority(short, short_subpriority);
    let ordered = order_sprites(&[above.clone(), below.clone()]);
    assert_eq!(
        ordered[0].palette_bank(),
        1,
        "the sprite at screen top +8 must take the earlier OAM slot"
    );
    assert_eq!(ordered[0].oam().y(), 8, "the emitted Y stays wrapped");
    assert_eq!(ordered[1].oam().y(), 248, "the emitted Y stays wrapped");
    assert_eq!(
        banks_in_order(&[below, above]),
        [1, 0],
        "input order does not matter once the Y keys differ"
    );

    // Every tile is opaque colour index 1; each bank paints it differently.
    let tiles =
        rendering::Tileset::decode(rendering::BitDepth::Bpp4, &[0x11_u8; 32 * 16]).expect("tiles");
    let mut colors = [rendering::Bgr555::from_raw(0); rendering::Palette::LEN];
    colors[1] = rendering::Bgr555::from_raw(0x001F);
    colors[rendering::Palette::BANK_LEN + 1] = rendering::Bgr555::from_raw(0x7C00);
    let palette = rendering::Palette::new(colors);
    let entries: Vec<rendering::OamEntry> = ordered.into_iter().map(SheetSprite::oam).collect();
    let layer = rendering::SpriteLayer::new(&entries, &tiles, &tiles, &palette);
    let short_color = palette.bank_color(1, 1).to_rgb888();
    let tall_color = palette.bank_color(0, 1).to_rgb888();
    assert_ne!(short_color, tall_color, "banks are distinguishable");
    let overlap = layer
        .resolve_pixel(124, 12)
        .expect("both texels are opaque");
    assert_eq!(
        overlap.color, short_color,
        "the top +8 sprite wins the overlap"
    );
    let tall_only = layer.resolve_pixel(124, 2).expect("tall texel is opaque");
    assert_eq!(
        tall_only.color, tall_color,
        "the wrapped sprite covers row 2"
    );
}

/// The wrap point is `DISPLAY_HEIGHT` (`sprite.c:385`): OAM 160 is -96, so
/// it sorts behind OAM 159 even though the raw byte is larger.
#[test]
fn order_sprites_breaks_tie_by_signed_screen_y_across_bottom_boundary() {
    let last_row = DepthSprite::with_subpriority(tagged_at(159, 0), 100);
    let wrapped = DepthSprite::with_subpriority(tagged_at(160, 1), 100);
    assert_eq!(banks_in_order(&[wrapped.clone(), last_row.clone()]), [0, 1]);
    assert_eq!(banks_in_order(&[last_row, wrapped]), [0, 1]);
}

/// Two wrapped Ys and two on-screen Ys keep the larger-Y-first order.
#[test]
fn order_sprites_keeps_larger_y_first_when_no_boundary_is_crossed() {
    let high = DepthSprite::with_subpriority(tagged_at(8, 0), 100);
    let low = DepthSprite::with_subpriority(tagged_at(40, 1), 100);
    assert_eq!(banks_in_order(&[high, low]), [1, 0]);
    let far = DepthSprite::with_subpriority(tagged_at(200, 2), 100);
    let near = DepthSprite::with_subpriority(tagged_at(248, 3), 100);
    assert_eq!(banks_in_order(&[far, near]), [3, 2]);
}
