//! Oldale Town object events, gated by two save flags exactly as upstream's
//! `OldaleTown_OnTransition` gates them (`data/maps/OldaleTown/scripts.inc`).

use assets::{MapEvents, MapId, MovementType, ObjectEvent, TrainerType};
use engine::event_data::EventData;

/// The map whose transition repositions object events.
pub(crate) const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");

const GIRL_LOCAL_ID: u8 = 1;
const LOCALID_OLDALE_MART_EMPLOYEE: u8 = 2;
const LOCALID_FOOTPRINTS_MAN: u8 = 3;
const RIVAL_LOCAL_ID: u8 = 4;

/// `FLAG_ADVENTURE_STARTED` (`include/constants/flags.h:136`), gating the
/// footprints man (`data/maps/OldaleTown/scripts.inc:5,8`).
const FLAG_ADVENTURE_STARTED: u16 = 0x74;

/// `FLAG_RECEIVED_POTION_OLDALE` (`include/constants/flags.h:154`), gating
/// the mart employee (`data/maps/OldaleTown/scripts.inc:5,9`).
const FLAG_RECEIVED_POTION_OLDALE: u16 = 0x84;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ObjectPlacement {
    x: i16,
    y: i16,
    movement_type: MovementType,
}

const MART_EMPLOYEE_BEFORE_TRANSITION: ObjectPlacement = ObjectPlacement {
    x: 13,
    y: 7,
    movement_type: MovementType::FaceDown,
};
const MART_EMPLOYEE_AFTER_TRANSITION: ObjectPlacement = ObjectPlacement {
    x: 13,
    y: 14,
    movement_type: MovementType::FaceDown,
};
const FOOTPRINTS_MAN_BEFORE_TRANSITION: ObjectPlacement = ObjectPlacement {
    x: 8,
    y: 9,
    movement_type: MovementType::FaceRight,
};
const FOOTPRINTS_MAN_AFTER_TRANSITION: ObjectPlacement = ObjectPlacement {
    x: 1,
    y: 11,
    movement_type: MovementType::FaceLeft,
};

/// Builds Oldale Town's object events with the footprints man and mart
/// employee at the given placements, leaving the girl and rival as upstream
/// declares them.
const fn oldale_town_object_events(
    footprints_man: ObjectPlacement,
    mart_employee: ObjectPlacement,
) -> [ObjectEvent; 4] {
    [
        ObjectEvent {
            local_id: GIRL_LOCAL_ID,
            graphics_id: "OBJ_EVENT_GFX_GIRL_3",
            x: 16,
            y: 11,
            elevation: 3,
            movement_type: MovementType::FaceLeft,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "OldaleTown_EventScript_Girl",
            flag: "0",
        },
        ObjectEvent {
            local_id: LOCALID_OLDALE_MART_EMPLOYEE,
            graphics_id: "OBJ_EVENT_GFX_MART_EMPLOYEE",
            x: mart_employee.x,
            y: mart_employee.y,
            elevation: 3,
            movement_type: mart_employee.movement_type,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "OldaleTown_EventScript_MartEmployee",
            flag: "0",
        },
        ObjectEvent {
            local_id: LOCALID_FOOTPRINTS_MAN,
            graphics_id: "OBJ_EVENT_GFX_MANIAC",
            x: footprints_man.x,
            y: footprints_man.y,
            elevation: 3,
            movement_type: footprints_man.movement_type,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "OldaleTown_EventScript_FootprintsMan",
            flag: "0",
        },
        ObjectEvent {
            local_id: RIVAL_LOCAL_ID,
            graphics_id: "OBJ_EVENT_GFX_VAR_0",
            x: 11,
            y: 19,
            elevation: 3,
            movement_type: MovementType::FaceUp,
            movement_range_x: 1,
            movement_range_y: 1,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "OldaleTown_EventScript_Rival",
            flag: "FLAG_HIDE_OLDALE_TOWN_RIVAL",
        },
    ]
}

/// Both flags unset.
static OLDALE_TOWN_OBJECT_EVENTS: [ObjectEvent; 4] = oldale_town_object_events(
    FOOTPRINTS_MAN_AFTER_TRANSITION,
    MART_EMPLOYEE_AFTER_TRANSITION,
);

/// `FLAG_ADVENTURE_STARTED` set, `FLAG_RECEIVED_POTION_OLDALE` unset.
static OLDALE_TOWN_OBJECT_EVENTS_ADVENTURE_STARTED: [ObjectEvent; 4] = oldale_town_object_events(
    FOOTPRINTS_MAN_BEFORE_TRANSITION,
    MART_EMPLOYEE_AFTER_TRANSITION,
);

