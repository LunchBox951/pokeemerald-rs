//! Overworld runtime (S-5, issue #108): map data binding, movement
//! collision, connection crossing, and warp triggering for a lone on-foot
//! player avatar.
//!
//! Ports the observable field-movement behaviour of upstream
//! `pokeemerald/src/field_player_avatar.c`, `src/fieldmap.c`, and the
//! warp-related subset of `src/field_control_avatar.c`/`src/metatile_behavior.c`
//! `(behavioral-fidelity)`, restricted to the v1 north-star path: the
//! protagonist's room -> downstairs -> Littleroot Town's outdoors -> Route
//! 101's first wild-encounter tile. See each submodule's own docs for the
//! exact upstream symbols it mirrors and the citations backing its test
//! cases.
//!
//! # Modules
//!
//! - [`direction`] — the four cardinal walking directions (`DIR_*`
//!   restricted to on-foot movement).
//! - [`map_runtime`] — [`map_runtime::MapRuntime`], binding one map's typed
//!   header/events/layout-grid/tileset-attribute data together for tile
//!   lookup, event lookup, and connection-edge crossing.
//! - [`collision`] — the passable/impassable/elevation-mismatch
//!   classification a step checks against, including behavior-driven
//!   directional impassability ([`collision::directionally_impassable`],
//!   issue #218).
//! - [`metatile_behavior`] — the small, deliberately incomplete subset of
//!   upstream `MB_*` behavior ids/predicates this slice ports (doors, arrow
//!   warps, land encounters, and the `MB_IMPASSABLE_*` directional family),
//!   with an explicit fail-closed policy for everything it doesn't.
//! - [`warp`] — resolving a [`assets::WarpEvent`] into a typed
//!   destination-map-and-warp-id transition, plus the arrival position and
//!   facing that transition lands on.
//! - [`object_event`] — object-event hide-flag visibility, the
//!   player-facing interactive-object lookup, and the visible-object-at-a-tile
//!   query movement collision consults (issue #161).
//! - [`player`] — [`player::PlayerState`], the tile-position/facing/
//!   sub-tile-step-progress state machine that ties the above together into
//!   one input-poll-at-a-time `step()` call.
//! - [`wild_encounter`] — the per-step wild-encounter roll (issue #169):
//!   upstream's encounter-rate/slot/level draws against the extracted
//!   `gWildMonHeaders` tables, plus the post-transition immunity-step
//!   bookkeeping around them.
//! - [`trainer_sight`] — the `TRAINER_TYPE_NORMAL` sight-cone geometry check
//!   (issue #264): whether a stationary trainer object event's own facing
//!   direction, sight range, and an unobstructed, elevation-compatible line
//!   reach the player.
//!
//! # Scope
//!
//! In scope: a single on-foot player avatar's tile position, facing, walk
//! pacing, collision against static map geometry (grid collision bits +
//! elevation + the `MB_IMPASSABLE_*` behaviors'
//! `IsMetatileDirectionallyImpassable` one-sided walls) and against visible,
//! stationary object events, map-connection crossing, warp triggering, and
//! the facing-tile interactive-object lookup.
//!
//! Out of scope (tracked as future overworld slices, not silently
//! approximated): rendering/camera, NPC/object-event *movement* and AI,
//! script binding of any kind, the bike and running, forced movement (currents,
//! conveyor slopes, ice sliding), ledges, and every `MB_*` behavior this
//! module doesn't explicitly name. Where an unported behavior could
//! otherwise be silently treated as passable/normal, this crate fails
//! closed instead (denies the action) rather than guessing — see
//! [`metatile_behavior`]'s and [`warp`]'s module docs for the specific
//! policy.

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
    object_event_is_visible, visible_object_event_at, visible_object_events,
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
