use super::test_support::{
    object, open_runtime, DEFAULT_ELEVATION, EXPECTED_WALK_FRAMES_PER_TILE,
    HIDE_BRENDAN_BEDROOM_RIVAL, RAISED_ELEVATION, RAISED_PRIORITY,
};
use super::*;
use assets::MovementType;
use engine::overworld::Direction;

#[test]
fn object_screen_position_matches_the_player_obj_position_when_colocated_and_at_rest() {
    let (x, y) = object_screen_position((5, 5), (5, 5), (0, 0));
    assert_eq!(x, avatar::PLAYER_OBJ_X);
    assert_eq!(y, avatar::PLAYER_OBJ_Y);
}

#[test]
fn object_screen_position_offsets_by_one_metatile_per_tile_of_distance_at_rest() {
    let metatile_px = u16::try_from(super::METATILE_PX).unwrap();

    let (x, y) = object_screen_position((6, 5), (5, 5), (0, 0));
    assert_eq!(x, avatar::PLAYER_OBJ_X + metatile_px);
    assert_eq!(y, avatar::PLAYER_OBJ_Y);

    let (x2, y2) = object_screen_position((5, 4), (5, 5), (0, 0));
    assert_eq!(x2, avatar::PLAYER_OBJ_X);
    let expected_y = avatar::PLAYER_OBJ_Y - u8::try_from(metatile_px).unwrap();
    assert_eq!(y2, expected_y);
}

#[test]
fn object_screen_position_applies_the_camera_lag_before_wrapping() {
    let metatile_px = i32::from(u16::try_from(super::METATILE_PX).unwrap());
    let (x, _) = object_screen_position((6, 5), (5, 5), (-metatile_px, 0));
    assert_eq!(
        x,
        avatar::PLAYER_OBJ_X,
        "one metatile of distance offset by one metatile of opposite-signed lag \
         must cancel back to the player's own screen column"
    );
}

#[test]
fn camera_placement_uses_the_upstream_walk_duration() {
    assert_eq!(
        engine::overworld::WALK_FRAMES_PER_TILE,
        EXPECTED_WALK_FRAMES_PER_TILE
    );
}

#[test]
fn the_spawn_window_keeps_every_admitted_sprite_clear_of_the_oam_y_wrap() {
    const CANDIDATE_ROW_RADIUS: i32 = 32;
    const EXPECTED_ADMITTED_ROWS: usize = 17;
    const EXPECTED_UNWRAPPED_Y_BOUNDS: (i32, i32) = (-72, 216);

    let player = (40, 40);
    let max_lag = i32::from(engine::overworld::WALK_FRAMES_PER_TILE);
    let screen_rows = i32::try_from(rendering::Framebuffer::HEIGHT).unwrap();

    let mut bindings = HashMap::new();
    bindings.insert(
        "OBJ_EVENT_GFX_MOM",
        SpriteBinding {
            base_tile: avatar::FRAME_BLOCK_TILES,
            palette_bank: NpcPaletteTag::Npc4.bank(),
        },
    );
    let data = EventData::new();
    let here: &'static [ObjectEvent] = Box::leak(Box::new([object(
        "OBJ_EVENT_GFX_MOM",
        40,
        40,
        MovementType::FaceDown,
    )]));
    let drawn = oam_entries(
        here,
        &bindings,
        &PlayerState::new(player, DEFAULT_ELEVATION, Direction::South),
        &data,
    );
    let sprite_height = i32::try_from(drawn[0].dimensions().1).unwrap();

    let mut admitted = 0_usize;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for event_y in (player.1 - CANDIDATE_ROW_RADIUS)..=(player.1 + CANDIDATE_ROW_RADIUS) {
        let event = object(
            "OBJ_EVENT_GFX_MOM",
            i16::try_from(player.0).unwrap(),
            i16::try_from(event_y).unwrap(),
            MovementType::FaceDown,
        );
        if !engine::overworld::object_event_is_in_view(&event, player) {
            continue;
        }
        admitted += 1;
        for lag_y in -max_lag..=max_lag {
            let unwrapped =
                i32::from(avatar::PLAYER_OBJ_Y) + (event_y - player.1) * super::METATILE_PX + lag_y;
            min_y = min_y.min(unwrapped);
            max_y = max_y.max(unwrapped);

            let (_, wrapped) = object_screen_position((player.0, event_y), player, (0, lag_y));
            assert_eq!(
                i32::from(wrapped),
                unwrapped.rem_euclid(OAM_Y_MODULUS),
                "object_screen_position must be the wrapped unwrapped position"
            );
        }
    }

    assert_eq!(
        admitted, EXPECTED_ADMITTED_ROWS,
        "the object-event view window changed"
    );
    assert_eq!(
        (min_y, max_y),
        EXPECTED_UNWRAPPED_Y_BOUNDS,
        "the admitted y range with camera lag changed"
    );

    assert!(
        max_y + sprite_height <= OAM_Y_MODULUS,
        "a {sprite_height}px-tall object event at the bottom of the spawn \
         window ({max_y}) would wrap across the top of the screen"
    );

    assert!(
        OAM_Y_MODULUS + min_y >= screen_rows,
        "the top of the spawn window ({min_y}) aliases to row {}, which \
         must stay below the {screen_rows}-row screen",
        OAM_Y_MODULUS + min_y
    );
    assert!(
        min_y + sprite_height <= 0,
        "and its true position must be entirely above row 0"
    );
}

