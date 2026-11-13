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

/// A 16x16 OBJ of OBJ `priority` at OAM `y` whose palette bank tags it.
fn prioritised_at(y: u8, bank: u8, priority: u8) -> SheetSprite {
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
            priority,
            true,
        ),
        0,
    )
}

/// A sprite named by object `n`, tagged with bank `n`, at subpriority 100
/// and OAM `y` (OBJ priority 2).
fn object(n: u8, y: u8) -> DepthSprite {
    DepthSprite::with_subpriority(tagged_at(y, n), 100).with_id(SpriteId::Object(u32::from(n)))
}

fn banks(order: &mut SpriteOrder, sprites: &[DepthSprite]) -> Vec<u8> {
    order
        .order(sprites)
        .iter()
        .map(|sprite| sprite.palette_bank())
        .collect()
}

/// `SortSprites` swaps only on a strictly greater key or a strictly smaller
/// `y` (`sprite.c:413-415`): sprites tied in everything keep last frame's order
/// whatever order they are listed in.
#[test]
fn sprite_order_keeps_the_previous_order_on_an_exact_tie() {
    let mut order = SpriteOrder::default();
    // Object 1 is lower (larger y) and leads.
    assert_eq!(banks(&mut order, &[object(0, 40), object(1, 70)]), [1, 0]);
    // The tie: listed player-first, but object 1 was ahead.
    assert_eq!(banks(&mut order, &[object(0, 40), object(1, 40)]), [1, 0]);
    // Still tied, listed the other way round.
    assert_eq!(banks(&mut order, &[object(1, 40), object(0, 40)]), [1, 0]);
    // A strictly lower object 0 takes the lead and keeps it through a tie.
    assert_eq!(banks(&mut order, &[object(0, 90), object(1, 40)]), [0, 1]);
    assert_eq!(banks(&mut order, &[object(0, 40), object(1, 40)]), [0, 1]);
}

/// A first frame has no history: ties keep the listing order, as the identity
/// `sSpriteOrder` does after `ResetAllSprites` (`sprite.c:848-858`).
#[test]
fn sprite_order_breaks_a_first_frame_tie_in_listing_order() {
    let mut order = SpriteOrder::default();
    assert_eq!(banks(&mut order, &[object(0, 40), object(1, 40)]), [0, 1]);
    let mut order = SpriteOrder::default();
    assert_eq!(banks(&mut order, &[object(1, 40), object(0, 40)]), [1, 0]);
}

/// A sprite that disappears leaves the others' relative order alone, and one
/// that returns is new: its slot sorts after every drawn sprite (an unused
/// sprite has OBJ priority 3, `sprite.c:101-115`), so it loses an exact tie.
#[test]
fn sprite_order_treats_a_returning_sprite_as_new_and_leaves_the_rest_alone() {
    let mut order = SpriteOrder::default();
    assert_eq!(
        banks(&mut order, &[object(0, 70), object(1, 40), object(2, 10)]),
        [0, 1, 2]
    );
    // Object 0 goes away; 1 and 2 keep their order through a tie.
    assert_eq!(banks(&mut order, &[object(1, 40), object(2, 40)]), [1, 2]);
    assert_eq!(banks(&mut order, &[object(2, 40), object(1, 40)]), [1, 2]);
    // Object 0 returns tied: it is new, so it sorts behind both.
    assert_eq!(
        banks(&mut order, &[object(0, 40), object(1, 40), object(2, 40)]),
        [1, 2, 0]
    );
}

/// A new sprite loses an exact tie to an existing one at every OBJ priority,
/// including 0, whatever the listing order.
#[test]
fn sprite_order_puts_a_new_sprite_behind_a_tied_one_at_any_priority() {
    for priority in 0..=2 {
        let sprite = |n: u8| {
            DepthSprite::with_subpriority(prioritised_at(40, n, priority), 100)
                .with_id(SpriteId::Object(u32::from(n)))
        };
        let mut order = SpriteOrder::default();
        assert_eq!(banks(&mut order, &[sprite(0)]), [0]);
        assert_eq!(banks(&mut order, &[sprite(1), sprite(0)]), [0, 1]);
    }
}

/// A sprite with no id has no history: it is new on every frame, so it loses
/// an exact tie to a named one.
#[test]
fn sprite_order_gives_an_unnamed_sprite_no_history() {
    let anonymous = |n: u8, y: u8| DepthSprite::with_subpriority(tagged_at(y, n), 100);
    let mut order = SpriteOrder::default();
    assert_eq!(banks(&mut order, &[anonymous(0, 40), object(1, 70)]), [1, 0]);
    assert_eq!(banks(&mut order, &[anonymous(0, 40), object(1, 40)]), [1, 0]);
}

/// A crossing translates a tall-grass effect's identity with its tile, so the
/// effect keeps its place, and leaves objects and the player named as they were.
#[test]
fn sprite_order_shifted_follows_an_effect_across_a_crossing() {
    let effect = |tile: (i32, i32), bank: u8, y: u8| {
        DepthSprite::with_subpriority(tagged_at(y, bank), 100).with_id(SpriteId::Effect {
            owner: None,
            tile,
            nth: 0,
        })
    };
    let mut order = SpriteOrder::default();
    assert_eq!(
        banks(&mut order, &[effect((3, 4), 0, 70), object(1, 40)]),
        [0, 1]
    );
    let mut order = order.shifted((-2, 5));
    // Both tied now: the effect, moved to its shifted tile, still leads.
    assert_eq!(
        banks(&mut order, &[object(1, 40), effect((1, 9), 0, 40)]),
        [0, 1]
    );
}
