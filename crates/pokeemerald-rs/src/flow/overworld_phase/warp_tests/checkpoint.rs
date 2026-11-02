//! Every declared physical entrance and return warp among the checkpoint maps
//! (Littleroot Town, Oldale Town and their interiors), driven through the
//! production trigger for the warp tile's metatile behavior and then
//! [`OverworldPhase::warp_to`], over scenes loaded from the checkout pack.

use super::super::OverworldPhase;
use assets::{MapId, WarpDestination, WarpEvent, WarpId};
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_NON_ANIMATED_DOOR, MB_NORMAL, MB_SOUTH_ARROW_WARP,
};
use engine::overworld::{
    trigger_animated_door_warp, trigger_arrow_warp, trigger_door_warp, Direction, PlayerState,
    WarpTrigger,
};

/// The checkpoint maps that declare warps between one another. Route 101,
/// Route 103 and the moving truck declare none among them; Pokemon Center 2F's
/// Union Room and Trade Center warps, Route 103's Altering Cave warp and the
/// truck's dynamic exits leave the set.
const CHECKPOINT_MAPS: [MapId; 12] = [
    MapId("MAP_LITTLEROOT_TOWN"),
    MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"),
    MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
    MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F"),
    MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
    MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
    MapId("MAP_OLDALE_TOWN"),
    MapId("MAP_OLDALE_TOWN_HOUSE1"),
    MapId("MAP_OLDALE_TOWN_HOUSE2"),
    MapId("MAP_OLDALE_TOWN_MART"),
    MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_1F"),
    MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_2F"),
];

/// `MB_UP_ESCALATOR` and `MB_DOWN_ESCALATOR` (`metatile_behaviors.h`): the two
/// Pokemon Center escalator edges, which `is_warp_trigger` does not accept.
const ESCALATOR_BEHAVIORS: [u8; 2] = [0x6A, 0x6B];

struct Edge {
    source: MapId,
    index: usize,
    warp: WarpEvent,
    destination: MapId,
    destination_warp: u8,
}

/// Every fixed warp event of [`CHECKPOINT_MAPS`] whose destination is also in
/// it, in map and event order.
fn edges() -> Vec<Edge> {
    let mut edges = Vec::new();
    for source in CHECKPOINT_MAPS {
        let events = assets::MapEventsTable::new().resolve(source).unwrap();
        for (index, warp) in events.warp_events.iter().enumerate() {
            let (WarpDestination::Map(destination), WarpId::Fixed(destination_warp)) =
                (warp.dest_map, warp.dest_warp_id)
            else {
                continue;
            };
            if CHECKPOINT_MAPS.contains(&destination) {
                edges.push(Edge {
                    source,
                    index,
                    warp: *warp,
                    destination,
                    destination_warp,
                });
            }
        }
    }
    edges
}

fn fresh_event_data() -> engine::event_data::EventData {
    crate::new_game::init_save_blocks_for_new_game()
        .0
        .event_data
}

fn checkpoint_phase(map: MapId, position: (i32, i32), elevation: u8) -> OverworldPhase {
    let scene = crate::overworld::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        map,
        crate::overworld::PlayerCharacter::Brendan,
        &fresh_event_data(),
    )
    .unwrap_or_else(|err| panic!("{map:?} must load from the checkout pack: {err}"));
    OverworldPhase::for_test_with_source(
        scene,
        map,
        PlayerState::new(position, elevation, Direction::North),
        None,
        crate::pack_source::PackSource::Repo,
    )
}

