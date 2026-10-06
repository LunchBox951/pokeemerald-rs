use super::*;

/// Upstream `SortSprites` (`sprite.c:413-415`) breaks an equal
/// priority/subpriority tie by `oam.y`, the lower-on-screen sprite first.
#[test]
fn order_by_depth_breaks_a_subpriority_tie_by_lower_screen_y() {
    let at = |y: u8, bank: u8| {
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
        )
    };
    let player = at(64, 0);
    let npc = at(70, 1);
    assert_eq!(subpriority(player, 3), subpriority(npc, 3), "tie premise");
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
    let ordered = order_by_depth(&[(player, 3), (npc, 4)]);
    let pixel = rendering::SpriteLayer::new(&ordered, &tiles, &tiles, &palette)
        .resolve_pixel(122, 66)
        .expect("player texel is opaque");
    assert_eq!(pixel.priority, 2, "player keeps its own OBJ priority");
}
