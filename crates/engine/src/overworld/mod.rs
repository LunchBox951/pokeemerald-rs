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
//!   classification a step checks against.
//! - [`metatile_behavior`] — the small, deliberately incomplete subset of
//!   upstream `MB_*` behavior ids/predicates this slice ports (doors), with
//!   an explicit fail-closed policy for everything it doesn't.
//! - [`warp`] — resolving a [`assets::WarpEvent`] into a typed
//!   destination-map-and-warp-id transition, plus the arrival position and
//!   facing that transition lands on.
//! - [`object_event`] — object-event hide-flag visibility and the
//!   player-facing interactive-object lookup (issue #161).
//! - [`player`] — [`player::PlayerState`], the tile-position/facing/
//!   sub-tile-step-progress state machine that ties the above together into
//!   one input-poll-at-a-time `step()` call.
//!
//! # Scope
//!
//! In scope: a single on-foot player avatar's tile position, facing, walk
//! pacing, collision against static map geometry (grid collision bits +
//! elevation), map-connection crossing, and warp triggering.
//!
//! Out of scope (tracked as future overworld slices, not silently
//! approximated): rendering/camera, NPC/object-event AI and scripts, script
//! binding of any kind, the bike and running, forced movement (currents,
//! conveyor slopes, ice sliding), ledges, directional metatile impassability
//! (`IsMetatileDirectionallyImpassable` — one-way rails and the like, absent
//! from the v1 path), and every `MB_*` behavior this
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
pub mod warp;

pub use collision::{elevation_mismatch, Collision, ELEVATION_MULTI_LEVEL, ELEVATION_TRANSITION};
pub use direction::Direction;
pub use map_runtime::{ConnectedMapData, ConnectionCrossing, MapRuntime, NUM_METATILES_IN_PRIMARY};
pub use object_event::{
    facing_object_event, initial_facing_direction, object_event_is_visible, visible_object_events,
};
pub use player::{PlayerState, StepOutcome, TilePos, WALK_FRAMES_PER_TILE};
pub use warp::{
    resolve_warp_event, trigger_warp, warp_destination_position, warp_in_facing, WarpTrigger,
};
