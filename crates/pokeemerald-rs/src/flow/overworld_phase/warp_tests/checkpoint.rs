//! Every declared physical entrance and return warp among the checkpoint maps
//! (Littleroot Town, Oldale Town and their interiors), driven through the
//! production trigger for the warp tile's metatile behavior and then
//! [`OverworldPhase::warp_to`], over scenes loaded from the checkout pack.

use super::super::OverworldPhase;
use assets::{MapId, WarpDestination, WarpEvent, WarpId};
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_DOWN_ESCALATOR, MB_NON_ANIMATED_DOOR, MB_NORMAL, MB_SOUTH_ARROW_WARP,
    MB_UP_ESCALATOR,
};
use engine::overworld::{
    trigger_animated_door_warp, trigger_arrow_warp, trigger_door_warp, trigger_escalator_warp,
    Direction, PlayerState, WarpTrigger,
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
        // An escalator arrival takes the default facing here; its arrival
        // callback then faces the player east (asserted separately).
        MB_ANIMATED_DOOR | MB_NON_ANIMATED_DOOR | MB_NORMAL | MB_UP_ESCALATOR
        | MB_DOWN_ESCALATOR => Direction::South,
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
            (MB_UP_ESCALATOR, 1),
            (MB_DOWN_ESCALATOR, 1),
        ]),
        "7 animated doors, 14 south-arrow exits, 4 staircases, 2 escalators"
    );
}

/// Each edge: the warp tile's own trigger resolves the declared destination
/// through [`engine::overworld::MapRuntime`], [`OverworldPhase::warp_to`]
/// lands there, and the phase, scene and save agree on the arrival.
///
/// The two escalator edges trigger on the arrived tile like a door
/// (`field_control_avatar.c:702-714`). Their arrival then faces the player
/// east and walks one tile east off the escalator
/// (`FieldCallback_EscalatorWarpIn`, `field_effect.c:1690-1702,1800-1818`),
/// asserted by the escalator tests below through the ride that precedes it
/// (`DoEscalatorWarp`); this test lands on the warp event directly.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn every_checkpoint_entrance_and_return_warp_lands_in_agreement() {
    let mut driven = 0;
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
                MB_UP_ESCALATOR | MB_DOWN_ESCALATOR => {
                    trigger_escalator_warp(&runtime, x, y, elevation)
                }
                other => panic!("{label}: behavior {other:#04x}"),
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
    assert_eq!(driven, 27);
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

/// Idle frames until an escalator arrival's east step has landed, then the
/// player's tile.
fn walk_off_the_escalator(phase: &mut OverworldPhase) -> (i32, i32) {
    for _ in 0..40 {
        phase.step(platform::ButtonState::new());
    }
    phase.player.position()
}

/// Holds LEFT until the step onto the escalator at `edge`'s source fires its
/// ride, then rides it out with LEFT still held: the field controls are
/// locked for the whole ride, so the held button reaches nothing until the
/// walk-off has begun. Returns with the arrival's east step started.
fn ride_the_escalator(phase: &mut OverworldPhase, edge: &Edge) {
    use platform::Buttons;
    for _ in 0..40 {
        if phase.escalator.is_some() {
            break;
        }
        phase.step(super::super::test_support::held(Buttons::LEFT));
    }
    assert!(
        phase.escalator.is_some(),
        "{:?}: the step onto the escalator starts its ride",
        edge.source
    );
    assert_eq!(
        phase.map_id, edge.source,
        "{:?}: the warp waits for the ride",
        edge.source
    );
    for _ in 0..100 {
        if phase.escalator.is_none() {
            break;
        }
        phase.step(super::super::test_support::held(Buttons::LEFT));
    }
    assert!(
        phase.escalator.is_none(),
        "{:?}: the ride ends",
        edge.source
    );
}

