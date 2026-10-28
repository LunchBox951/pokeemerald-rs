//! Frame geometry, anchoring, depth and packing of non-16x32 object graphics.

use super::sheets::{object_sheet, Facing};
use super::test_support::*;
use super::*;
use assets::MovementType;
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::Direction;

fn colocated_origin(size: (u16, u16)) -> (i32, i32) {
    object_origin((5, 5), (5, 5), (0, 0), size)
}

/// Every size's bottom edge meets the player's, centred on the tile
/// (`event_object_movement.c:1459-1464`): top-left `(16x + 8 - w/2, 16y + 16 - h)`
/// relative to the tile, with the 16x32 player OBJ at (112, 56).
#[test]
fn colocated_origins_share_the_tile_bottom_and_centre() {
    assert_eq!(colocated_origin((16, 16)), (112, 72));
    assert_eq!(colocated_origin((16, 32)), (112, 56));
    assert_eq!(colocated_origin((32, 32)), (104, 56));
    assert_eq!(colocated_origin((48, 48)), (96, 40));
    for size in [(16, 16), (16, 32), (32, 32), (48, 48)] {
        assert_eq!(colocated_origin(size).1 + i32::from(size.1), 88, "{size:?}");
    }
}

/// `SetObjectSubpriorityByElevation` with subpriority 1: the base at the
/// elevation plus twice `16 - ((bottom + 8) >> 4)`; at bottom 88 the row is 6.
#[test]
fn subpriority_follows_the_bottom_edge_and_elevation_base() {
    assert_eq!(subpriority_from_bottom(88, 3), 115 + 2 * (16 - 6) + 1);
    assert_eq!(subpriority_from_bottom(88, 2), 83 + 2 * (16 - 6) + 1);
    assert_eq!(subpriority_from_bottom(88, 4), 83 + 2 * (16 - 6) + 1);
    assert_eq!(subpriority_from_bottom(88, 3), 136);
    // The sum wraps at 256 like upstream's `& 0xFF`.
    assert_eq!(
        subpriority_from_bottom(248, 3),
        subpriority_from_bottom(248 - 256, 3)
    );
}

fn truck_binding() -> SpriteBinding {
    let sheet = object_sheet("OBJ_EVENT_GFX_TRUCK").unwrap();
    SpriteBinding {
        base_tile: 100,
        palette_bank: sheet.palette_bank,
        width: sheet.width,
        height: sheet.height,
        facing: sheet.facing,
        packed_cells: 1,
    }
}

/// `sOamTable_48x48`: per 8-pixel row a 32x8 OBJ at the row's left then a
/// 16x8 OBJ 32 pixels right, tile offsets 0, 4, 6, 10, ... in the row-major
/// frame, all priority 2 and unflipped.
#[test]
fn the_truck_is_twelve_horizontal_objs_at_priority_two() {
    let entries = truck_entries(truck_binding(), (96, 40));
    assert_eq!(entries.len(), 12);
    let offsets: Vec<u16> = entries.iter().map(|e| e.tile_index() - 100).collect();
    assert_eq!(offsets, [0, 4, 6, 10, 12, 16, 18, 22, 24, 28, 30, 34]);
    for (index, entry) in entries.iter().enumerate() {
        let wide = index % 2 == 0;
        assert_eq!(entry.dimensions(), if wide { (32, 8) } else { (16, 8) });
        assert_eq!(entry.x(), if wide { 96 } else { 128 });
        assert_eq!(
            usize::from(entry.y()),
            40 + 8 * (index / 2),
            "row {}",
            index / 2
        );
        assert_eq!(entry.priority(), 2);
        assert!(!entry.h_flip() && !entry.v_flip());
        assert_eq!(entry.palette_bank(), 8, "the truck palette bank");
    }
}

fn truck_event() -> &'static [ObjectEvent] {
    // Elevation 4 maps to OBJ priority 1; the truck's pieces still draw at 2.
    let mut event = object("OBJ_EVENT_GFX_TRUCK", 5, 5, MovementType::FaceRight);
    event.elevation = 4;
    Box::leak(Box::new([event]))
}

