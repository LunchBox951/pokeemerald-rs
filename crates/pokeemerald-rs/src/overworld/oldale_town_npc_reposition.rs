//! Oldale Town object events after its always-taken transition branches.
//!
//! This port cannot set `FLAG_ADVENTURE_STARTED` or
//! `FLAG_RECEIVED_POTION_OLDALE`, so both upstream `call_if_unset` branches
//! apply whenever Oldale Town's events resolve.

use assets::{MapEvents, MapId, MovementType, ObjectEvent, TrainerType};

/// The map whose transition repositions object events.
pub(crate) const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");

const GIRL_LOCAL_ID: u8 = 1;
const LOCALID_OLDALE_MART_EMPLOYEE: u8 = 2;
const LOCALID_FOOTPRINTS_MAN: u8 = 3;
const RIVAL_LOCAL_ID: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ObjectPlacement {
    x: i16,
    y: i16,
    movement_type: MovementType,
}

#[cfg(test)]
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
#[cfg(test)]
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

static OLDALE_TOWN_OBJECT_EVENTS: [ObjectEvent; 4] = [
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
        x: MART_EMPLOYEE_AFTER_TRANSITION.x,
        y: MART_EMPLOYEE_AFTER_TRANSITION.y,
        elevation: 3,
        movement_type: MART_EMPLOYEE_AFTER_TRANSITION.movement_type,
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
        x: FOOTPRINTS_MAN_AFTER_TRANSITION.x,
        y: FOOTPRINTS_MAN_AFTER_TRANSITION.y,
        elevation: 3,
        movement_type: FOOTPRINTS_MAN_AFTER_TRANSITION.movement_type,
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
];

fn oldale_transition_applies_to(resolved_map: MapId) -> bool {
    resolved_map == OLDALE_TOWN
}

/// Resolves a map's events and applies Oldale Town's transition repositioning.
///
/// The resolved event owner controls the transition, so an alias of Oldale
/// Town's events receives the same object replacements.
///
/// # Errors
///
/// Returns [`assets::AssetError::UnknownMapEvents`] if the map or its event
/// owner is unknown.
pub(crate) fn resolve_map_events(map: MapId) -> Result<MapEvents, assets::AssetError> {
    let events = assets::MapEventsTable::new().resolve(map)?;
    if oldale_transition_applies_to(events.id) {
        Ok(MapEvents {
            object_events: &OLDALE_TOWN_OBJECT_EVENTS,
            ..*events
        })
    } else {
        Ok(*events)
    }
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

    fn placement(event: ObjectEvent) -> ObjectPlacement {
        ObjectPlacement {
            x: event.x,
            y: event.y,
            movement_type: event.movement_type,
        }
    }

    #[test]
    fn replacement_table_has_every_generated_object_event() {
        assert_eq!(
            OLDALE_TOWN_OBJECT_EVENTS.len(),
            generated_object_events().len()
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
        let resolved = resolve_map_events(route_103).unwrap();
        assert_eq!(resolved, *live);
    }

    #[test]
    fn resolve_map_events_patches_only_oldale_towns_object_events() {
        let live = assets::MapEventsTable::new().resolve(OLDALE_TOWN).unwrap();
        let resolved = resolve_map_events(OLDALE_TOWN).unwrap();
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
            resolve_map_events(unknown),
            assets::MapEventsTable::new().resolve(unknown).copied()
        );
    }
}