#[test]
fn oam_entries_skips_a_hidden_object_and_one_with_no_binding() {
    let mut data = EventData::new();
    data.flag_set(HIDE_BRENDAN_BEDROOM_RIVAL).unwrap();

    let hidden = {
        let mut o = object(
            "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
            7,
            1,
            MovementType::FaceDown,
        );
        o.flag = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM";
        o
    };
    let unrecognized = object("OBJ_EVENT_GFX_VAR_0", 0, 0, MovementType::LookAround);
    let events: &'static [ObjectEvent] = Box::leak(Box::new([hidden, unrecognized]));

    let mut bindings = HashMap::new();
    bindings.insert(
        "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
        SpriteBinding {
            base_tile: 0,
            palette_bank: PLAYER_PALETTE_BANK,
        },
    );

    let player = PlayerState::new((7, 2), DEFAULT_ELEVATION, Direction::North);
    let entries = oam_entries(events, &bindings, &player, &data);
    assert!(
        entries.is_empty(),
        "the hidden rival and the unrecognized decoration must both be skipped"
    );
}

#[test]
fn oam_entries_draws_a_visible_recognized_object() {
    let data = EventData::new();
    let mom = object("OBJ_EVENT_GFX_MOM", 2, 6, MovementType::FaceRight);
    let events: &'static [ObjectEvent] = Box::leak(Box::new([mom]));

    let mut bindings = HashMap::new();
    bindings.insert(
        "OBJ_EVENT_GFX_MOM",
        SpriteBinding {
            base_tile: avatar::FRAME_BLOCK_TILES,
            palette_bank: NpcPaletteTag::Npc4.bank(),
        },
    );

    let player = PlayerState::new((2, 6), DEFAULT_ELEVATION, Direction::South);
    let entries = oam_entries(events, &bindings, &player, &data);
    assert_eq!(entries.len(), 1);
    let entry = entries[0];
    assert_eq!(entry.palette_bank(), NpcPaletteTag::Npc4.bank());
    assert!(entry.enabled());
    assert!(entry.h_flip());
    let (frame_west_stand, _) = avatar::stand_frame_for(Direction::West);
    assert_eq!(
        entry.tile_index(),
        avatar::FRAME_BLOCK_TILES + frame_west_stand * avatar::FRAME_TILES
    );
}

#[test]
fn oam_entries_take_their_priority_from_the_templates_elevation() {
    let data = EventData::new();
    let mut bindings = HashMap::new();
    bindings.insert(
        "OBJ_EVENT_GFX_MOM",
        SpriteBinding {
            base_tile: avatar::FRAME_BLOCK_TILES,
            palette_bank: NpcPaletteTag::Npc4.bank(),
        },
    );
    let player = PlayerState::new((2, 6), DEFAULT_ELEVATION, Direction::South);

    let flat = object("OBJ_EVENT_GFX_MOM", 2, 6, MovementType::FaceDown);
    assert_eq!(
        flat.elevation, DEFAULT_ELEVATION,
        "fixture precondition: ordinary floor"
    );
    let flat: &'static [ObjectEvent] = Box::leak(Box::new([flat]));
    assert_eq!(
        oam_entries(flat, &bindings, &player, &data)[0].priority(),
        avatar::PLAYER_OBJ_PRIORITY,
    );

    let raised = {
        let mut o = object("OBJ_EVENT_GFX_MOM", 2, 6, MovementType::FaceDown);
        o.elevation = RAISED_ELEVATION;
        o
    };
    let raised: &'static [ObjectEvent] = Box::leak(Box::new([raised]));
    assert_eq!(
        oam_entries(raised, &bindings, &player, &data)[0].priority(),
        RAISED_PRIORITY,
        "a raised object must not draw at the flat priority"
    );
}