/// `FLAG_RECEIVED_POTION_OLDALE` set, `FLAG_ADVENTURE_STARTED` unset.
static OLDALE_TOWN_OBJECT_EVENTS_POTION_RECEIVED: [ObjectEvent; 4] = oldale_town_object_events(
    FOOTPRINTS_MAN_AFTER_TRANSITION,
    MART_EMPLOYEE_BEFORE_TRANSITION,
);

fn oldale_transition_applies_to(resolved_map: MapId) -> bool {
    resolved_map == OLDALE_TOWN
}

/// Resolves a map's events, applying Oldale Town's flag-gated repositioning.
///
/// An alias of Oldale Town's events receives the same replacements. Both
/// flags set needs no override: the generated table already matches.
///
/// # Errors
///
/// Returns [`assets::AssetError::UnknownMapEvents`] if the map or its event
/// owner is unknown.
pub(crate) fn resolve_map_events(
    map: MapId,
    event_data: &EventData,
) -> Result<MapEvents, assets::AssetError> {
    let events = assets::MapEventsTable::new().resolve(map)?;
    if !oldale_transition_applies_to(events.id) {
        return Ok(*events);
    }
    let adventure_started = event_data
        .flag_get(FLAG_ADVENTURE_STARTED)
        .expect("FLAG_ADVENTURE_STARTED is an ordinary flag id");
    let potion_received = event_data
        .flag_get(FLAG_RECEIVED_POTION_OLDALE)
        .expect("FLAG_RECEIVED_POTION_OLDALE is an ordinary flag id");
    let object_events: &'static [ObjectEvent] = match (adventure_started, potion_received) {
        (false, false) => &OLDALE_TOWN_OBJECT_EVENTS,
        (true, false) => &OLDALE_TOWN_OBJECT_EVENTS_ADVENTURE_STARTED,
        (false, true) => &OLDALE_TOWN_OBJECT_EVENTS_POTION_RECEIVED,
        (true, true) => return Ok(*events),
    };
    Ok(MapEvents {
        object_events,
        ..*events
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated_object_events() -> &'static [ObjectEvent] {
        assets::MapEventsTable::new()
            .resolve(OLDALE_TOWN)
            .expect("MAP_OLDALE_TOWN must resolve in the generated table")
            .object_events
    }

    fn event_with_local_id(events: &[ObjectEvent], local_id: u8) -> ObjectEvent {
        *events
            .iter()
            .find(|event| event.local_id == local_id)
            .expect("Oldale Town must contain every named object event")
    }

    fn generated_event(local_id: u8) -> ObjectEvent {
        event_with_local_id(generated_object_events(), local_id)
    }

    fn replacement_event(local_id: u8) -> ObjectEvent {
        event_with_local_id(&OLDALE_TOWN_OBJECT_EVENTS, local_id)
    }

    fn local_ids(events: &[ObjectEvent]) -> Vec<u8> {
        events.iter().map(|event| event.local_id).collect()
    }

    fn placement(event: ObjectEvent) -> ObjectPlacement {
        ObjectPlacement {
            x: event.x,
            y: event.y,
            movement_type: event.movement_type,
        }
    }

    fn flags_clear() -> EventData {
        EventData::new()
    }

    /// Declaration order is player-visible: `visible_object_events` keeps it
    /// and `super::npc`'s OAM entries are built in that order, so a generated
    /// table that reorders the same local ids is drift too.
    #[test]
    fn replacement_table_has_every_generated_object_event_in_order() {
        assert_eq!(
            local_ids(&OLDALE_TOWN_OBJECT_EVENTS),
            local_ids(generated_object_events())
        );
    }

    #[test]
    fn transition_leaves_the_girl_and_rival_unchanged() {
        assert_eq!(
            replacement_event(GIRL_LOCAL_ID),
            generated_event(GIRL_LOCAL_ID)
        );
        assert_eq!(
            replacement_event(RIVAL_LOCAL_ID),
            generated_event(RIVAL_LOCAL_ID)
        );
    }

    #[test]
    fn transition_moves_the_mart_employee() {
        let generated_employee = generated_event(LOCALID_OLDALE_MART_EMPLOYEE);
        assert_eq!(
            placement(generated_employee),
            MART_EMPLOYEE_BEFORE_TRANSITION
        );
        assert_eq!(
            replacement_event(LOCALID_OLDALE_MART_EMPLOYEE),
            ObjectEvent {
                x: MART_EMPLOYEE_AFTER_TRANSITION.x,
                y: MART_EMPLOYEE_AFTER_TRANSITION.y,
                movement_type: MART_EMPLOYEE_AFTER_TRANSITION.movement_type,
                ..generated_employee
            }
        );
    }

    #[test]
    fn transition_moves_and_turns_the_footprints_man() {
        let generated_footprints_man = generated_event(LOCALID_FOOTPRINTS_MAN);
        assert_eq!(
            placement(generated_footprints_man),
            FOOTPRINTS_MAN_BEFORE_TRANSITION
        );
        assert_eq!(
            replacement_event(LOCALID_FOOTPRINTS_MAN),
            ObjectEvent {
                x: FOOTPRINTS_MAN_AFTER_TRANSITION.x,
                y: FOOTPRINTS_MAN_AFTER_TRANSITION.y,
                movement_type: FOOTPRINTS_MAN_AFTER_TRANSITION.movement_type,
                ..generated_footprints_man
            }
        );
    }

    #[test]
    fn resolve_map_events_is_a_no_op_for_every_other_map() {
        let route_103 = MapId("MAP_ROUTE103");
        let live = assets::MapEventsTable::new().resolve(route_103).unwrap();
        let resolved = resolve_map_events(route_103, &flags_clear()).unwrap();
        assert_eq!(resolved, *live);
    }

    #[test]
    fn resolve_map_events_patches_only_oldale_towns_object_events() {
        let live = assets::MapEventsTable::new().resolve(OLDALE_TOWN).unwrap();
        let resolved = resolve_map_events(OLDALE_TOWN, &flags_clear()).unwrap();
        assert_eq!(resolved.object_events, &OLDALE_TOWN_OBJECT_EVENTS);
        assert_eq!(
            resolved,
            MapEvents {
                object_events: &OLDALE_TOWN_OBJECT_EVENTS,
                ..*live
            }
        );
    }

    #[test]
    fn resolve_map_events_reports_an_unknown_map_the_same_way() {
        let unknown = MapId("MAP_THIS_DOES_NOT_EXIST");
        assert_eq!(
            resolve_map_events(unknown, &flags_clear()),
            assets::MapEventsTable::new().resolve(unknown).copied()
        );
    }

    /// Both flags unset: both NPCs stay at their moved tiles.
    #[test]
    fn resolve_map_events_repositions_both_npcs_before_the_transition_is_reached() {
        let resolved = resolve_map_events(OLDALE_TOWN, &flags_clear()).unwrap();
        assert_eq!(resolved.object_events, &OLDALE_TOWN_OBJECT_EVENTS);
    }

    /// Both flags set: both NPCs sit at their map.json tiles.
    #[test]
    fn resolve_map_events_leaves_both_npcs_at_their_map_json_tiles_once_the_transition_is_reached()
    {
        let mut event_data = flags_clear();
        event_data.flag_set(FLAG_ADVENTURE_STARTED).unwrap();
        event_data.flag_set(FLAG_RECEIVED_POTION_OLDALE).unwrap();
        let live = assets::MapEventsTable::new().resolve(OLDALE_TOWN).unwrap();
        let resolved = resolve_map_events(OLDALE_TOWN, &event_data).unwrap();
        assert_eq!(resolved, *live);
    }

    /// Each flag gates only its own NPC, matching upstream's two independent
    /// `call_if_unset` lines rather than one combined gate.
    #[test]
    fn resolve_map_events_gates_each_npc_independently() {
        let mut adventure_started_only = flags_clear();
        adventure_started_only
            .flag_set(FLAG_ADVENTURE_STARTED)
            .unwrap();
        let resolved = resolve_map_events(OLDALE_TOWN, &adventure_started_only).unwrap();
        assert_eq!(
            placement(event_with_local_id(
                resolved.object_events,
                LOCALID_FOOTPRINTS_MAN
            )),
            FOOTPRINTS_MAN_BEFORE_TRANSITION
        );
        assert_eq!(
            placement(event_with_local_id(
                resolved.object_events,
                LOCALID_OLDALE_MART_EMPLOYEE
            )),
            MART_EMPLOYEE_AFTER_TRANSITION
        );

        let mut potion_received_only = flags_clear();
        potion_received_only
            .flag_set(FLAG_RECEIVED_POTION_OLDALE)
            .unwrap();
        let resolved = resolve_map_events(OLDALE_TOWN, &potion_received_only).unwrap();
        assert_eq!(
            placement(event_with_local_id(
                resolved.object_events,
                LOCALID_FOOTPRINTS_MAN
            )),
            FOOTPRINTS_MAN_AFTER_TRANSITION
        );
        assert_eq!(
            placement(event_with_local_id(
                resolved.object_events,
                LOCALID_OLDALE_MART_EMPLOYEE
            )),
            MART_EMPLOYEE_BEFORE_TRANSITION
        );
    }
}
