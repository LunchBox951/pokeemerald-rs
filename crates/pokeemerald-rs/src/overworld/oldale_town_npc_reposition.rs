//! `OldaleTown_OnTransition`'s `setobjectxyperm`/`setobjectmovementtype`
//! pair (issue #281): the footprints man and the mart employee are
//! repositioned, unconditionally, before the player can ever see Oldale
//! Town.
//!
//! # Upstream, read in full
//!
//! `OldaleTown_OnTransition` (`pokeemerald/data/maps/OldaleTown/scripts.inc:5-11`):
//!
//! ```text
//! OldaleTown_OnTransition:
//!     call Common_EventScript_SetupRivalGfxId
//!     setflag FLAG_VISITED_OLDALE_TOWN
//!     call_if_unset FLAG_ADVENTURE_STARTED, OldaleTown_EventScript_BlockWestEntrance
//!     call_if_unset FLAG_RECEIVED_POTION_OLDALE, OldaleTown_EventScript_MoveMartEmployee
//!     call_if_set FLAG_ADVENTURE_STARTED, OldaleTown_EventScript_SetOldaleState
//!     end
//! ```
//!
//! wired to `MAP_SCRIPT_ON_TRANSITION` (`scripts.inc:1-3`), so it runs on
//! *every* entry into Oldale Town, before the room is ever drawn. The two
//! branches this module ports:
//!
//! - `OldaleTown_EventScript_BlockWestEntrance` (`scripts.inc:18-21`):
//!   `setobjectxyperm LOCALID_FOOTPRINTS_MAN, 1, 11` then
//!   `setobjectmovementtype LOCALID_FOOTPRINTS_MAN, MOVEMENT_TYPE_FACE_LEFT`.
//! - `OldaleTown_EventScript_MoveMartEmployee` (`scripts.inc:23-26`):
//!   `setobjectxyperm LOCALID_OLDALE_MART_EMPLOYEE, 13, 14` then
//!   `setobjectmovementtype LOCALID_OLDALE_MART_EMPLOYEE, MOVEMENT_TYPE_FACE_DOWN`.
//!
//! `LOCALID_FOOTPRINTS_MAN`/`LOCALID_OLDALE_MART_EMPLOYEE` are `local_id`s 3
//! and 2 respectively -- their 1-based position in
//! `data/maps/OldaleTown/map.json`'s own `object_events` array (mirrors
//! `crates/assets/src/map_events.rs`'s own module docs on how `local_id` is
//! computed, not transcribed).
//!
//! **Both `call_if_unset` guards are always taken.** `FLAG_ADVENTURE_STARTED`
//! and `FLAG_RECEIVED_POTION_OLDALE` are ordinary story flags this port has
//! no script engine to ever *set* (no `EventScript_ResetAllMapFlags`-style
//! catch-all sets either -- `assets::RESET_MAP_FLAGS` doesn't include
//! them), so both flags read `false` forever and both branches fire on
//! every single visit -- the reposition is not a one-time event this port
//! could plausibly race or skip, it is simply *what Oldale Town's object
//! events are*, permanently, from this port's perspective. That is also why
//! this is modelled as a standing correction to the object-event data
//! itself ([`resolve_map_events`]) rather than a call-site-wired
//! on-transition effect like
//! `crate::flow::overworld_phase::route103_rival_trigger::setup_rival_gfx_id_on_transition`
//! (which mutates live [`engine::event_data::EventData`], read back later):
//! there is no live var/flag here for anything to read, and both consumers
//! of Oldale Town's *object events* that exist today -- rendering
//! ([`super::load_room`] -> [`super::sprites::SceneSprites::from_pack`])
//! and collision/interaction (`crate::flow::overworld_phase::step`'s
//! per-frame [`engine::overworld::MapRuntime`] rebuild) -- were switched to
//! [`resolve_map_events`]. Being a drop-in replacement cuts both ways: the
//! raw [`assets::MapEventsTable::resolve`] still compiles and still looks
//! right, so a *future* object-event consumer could forget the wrapper --
//! the staleness-guard test below catches generated-data drift, not an
//! un-switched call site, and a reviewer adding such a consumer must route
//! it through this chokepoint.
//!
//! # What upstream's `SetObjEventTemplateCoords` actually mutates
//!
//! `ScrCmd_setobjectxyperm` (`pokeemerald/src/scrcmd.c:1094-1101`) calls
//! `SetObjEventTemplateCoords` (`pokeemerald/src/overworld.c:490-503`),
//! which walks `gSaveBlock1Ptr->objectEventTemplates` (the current map's
//! *save-block* copy of its object event templates, refreshed whenever a
//! map loads) and overwrites the matching `localId`'s `x`/`y` in place --
//! not the live sprite's position directly, but the template a later
//! (re)spawn reads. Since `MAP_SCRIPT_ON_TRANSITION` runs before
//! `TrySpawnObjectEvents` on every load (`RunOnTransitionMapScript`,
//! `overworld.c:860`, ahead of `InitObjectEventsLocal`,
//! `:2163-2178`) and this port's own guards are always taken, the
//! observable result collapses to the simple case this module ports: the
//! footprints man spawns at `(1, 11)` facing left and the mart employee at
//! `(13, 14)` facing down on every visit, full stop -- the "was this
//! already applied to the template" persistence question that matters
//! upstream (a second, redundant `setobjectxyperm` writing the same value)
//! is unobservable here.
//!
//! # Ledger
//!
//! Recorded on `data/maps/OldaleTown#OldaleTown_OnTransition`: the
//! repositioning is now modelled; `Common_EventScript_SetupRivalGfxId`
//! (ported separately, Route 103's own on-transition effect --
//! `route103_rival_trigger`'s module docs) and
//! `OldaleTown_EventScript_SetOldaleState` (dead upstream code -- its own
//! comment: "nothing uses `VAR_OLDALE_TOWN_STATE`") stay out of scope, as
//! does `setflag FLAG_VISITED_OLDALE_TOWN` (nothing in this port's bundled
//! data reads that flag back).