/// Walking west onto each Oldale Pokemon Center escalator through
/// [`OverworldPhase::step`] warps to the other floor's escalator: the
/// completed-step warp check (`TryStartWarpEventScript`,
/// `field_control_avatar.c:702-714`) sits beside the door check and, unlike an
/// arrow warp, needs no held direction once the step lands. The arrival then
/// faces the player east and walks one tile east off the destination
/// escalator (`FieldCallback_EscalatorWarpIn`, `field_effect.c:1690-1702,
/// 1800-1818`), whatever buttons are held.
///
/// The tile east of each escalator is the transition-elevation floor the
/// escalator cell (elevation 4) can be entered from; the neighbours to its
/// north and south sit at elevation 3 and mismatch it.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_each_pokemon_center_escalator_warps_and_walks_off_east() {
    let mut escalators = 0;
    for edge in edges() {
        let (x, y) = (i32::from(edge.warp.x), i32::from(edge.warp.y));
        let start = (x + 1, y);
        let mut phase = checkpoint_phase(edge.source, start, 0);
        let (behavior, standing) = {
            let runtime = phase.scene.runtime();
            (
                runtime.metatile_behavior(x, y).unwrap(),
                runtime.arrival_elevation(start.0, start.1).unwrap(),
            )
        };
        if !matches!(behavior, MB_UP_ESCALATOR | MB_DOWN_ESCALATOR) {
            continue;
        }
        escalators += 1;
        phase.player = PlayerState::new(start, standing, Direction::West);

        ride_the_escalator(&mut phase, &edge);

        let arrival = assets::MapEventsTable::new()
            .resolve(edge.destination)
            .unwrap()
            .warp_events[usize::from(edge.destination_warp)];
        assert_eq!(phase.map_id, edge.destination, "{:?}", edge.source);
        // The ride's last frame turned the player east and started the walk
        // off the arrival escalator.
        assert_eq!(
            phase.player.position(),
            (i32::from(arrival.x) + 1, i32::from(arrival.y)),
            "{:?}",
            edge.source
        );
        assert!(phase.player.in_transit(), "{:?}", edge.source);
        assert_eq!(phase.player.facing(), Direction::East, "{:?}", edge.source);

        assert_eq!(
            walk_off_the_escalator(&mut phase),
            (i32::from(arrival.x) + 1, i32::from(arrival.y)),
            "{:?}: one tile east of the destination escalator",
            edge.source
        );
        assert_eq!(phase.player.facing(), Direction::East, "{:?}", edge.source);
        assert_eq!(phase.map_id, edge.destination, "{:?}", edge.source);
    }
    assert_eq!(escalators, 2, "one escalator on each floor");
}

