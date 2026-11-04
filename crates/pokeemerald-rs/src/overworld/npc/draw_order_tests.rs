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

/// A 16x16 OBJ at OAM `y` whose palette bank tags it.
fn tagged_at(y: u8, bank: u8) -> SheetSprite {
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
            1,
            2,
            true,
        ),
        0,
    )
}

fn banks_in_order(sprites: &[DepthSprite]) -> Vec<u8> {
    order_sprites(sprites)
        .iter()
        .map(|sprite| sprite.palette_bank())
        .collect()
}

/// Upstream `SortSprites` maps `oam.y >= 160` to `y - 256` before the tie
/// (`sprite.c:385-389`), so a sprite at top -8 (OAM 248) sorts behind one
/// at top +8 (`:413-415`).
#[test]
fn order_sprites_breaks_tie_by_signed_screen_y_across_top_boundary() {
    let above = DepthSprite::with_subpriority(tagged_at(248, 0), 100);
    let below = DepthSprite::with_subpriority(tagged_at(8, 1), 100);
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