/// The truck sorts as one parent at its elevation priority and its twelve
/// OBJs stay contiguous and in table order, whatever sorts around them.
#[test]
fn a_truck_sorts_as_one_parent_and_never_flips_for_its_facing() {
    let events = truck_event();
    let mut bindings = HashMap::new();
    bindings.insert("OBJ_EVENT_GFX_TRUCK", truck_binding());
    let player = PlayerState::new((5, 5), 3, Direction::South);
    let data = EventData::new();
    let drawn = elevated_oam_entries(
        &ObjectEventCollection::from_templates(events),
        &[4],
        &bindings,
        &player,
        &data,
    );
    assert_eq!(drawn.len(), 1, "one parent");
    assert_eq!(drawn[0].entries.len(), 12);
    assert_eq!(drawn[0].parent_priority, 1, "elevation 4's OBJ priority");
    assert_eq!(drawn[0].subpriority, subpriority_from_bottom(88, 4));
    assert!(
        drawn[0].entries.iter().all(|e| !e.h_flip()),
        "an inanimate object ignores its authored facing"
    );

    let mut sprites = vec![DepthSprite::single(
        OamEntry::new(
            112,
            56,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            rendering::ObjShape::Vertical,
            2,
            2,
            true,
        ),
        3,
    )];
    sprites.extend(drawn);
    let ordered = order_sprites(&sprites);
    assert_eq!(ordered.len(), 13);
    assert_eq!(
        ordered[0].dimensions(),
        (32, 8),
        "priority 1 sorts before the player's priority 2"
    );
    assert_eq!(ordered[12].dimensions(), (16, 32), "the player is last");
}

/// A single-OBJ 16x16 object is anchored on its tile and takes its OBJ
/// priority from the elevation it settles at, not its authored one.
#[test]
fn a_sixteen_by_sixteen_object_uses_its_effective_elevation() {
    let mut event = object("OBJ_EVENT_GFX_PICHU_DOLL", 5, 5, MovementType::FaceDown);
    event.elevation = 4;
    let events: &'static [ObjectEvent] = Box::leak(Box::new([event]));
    let sheet = object_sheet("OBJ_EVENT_GFX_PICHU_DOLL").unwrap();
    let binding = SpriteBinding {
        base_tile: 50,
        palette_bank: sheet.palette_bank,
        width: 16,
        height: 16,
        facing: sheet.facing,
        packed_cells: 1,
    };
    let mut bindings = HashMap::new();
    bindings.insert("OBJ_EVENT_GFX_PICHU_DOLL", binding);
    let player = PlayerState::new((5, 5), 3, Direction::South);
    let data = EventData::new();
    for (elevation, priority) in [(4, 1), (3, 2)] {
        let drawn = elevated_oam_entries(
            &ObjectEventCollection::from_templates(events),
            &[elevation],
            &bindings,
            &player,
            &data,
        );
        let entry = drawn[0].entries[0];
        assert_eq!(entry.priority(), priority, "elevation {elevation}");
        assert_eq!((entry.x(), entry.y()), (112, 72));
        assert_eq!(entry.dimensions(), (16, 16));
        assert_eq!(entry.tile_index(), 50);
    }
}

/// Only packed cells take tiles, in ascending cell order.
#[test]
fn packed_cells_take_consecutive_blocks_in_cell_order() {
    let binding = SpriteBinding {
        base_tile: 10,
        packed_cells: 0b1_0101,
        ..SpriteBinding::PEOPLE_16X32
    };
    assert_eq!(binding.frame_tile(0), 10);
    assert_eq!(binding.frame_tile(2), 18);
    assert_eq!(binding.frame_tile(4), 26);
    let vigoroth = SpriteBinding {
        width: 32,
        height: 32,
        packed_cells: 0b01001,
        ..SpriteBinding::PEOPLE_16X32
    };
    assert_eq!(vigoroth.frame_tile(3), 16);
}

#[test]
fn inanimate_and_cutting_tree_graphics_ignore_facing() {
    for id in [
        "OBJ_EVENT_GFX_TRUCK",
        "OBJ_EVENT_GFX_ITEM_BALL",
        "OBJ_EVENT_GFX_CUTTABLE_TREE",
        "OBJ_EVENT_GFX_MOVING_BOX",
    ] {
        let facing = object_sheet(id).unwrap().facing;
        assert_eq!(facing, Facing::Fixed(0), "{id}");
        for direction in [
            Direction::South,
            Direction::North,
            Direction::West,
            Direction::East,
        ] {
            assert_eq!(facing.cell_for(direction), (0, false), "{id}");
        }
    }
}

/// An object's facing changes after load (`faceplayer`, a script turn), so the
/// packed cells must cover every facing, not only the initial one: an
/// unpacked cell would silently draw as the nearest packed frame.
#[test]
fn every_facing_selects_a_cell_the_binding_packs() {
    let all = [
        Direction::South,
        Direction::North,
        Direction::West,
        Direction::East,
    ];
    let mut facings = vec![Facing::PEOPLE];
    facings.extend(
        ["OBJ_EVENT_GFX_TRUCK", "OBJ_EVENT_GFX_PICHU_DOLL"]
            .map(|id| object_sheet(id).unwrap().facing),
    );
    for facing in facings {
        let packed = facing.reachable_cells();
        for direction in all {
            let (cell, _) = facing.cell_for(direction);
            assert_ne!(packed & (1 << cell), 0, "{facing:?} {direction:?}");
        }
    }
    assert_eq!(Facing::PEOPLE.reachable_cells(), 0b111);
    assert_eq!(Facing::Fixed(0).reachable_cells(), 0b1);
}
