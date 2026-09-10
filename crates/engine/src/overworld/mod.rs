//! On-foot movement, map and object queries, collision, warps, wild encounters, and trainer sight.

pub mod collision;
pub mod direction;
pub mod map_runtime;
pub mod metatile_behavior;
pub mod object_event;
pub mod player;
pub mod trainer_sight;
pub mod warp;
pub mod wild_encounter;

pub use collision::{
    directionally_impassable, elevation_mismatch, elevations_compatible, Collision,
    ELEVATION_MULTI_LEVEL, ELEVATION_TRANSITION,
};
pub use direction::Direction;
pub use map_runtime::{ConnectedMapData, ConnectionCrossing, MapRuntime, NUM_METATILES_IN_PRIMARY};
pub use object_event::{
    facing_object_event, initial_facing_direction, object_event_is_in_view,
    object_event_is_visible, trainer_facing_movement_type, visible_object_event_at,
    visible_object_events, ObjectEventState,
};
pub use player::{PlayerState, StepOutcome, TilePos, WALK_FRAMES_PER_TILE};
pub use trainer_sight::trainer_can_see_player;
pub use warp::{
    resolve_warp_event, trigger_arrow_warp, trigger_door_warp, warp_destination_position,
    warp_in_facing, WarpTrigger,
};
pub use wild_encounter::{
    standard_wild_encounter, WildEncounter, WildEncounterState, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