/// The warp-in facing `GetAdjustedInitialDirection` gives an arrival on a tile
/// of `behavior` (`overworld.c:929-951`), written from upstream rather than
/// through `warp_in_facing`.
fn expected_arrival_facing(behavior: u8) -> Direction {
    match behavior {
        MB_SOUTH_ARROW_WARP => Direction::North,
        MB_ANIMATED_DOOR | MB_NON_ANIMATED_DOOR | MB_NORMAL => Direction::South,
        other if ESCALATOR_BEHAVIORS.contains(&other) => Direction::South,
        other => panic!("unexpected arrival behavior {other:#04x}"),
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_checkpoint_set_declares_twenty_seven_internal_edges_in_four_shapes() {
    let edges = edges();
    assert_eq!(edges.len(), 27, "the set's fixed internal warp events");
    let mut by_behavior = std::collections::BTreeMap::<u8, usize>::new();
    for edge in &edges {
        let phase = checkpoint_phase(edge.source, (edge.warp.x.into(), edge.warp.y.into()), 3);
        let behavior = phase
            .scene
            .runtime()
            .metatile_behavior(edge.warp.x.into(), edge.warp.y.into())
            .unwrap();
        *by_behavior.entry(behavior).or_default() += 1;
    }
    assert_eq!(
        by_behavior,
        std::collections::BTreeMap::from([
            (MB_NON_ANIMATED_DOOR, 4),
            (MB_SOUTH_ARROW_WARP, 14),
            (MB_ANIMATED_DOOR, 7),
            (0x6A, 1),
            (0x6B, 1),
        ]),
        "7 animated doors, 14 south-arrow exits, 4 staircases, 2 escalators"
    );
}

/// Each edge: the warp tile's own trigger resolves the declared destination
/// through [`engine::overworld::MapRuntime`], [`OverworldPhase::warp_to`]
/// lands there, and the phase, scene and save agree on the arrival.
///
/// The two escalator edges have no production trigger: `is_warp_trigger`
/// accepts only door behaviors, and upstream's completed-step escalator warp
/// (`field_control_avatar.c:702-714`) is unmodelled, so they are asserted
/// untriggerable here rather than driven around their missing trigger.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn every_checkpoint_entrance_and_return_warp_lands_in_agreement() {
    let mut driven = 0;
    let mut untriggerable = Vec::new();
    for edge in edges() {
        let (x, y, elevation) = (
            i32::from(edge.warp.x),
            i32::from(edge.warp.y),
            edge.warp.elevation,
        );
        let label = format!(
            "{:?} warp {} -> {:?}:{}",
            edge.source, edge.index, edge.destination, edge.destination_warp
        );
        let mut phase = checkpoint_phase(edge.source, (x, y), elevation);
        let trigger = {
            let runtime = phase.scene.runtime();
            let behavior = runtime.metatile_behavior(x, y).unwrap();
            match behavior {
                MB_NON_ANIMATED_DOOR => trigger_door_warp(&runtime, x, y, elevation),
                MB_ANIMATED_DOOR => {
                    trigger_animated_door_warp(&runtime, x, y, elevation, Direction::North)
                }
                MB_SOUTH_ARROW_WARP => {
                    trigger_arrow_warp(&runtime, x, y, elevation, Direction::South)
                }
                other => {
                    assert!(
                        ESCALATOR_BEHAVIORS.contains(&other),
                        "{label}: behavior {other:#04x}"
                    );
                    assert_eq!(
                        trigger_door_warp(&runtime, x, y, elevation),
                        None,
                        "{label}: an escalator has no door-shaped trigger"
                    );
                    untriggerable.push(label);
                    continue;
                }
            }
        };
        assert_eq!(
            trigger,
            Some(WarpTrigger::Resolved {
                map: edge.destination,
                warp_id: edge.destination_warp,
            }),
            "{label}"
        );

        phase.warp_to(edge.destination, edge.destination_warp);

        let header = assets::MapHeaderTable::new()
            .header(edge.destination)
            .unwrap();
        let arrival = assets::MapEventsTable::new()
            .resolve(edge.destination)
            .unwrap()
            .warp_events[usize::from(edge.destination_warp)];
        assert_eq!(phase.map_id, edge.destination, "{label}: phase map");
        assert_eq!(phase.scene.map_id(), edge.destination, "{label}: scene map");
        assert_eq!(
            phase.save1.location.map_group,
            i8::try_from(header.group).unwrap(),
            "{label}"
        );
        assert_eq!(
            phase.save1.location.map_num,
            i8::try_from(header.num).unwrap(),
            "{label}"
        );
        assert_eq!(
            phase.save1.location.warp_id,
            i8::try_from(edge.destination_warp).unwrap(),
            "{label}: save warp id"
        );
        assert_eq!(
            (phase.save1.location.x, phase.save1.location.y),
            (-1, -1),
            "{label}: a resolved warp records no coordinates"
        );
        assert_eq!(
            phase.player.position(),
            (i32::from(arrival.x), i32::from(arrival.y)),
            "{label}: lands on the destination warp event"
        );
        let runtime = phase.scene.runtime();
        let arrival_behavior = runtime
            .metatile_behavior(i32::from(arrival.x), i32::from(arrival.y))
            .unwrap();
        assert_eq!(
            Some(phase.player.elevation()),
            runtime.arrival_elevation(i32::from(arrival.x), i32::from(arrival.y)),
            "{label}: arrival elevation"
        );
        assert_eq!(
            phase.player.facing(),
            expected_arrival_facing(arrival_behavior),
            "{label}: warp-in facing"
        );
        driven += 1;
    }
    assert_eq!(driven, 25);
    assert_eq!(untriggerable.len(), 2, "{untriggerable:?}");
}

/// Walking north into each of Oldale Town's four animated doors through
/// [`OverworldPhase::step`] enters its building: the owner could enter none of
/// them. The pre-movement animated-door check fires on the first held frame
/// (`field_control_avatar.c:170-178`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn holding_north_at_each_oldale_door_enters_its_building() {
    use platform::Buttons;
    let oldale = MapId("MAP_OLDALE_TOWN");
    let doors: Vec<Edge> = edges().into_iter().filter(|e| e.source == oldale).collect();
    assert_eq!(doors.len(), 4, "house 1, house 2, Pokemon Center, Mart");
    for edge in doors {
        let (x, y) = (i32::from(edge.warp.x), i32::from(edge.warp.y));
        let mut phase = checkpoint_phase(oldale, (x, y + 1), 3);
        let (behavior, standing_elevation) = {
            let runtime = phase.scene.runtime();
            (
                runtime.metatile_behavior(x, y).unwrap(),
                runtime.metatile_cell(x, y + 1).unwrap().elevation,
            )
        };
        assert_eq!(behavior, MB_ANIMATED_DOOR, "{:?}", edge.destination);
        phase.player = PlayerState::new((x, y + 1), standing_elevation, Direction::North);

        phase.step(super::super::test_support::held(Buttons::UP));

        assert_eq!(phase.map_id, edge.destination, "door at ({x}, {y})");
        let arrival = assets::MapEventsTable::new()
            .resolve(edge.destination)
            .unwrap()
            .warp_events[usize::from(edge.destination_warp)];
        assert_eq!(
            phase.player.position(),
            (i32::from(arrival.x), i32::from(arrival.y)),
            "door at ({x}, {y})"
        );
        // The interior's exit mat is a south arrow warp: arrival faces north.
        assert_eq!(
            phase.player.facing(),
            Direction::North,
            "{:?}",
            edge.destination
        );
    }
}