use assets::{MapEvents, MapId, MovementType, ObjectEvent, TrainerType};

/// `MAP_OLDALE_TOWN` -- the only map [`resolve_map_events`] ever patches.
pub(crate) const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");

/// `LOCALID_OLDALE_MART_EMPLOYEE` -- position 2 in
/// `data/maps/OldaleTown/map.json`'s `object_events` (module docs).
const LOCALID_OLDALE_MART_EMPLOYEE: u8 = 2;

/// `LOCALID_FOOTPRINTS_MAN` -- position 3 in
/// `data/maps/OldaleTown/map.json`'s `object_events` (module docs).
const LOCALID_FOOTPRINTS_MAN: u8 = 3;

/// Oldale Town's `object_events`, always reflecting
/// `OldaleTown_OnTransition`'s unconditional repositioning (module docs).
///
/// Hand-transcribed from the generated `assets::MapEventsTable`'s own
/// `MAP_OLDALE_TOWN` entry (`crates/assets/src/map_events.rs`), with only
/// the footprints man's and the mart employee's `x`/`y`/`movement_type`
/// overridden to their post-script values -- the girl (`local_id` 1) and
/// the rival (`local_id` 4) are copied verbatim, untouched by either
/// branch. `the_override_table_matches_the_generated_table_everywhere_else`
/// (below) pins every field of all four entries against that live table --
/// equal outright for the girl and the rival, equal apart from the three
/// deliberately-overridden fields for the other two -- so a future
/// `cargo xtask extract` regen that changes Oldale Town's map.json (a new
/// object event, a renamed script, a different flag) fails loudly here
/// instead of silently drifting out of sync.
static OLDALE_TOWN_OBJECT_EVENTS: [ObjectEvent; 4] = [
    ObjectEvent {
        local_id: 1,
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
        // `OldaleTown_EventScript_MoveMartEmployee`'s
        // `setobjectxyperm LOCALID_OLDALE_MART_EMPLOYEE, 13, 14` (module
        // docs): map.json's own `(13, 7)` becomes `(13, 14)`.
        x: 13,
        y: 14,
        elevation: 3,
        // `setobjectmovementtype LOCALID_OLDALE_MART_EMPLOYEE,
        // MOVEMENT_TYPE_FACE_DOWN` -- map.json already declares
        // `MOVEMENT_TYPE_FACE_DOWN`, so this is a no-op restated explicitly
        // (unlike the footprints man below, whose facing genuinely
        // changes), so this literal states the true post-script value
        // directly rather than relying on a coincidence.
        movement_type: MovementType::FaceDown,
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
        // `OldaleTown_EventScript_BlockWestEntrance`'s
        // `setobjectxyperm LOCALID_FOOTPRINTS_MAN, 1, 11` (module docs):
        // map.json's own `(8, 9)` becomes `(1, 11)`.
        x: 1,
        y: 11,
        elevation: 3,
        // `setobjectmovementtype LOCALID_FOOTPRINTS_MAN,
        // MOVEMENT_TYPE_FACE_LEFT` -- overrides map.json's own
        // `MOVEMENT_TYPE_FACE_RIGHT`.
        movement_type: MovementType::FaceLeft,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: "OldaleTown_EventScript_FootprintsMan",
        flag: "0",
    },
    ObjectEvent {
        local_id: 4,
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

/// [`assets::MapEventsTable::resolve`], with Oldale Town's own
/// `object_events` patched to [`OLDALE_TOWN_OBJECT_EVENTS`] (module docs) --
/// a drop-in replacement for every call site that needs `map`'s event data
/// to match what a player actually finds standing in the room, not the bare
/// map.json transcription. `warp_events`/`coord_events`/`bg_events`/
/// `shared_events_map` are copied verbatim from the live generated table
/// (struct-update syntax): only `object_events` is ever overridden, and
/// only when the *resolved* entry's own id -- not `map` itself, so a future
/// map that aliases its events to Oldale Town via `shared_events_map` would
/// still be patched -- is [`OLDALE_TOWN`]. No bundled map currently
/// aliases to Oldale Town, but comparing the resolved id rather than the
/// input keeps this correct if one someday does.
///
/// Called from [`super::load_room`] (rendering, every warp/connection entry
/// into a map) and from `crate::flow::overworld_phase::step`'s per-frame
/// runtime rebuild (collision/interaction) -- **not** from the three other
/// production `MapEventsTable::resolve` sites, which never read
/// `object_events`, so patching them would be a costless no-op left out
/// for that reason alone: `crate::flow::overworld_phase::connections`'s two
/// calls (one resolves a warp landing, one a caller-supplied position --
/// both `warp_events`/metatile only) and
/// `crate::flow::overworld_phase::placement`'s `saved_tile_placement`
/// (metatile only, on the continue-from-save path).
///
/// # Errors
///
/// Same as [`assets::MapEventsTable::resolve`]: `AssetError::UnknownMapEvents`
/// if `map` (or the map it aliases) has no entry.
pub(crate) fn resolve_map_events(map: MapId) -> Result<MapEvents, assets::AssetError> {
    let events = assets::MapEventsTable::new().resolve(map)?;
    if events.id == OLDALE_TOWN {
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

    /// The real generated table's own `MAP_OLDALE_TOWN` entry --
    /// [`OLDALE_TOWN_OBJECT_EVENTS`]'s source of truth.
    fn live_object_events() -> &'static [ObjectEvent] {
        assets::MapEventsTable::new()
            .resolve(OLDALE_TOWN)
            .expect("MAP_OLDALE_TOWN must resolve in the generated table")
            .object_events
    }

    /// The staleness guard [`OLDALE_TOWN_OBJECT_EVENTS`]'s own doc comment
    /// promises: every field of the untouched girl/rival entries matches
    /// the live table exactly, and every field *except* the three
    /// deliberately-overridden ones (`x`/`y` for both NPCs, plus
    /// `movement_type` for the footprints man) matches for the other two.
    #[test]
    fn the_override_table_matches_the_generated_table_everywhere_else() {
        let live = live_object_events();
        assert_eq!(live.len(), 4, "fixture precondition: four object events");
        assert_eq!(OLDALE_TOWN_OBJECT_EVENTS.len(), 4);

        // The girl (local_id 1): untouched, so this must be a byte-for-byte
        // match against the live table.
        assert_eq!(OLDALE_TOWN_OBJECT_EVENTS[0], live[1 - 1]);

        // The mart employee (local_id 2): only x/y move; graphics_id,
        // elevation, movement_type (already FaceDown), ranges, trainer
        // fields, script and flag are all untouched.
        let live_employee = live[LOCALID_OLDALE_MART_EMPLOYEE as usize - 1];
        assert_eq!(live_employee.local_id, LOCALID_OLDALE_MART_EMPLOYEE);
        assert_eq!(
            (live_employee.x, live_employee.y),
            (13, 7),
            "fixture precondition: the mart employee's real map.json position"
        );
        let patched_employee = OLDALE_TOWN_OBJECT_EVENTS[LOCALID_OLDALE_MART_EMPLOYEE as usize - 1];
        assert_eq!(
            patched_employee,
            ObjectEvent {
                x: 13,
                y: 14,
                ..live_employee
            },
            "only x/y may differ from the live table for the mart employee"
        );

        // The footprints man (local_id 3): x/y and movement_type move.
        let live_footprints = live[LOCALID_FOOTPRINTS_MAN as usize - 1];
        assert_eq!(live_footprints.local_id, LOCALID_FOOTPRINTS_MAN);
        assert_eq!(
            (
                live_footprints.x,
                live_footprints.y,
                live_footprints.movement_type
            ),
            (8, 9, MovementType::FaceRight),
            "fixture precondition: the footprints man's real map.json position/facing"
        );
        let patched_footprints = OLDALE_TOWN_OBJECT_EVENTS[LOCALID_FOOTPRINTS_MAN as usize - 1];
        assert_eq!(
            patched_footprints,
            ObjectEvent {
                x: 1,
                y: 11,
                movement_type: MovementType::FaceLeft,
                ..live_footprints
            },
            "only x/y/movement_type may differ from the live table for the footprints man"
        );

        // The rival (local_id 4): untouched.
        assert_eq!(OLDALE_TOWN_OBJECT_EVENTS[3], live[4 - 1]);
    }

    /// [`resolve_map_events`] is a no-op for any map other than Oldale
    /// Town -- Route 103's own rival object event (a real
    /// `OBJ_EVENT_GFX_VAR_0` entry, the closest neighbour this override
    /// could plausibly be confused with) must come back byte-for-byte
    /// identical to the live table.
    #[test]
    fn resolve_map_events_is_a_no_op_for_every_other_map() {
        let route_103 = MapId("MAP_ROUTE103");
        let live = assets::MapEventsTable::new().resolve(route_103).unwrap();
        let resolved = resolve_map_events(route_103).unwrap();
        assert_eq!(resolved, *live);
    }

    /// [`resolve_map_events`] patches Oldale Town's `object_events` to
    /// exactly [`OLDALE_TOWN_OBJECT_EVENTS`], and leaves every other field
    /// (warp/coord/bg events, id, `shared_events_map`) equal to the live
    /// table.
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

    /// A map with no entry at all still errors, exactly like
    /// `assets::MapEventsTable::resolve` -- this wrapper must not swallow
    /// or reshape that error.
    #[test]
    fn resolve_map_events_reports_an_unknown_map_the_same_way() {
        let unknown = MapId("MAP_THIS_DOES_NOT_EXIST");
        assert!(resolve_map_events(unknown).is_err());
        assert_eq!(
            resolve_map_events(unknown),
            assets::MapEventsTable::new().resolve(unknown).copied()
        );
    }
}