/// The arrival's east step is held movement, not a keypad step: it runs
/// `ObjectEventSetHeldMovement(GetWalkNormalMovementAction(DIR_EAST))`
/// (`field_effect.c:1813-1818`) and so bypasses `PlayerStep`
/// (`field_player_avatar.c:353-379`), leaving `runningState` at `NOT_MOVING`.
/// `tookStep` needs `runningState == MOVING`
/// (`field_control_avatar.c:118-119`), so the landing runs no step-based
/// script and advances no step counter: `VAR_POISON_STEP_COUNTER` stays put
/// across the walk-off, while the next ordinary step advances it.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn escalator_arrival_walk_off_is_not_a_took_step() {
    use platform::Buttons;
    // `VAR_POISON_STEP_COUNTER` (`pokeemerald/include/constants/vars.h:61`).
    const VAR_POISON_STEP_COUNTER: u16 = 0x402B;
    let counter = |phase: &OverworldPhase| {
        phase
            .save1
            .event_data
            .var_get(VAR_POISON_STEP_COUNTER)
            .unwrap()
    };
    let mut escalators = 0;
    for edge in edges() {
        let (x, y) = (i32::from(edge.warp.x), i32::from(edge.warp.y));
        let start = (x + 1, y);
        let mut phase = checkpoint_phase(edge.source, start, 0);
        let (behavior, standing) = {
            let runtime = phase.scene.runtime();
            (
                runtime.metatile_behavior(x, y).unwrap(),
                runtime.arrival_elevation(start.0, start.1).unwrap(),
            )
        };
        if !matches!(behavior, MB_UP_ESCALATOR | MB_DOWN_ESCALATOR) {
            continue;
        }
        escalators += 1;
        phase.player = PlayerState::new(start, standing, Direction::West);
        let _ = phase.save1.event_data.var_set(VAR_POISON_STEP_COUNTER, 0);

        ride_the_escalator(&mut phase, &edge);
        assert_eq!(phase.map_id, edge.destination, "{:?}", edge.source);
        // The step onto the escalator is a warp step, which skips the count.
        assert_eq!(counter(&phase), 0, "{:?}: warp step", edge.source);

        let arrival = assets::MapEventsTable::new()
            .resolve(edge.destination)
            .unwrap()
            .warp_events[usize::from(edge.destination_warp)];
        assert_eq!(
            walk_off_the_escalator(&mut phase),
            (i32::from(arrival.x) + 1, i32::from(arrival.y)),
            "{:?}: walked one tile east",
            edge.source
        );
        assert_eq!(
            counter(&phase),
            0,
            "{:?}: the walk-off is held movement, not a took-step",
            edge.source
        );

        // Control: an ordinary step off the walk-off tile counts.
        let walked_off = phase.player.position();
        phase.player = PlayerState::new(walked_off, phase.player.elevation(), Direction::East);
        for _ in 0..40 {
            phase.step(super::super::test_support::held(Buttons::RIGHT));
        }
        assert_ne!(phase.player.position(), walked_off, "{:?}", edge.source);
        assert!(
            counter(&phase) > 0,
            "{:?}: an ordinary step advances the counter",
            edge.source
        );
    }
    assert_eq!(escalators, 2, "one escalator on each floor");
}

/// Holding LEFT through the whole arrival: the walk-off is held movement, so
/// `runningState` is `NOT_MOVING` when it ends and the first LEFT poll after it
/// is a `TURN_DIRECTION` (`CheckMovementInputNotOnBike`,
/// `field_player_avatar.c:587-595`), not an immediate step west.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn escalator_arrival_walk_off_ends_not_moving_so_held_input_turns_first() {
    use platform::Buttons;
    let mut escalators = 0;
    for edge in edges() {
        let (x, y) = (i32::from(edge.warp.x), i32::from(edge.warp.y));
        let start = (x + 1, y);
        let mut phase = checkpoint_phase(edge.source, start, 0);
        let (behavior, standing) = {
            let runtime = phase.scene.runtime();
            (
                runtime.metatile_behavior(x, y).unwrap(),
                runtime.arrival_elevation(start.0, start.1).unwrap(),
            )
        };
        if !matches!(behavior, MB_UP_ESCALATOR | MB_DOWN_ESCALATOR) {
            continue;
        }
        escalators += 1;
        phase.player = PlayerState::new(start, standing, Direction::West);
        ride_the_escalator(&mut phase, &edge);
        assert_eq!(phase.map_id, edge.destination, "{:?}", edge.source);
        let arrival = assets::MapEventsTable::new()
            .resolve(edge.destination)
            .unwrap()
            .warp_events[usize::from(edge.destination_warp)];
        let walked_off = (i32::from(arrival.x) + 1, i32::from(arrival.y));

        // LEFT stays held; wait for the east step to land.
        for _ in 0..40 {
            if phase.player.position() == walked_off && !phase.player.in_transit() {
                break;
            }
            phase.step(super::super::test_support::held(Buttons::LEFT));
        }
        assert_eq!(phase.player.position(), walked_off, "{:?}", edge.source);
        // The next held-LEFT frames turn in place before any step west.
        phase.step(super::super::test_support::held(Buttons::LEFT));
        assert_eq!(phase.player.facing(), Direction::West, "{:?}", edge.source);
        assert_eq!(
            phase.player.position(),
            walked_off,
            "{:?}: the turn comes before the step",
            edge.source
        );
        assert!(!phase.player.in_transit(), "{:?}", edge.source);
    }
    assert_eq!(escalators, 2, "one escalator on each floor");
}