#[test]
fn a_distant_object_event_produces_no_oam_entry_instead_of_wrapping_onto_the_player() {
    const BOY_POSITION: (i32, i32) = (14, 17);
    const NORTH_EDGE: (i32, i32) = (14, 1);
    const NEAR_BOY: (i32, i32) = (14, 12);

    let events = assets::MapEventsTable::new()
        .resolve(assets::MapId("MAP_LITTLEROOT_TOWN"))
        .expect("a bundled map must resolve in the generated table");
    let boy = events
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_BOY_2")
        .expect("Littleroot Town's object events include the boy");
    assert_eq!(
        (i32::from(boy.x), i32::from(boy.y)),
        BOY_POSITION,
        "the bundled boy moved"
    );

    let (_, wrapped_y) = object_screen_position(BOY_POSITION, NORTH_EDGE, (0, 0));
    assert_eq!(
        wrapped_y,
        avatar::PLAYER_OBJ_Y,
        "the distant coordinate must alias before view culling"
    );

    let mut bindings = HashMap::new();
    bindings.insert(
        "OBJ_EVENT_GFX_BOY_2",
        SpriteBinding {
            base_tile: avatar::FRAME_BLOCK_TILES,
            palette_bank: NpcPaletteTag::Npc1.bank(),
        },
    );
    let data = EventData::new();

    let north_edge = PlayerState::new(NORTH_EDGE, DEFAULT_ELEVATION, Direction::South);
    let entries = oam_entries(events.object_events, &bindings, &north_edge, &data);
    assert!(
        entries.iter().all(|e| e.y() != avatar::PLAYER_OBJ_Y),
        "a distant NPC must not alias onto the player's row"
    );
    assert!(
        entries.is_empty(),
        "the boy is the only bound object event here, and he is out of view"
    );

    let near = PlayerState::new(NEAR_BOY, DEFAULT_ELEVATION, Direction::South);
    let entries = oam_entries(events.object_events, &bindings, &near, &data);
    assert_eq!(entries.len(), 1, "in view from five tiles away");
    let tile_distance = BOY_POSITION.1 - NEAR_BOY.1;
    let expected_y =
        avatar::PLAYER_OBJ_Y + u8::try_from(tile_distance * super::METATILE_PX).unwrap();
    assert_eq!(entries[0].y(), expected_y);
}

#[test]
fn oam_entries_glues_a_stationary_npc_to_the_camera_through_every_direction_of_a_step() {
    let midpoint_ticks = EXPECTED_WALK_FRAMES_PER_TILE / 2;
    let remaining_transit_ticks = EXPECTED_WALK_FRAMES_PER_TILE - midpoint_ticks - 1;
    let data = EventData::new();
    let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };

    let mut bindings = HashMap::new();
    bindings.insert(
        "OBJ_EVENT_GFX_MOM",
        SpriteBinding {
            base_tile: avatar::FRAME_BLOCK_TILES,
            palette_bank: NpcPaletteTag::Npc4.bank(),
        },
    );

    for direction in [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
    ] {
        let runtime = open_runtime(10, 10);
        let start = (5, 5);
        let mom = object("OBJ_EVENT_GFX_MOM", 7, 5, MovementType::FaceDown);
        let events: &'static [ObjectEvent] = Box::leak(Box::new([mom]));

        let mut player = PlayerState::new(start, DEFAULT_ELEVATION, direction);
        let at_rest = oam_entries(events, &bindings, &player, &data);
        assert_eq!(
            at_rest.len(),
            1,
            "{direction:?}: Mom must be in view at rest"
        );
        let (rest_x, rest_y) = (at_rest[0].x(), at_rest[0].y());

        let outcome = player.step(Some(direction), &runtime, &no_connections, &data);
        assert!(
            matches!(outcome, engine::overworld::StepOutcome::Advanced { .. }),
            "{direction:?}: an open runtime must let the step through"
        );

        let progress0 = oam_entries(events, &bindings, &player, &data);
        assert_eq!(
            (progress0[0].x(), progress0[0].y()),
            (rest_x, rest_y),
            "{direction:?}: a stationary NPC must not jump when a step starts"
        );

        for _ in 0..midpoint_ticks {
            player.tick();
        }
        assert!(
            player.in_transit(),
            "{direction:?}: the midpoint must remain in transit"
        );
        let mid = oam_entries(events, &bindings, &player, &data);

        for _ in 0..remaining_transit_ticks {
            player.tick();
        }
        assert!(
            player.in_transit(),
            "{direction:?}: the last transit frame must remain in transit"
        );
        let last = oam_entries(events, &bindings, &player, &data);

        player.tick();
        assert!(
            !player.in_transit(),
            "{direction:?}: the final tick must end the transit"
        );
        let settled = oam_entries(events, &bindings, &player, &data);

        let (dx, dy) = direction.delta();
        let step = |a: rendering::OamEntry, b: rendering::OamEntry| {
            (
                i32::from(b.x()) - i32::from(a.x()),
                i32::from(b.y()) - i32::from(a.y()),
            )
        };
        assert_eq!(
            step(progress0[0], mid[0]),
            (
                -dx * i32::from(midpoint_ticks),
                -dy * i32::from(midpoint_ticks)
            ),
            "{direction:?}: progress zero to midpoint"
        );
        assert_eq!(
            step(mid[0], last[0]),
            (
                -dx * i32::from(remaining_transit_ticks),
                -dy * i32::from(remaining_transit_ticks),
            ),
            "{direction:?}: midpoint to last transit frame"
        );
        assert_eq!(
            step(last[0], settled[0]),
            (-dx, -dy),
            "{direction:?}: frame 15 to the first resting frame"
        );

        let fresh_at_destination =
            PlayerState::new(player.position(), DEFAULT_ELEVATION, direction);
        let fresh = oam_entries(events, &bindings, &fresh_at_destination, &data);
        assert_eq!(
            (settled[0].x(), settled[0].y()),
            (fresh[0].x(), fresh[0].y()),
            "{direction:?}: the settled position must match a fresh player already there"
        );
    }
}
