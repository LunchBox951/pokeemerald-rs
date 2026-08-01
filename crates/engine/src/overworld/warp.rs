//! Warp triggering (S-5), ported from the warp-shaped subset of upstream
//! `src/field_control_avatar.c` (`TryStartWarpEventScript` / `TryDoorWarp` /
//! `SetupWarp`).
//!
//! **Simplification against upstream's door/arrow-warp split.** Upstream
//! distinguishes three warp-trigger paths: `TryArrowWarp` (holding a
//! direction into an arrow-warp tile, e.g. a house's doormat or some
//! cave-mouth ledges — `field_control_avatar.c:688-699, 767-780`),
//! `TryDoorWarp` (facing north into an `MB_ANIMATED_DOOR` tile *before*
//! stepping onto it, so the door can visually swing open first), and
//! `TryStartWarpEventScript` (standing on any other warp tile after a
//! completed step). Doors and non-animated warps are on the v1 north-star
//! path (a house's front door, an interior staircase), but this port has no
//! rendering/animation to time a pre-step "door opening" against, so both
//! collapse into one rule: **a door-shaped warp triggers once the player's
//! own tile position matches a [`WarpEvent`](assets::WarpEvent) whose
//! metatile behavior is a recognized trigger**
//! ([`crate::overworld::metatile_behavior::is_warp_trigger`]) — matching
//! upstream's post-step `TryStartWarpEventScript` path for both door kinds,
//! without the extra one-tile-early door animation lead-in.
//!
//! Arrow warps (issue #174) are ported too, as the separate direction-gated
//! predicate [`is_arrow_warp_trigger`] mirroring
//! `IsArrowWarpMetatileBehavior` — [`trigger_warp`] checks both predicates.
//! **Timing deviation, documented rather than hidden:** upstream's
//! `TryArrowWarp` is polled every frame the player holds a direction that
//! matches their current facing (`input->heldDirection && input->dpadDirection
//! == playerDirection`, `field_control_avatar.c:163-167`), independent of
//! `tookStep` — so turning to face an arrow tile's direction while already
//! standing on it (no step taken) fires it too. This port only evaluates
//! [`trigger_warp`] at step *completion*, the same
//! [`crate::overworld::warp`]-module "Warp timing" gate the door path uses
//! (see `crate::flow::overworld_phase::OverworldPhase::step`'s doc), using
//! the direction the just-completed step was taken in (frozen as
//! [`crate::overworld::player::PlayerState::facing`] for the whole
//! transition). This covers the common and DoD-required case — walking onto
//! an arrow tile while holding its direction — but not the "already standing
//! there, then turn to face it" edge case, which does not retrigger a check
//! here `(behavioral-fidelity)`.
//!
//! The destination side of a warp lives here too: [`warp_destination_position`]
//! (where the player lands) and [`warp_in_facing`] (which way they face on
//! arrival, decided by the *destination* tile's behavior — upstream
//! `GetAdjustedInitialDirection`, `src/overworld.c:929-951`).

use assets::{MapId, WarpDestination, WarpEvent, WarpId};

use super::collision::{ELEVATION_MULTI_LEVEL, ELEVATION_TRANSITION};
use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::metatile_behavior::{
    is_east_arrow_warp, is_north_arrow_warp, is_south_arrow_warp, is_warp_trigger,
    is_west_arrow_warp, MB_ANIMATED_DOOR, MB_DEEP_SOUTH_WARP, MB_EAST_ARROW_WARP,
    MB_NON_ANIMATED_DOOR, MB_NORTH_ARROW_WARP, MB_PETALBURG_GYM_DOOR, MB_SHOAL_CAVE_ENTRANCE,
    MB_SOUTH_ARROW_WARP, MB_STAIRS_OUTSIDE_ABANDONED_SHIP, MB_WATER_DOOR,
    MB_WATER_SOUTH_ARROW_WARP, MB_WEST_ARROW_WARP,
};

/// A resolved (or explicitly unresolved) warp trigger, returned by
/// [`trigger_warp`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarpTrigger {
    /// The warp resolves to a concrete destination map and warp index.
    /// Executing it is [`MapRuntime::new`]: bind a fresh `MapRuntime` over
    /// `map`'s data (loaded however the integration lane loads maps), then
    /// place the player at that map's warp event index `warp_id` (see
    /// [`warp_destination_position`]).
    Resolved {
        /// The destination map.
        map: MapId,
        /// The destination map's warp-event index to arrive at.
        warp_id: u8,
    },
    /// A [`WarpEvent`] exists and its behavior triggers, but its destination
    /// can't be resolved by this port: either
    /// [`WarpDestination::Dynamic`] (upstream resolves this from
    /// `gSaveBlock1Ptr->dynamicWarp`, e.g. the Battle Pyramid/Trainer Hill's
    /// per-run floor layouts — no dynamic-warp save state exists in this
    /// engine slice yet) or [`WarpId::Dynamic`]/[`WarpId::SecretBase`] (ditto
    /// for the destination *warp index*, e.g. a visited secret base's
    /// entrance). Fails closed rather than guessing a destination.
    Unsupported,
}

/// Whether standing on a tile with `behavior`, while facing/holding
/// `direction`, triggers an arrow warp — upstream `IsArrowWarpMetatileBehavior`
/// (`field_control_avatar.c:767-780`), dispatched by direction exactly as
/// upstream's own `sArrowWarpMetatileBehaviorChecks` table does
/// (`field_player_avatar.c:225-231`).
#[must_use]
pub const fn is_arrow_warp_trigger(behavior: u8, direction: Direction) -> bool {
    match direction {
        Direction::North => is_north_arrow_warp(behavior),
        Direction::South => is_south_arrow_warp(behavior),
        Direction::West => is_west_arrow_warp(behavior),
        Direction::East => is_east_arrow_warp(behavior),
    }
}

/// Check whether standing at `(x, y, elevation)` on `runtime`'s bound map,
/// facing/holding `direction`, triggers a warp.
///
/// Returns `None` if there is no warp event at that position at all, or if
/// one exists but its metatile behavior isn't a recognized trigger for
/// `direction` (fail closed — see the module docs,
/// [`crate::overworld::metatile_behavior::is_warp_trigger`], and
/// [`is_arrow_warp_trigger`]). Returns
/// `Some(`[`WarpTrigger::Unsupported`]`)` if a triggering warp exists but its
/// destination can't be resolved by this port.
#[must_use]
pub fn trigger_warp(
    runtime: &MapRuntime<'_>,
    x: i32,
    y: i32,
    elevation: u8,
    direction: Direction,
) -> Option<WarpTrigger> {
    let behavior = runtime.metatile_behavior(x, y)?;
    if !is_warp_trigger(behavior) && !is_arrow_warp_trigger(behavior, direction) {
        return None;
    }
    let warp = runtime.warp_event_at(x, y, elevation)?;
    Some(resolve_warp_event(warp))
}

/// Resolve a [`WarpEvent`]'s destination into a [`WarpTrigger`], independent
/// of any metatile-behavior gating (used by [`trigger_warp`]; exposed
/// separately so callers that already have a specific `WarpEvent` in hand —
/// e.g. from [`MapRuntime::warp_event_at`] directly — don't need to re-derive
/// it).
#[must_use]
pub fn resolve_warp_event(warp: &WarpEvent) -> WarpTrigger {
    let map = match warp.dest_map {
        WarpDestination::Map(id) => id,
        WarpDestination::Dynamic => return WarpTrigger::Unsupported,
    };
    let warp_id = match warp.dest_warp_id {
        WarpId::Fixed(id) => id,
        WarpId::Dynamic | WarpId::SecretBase => return WarpTrigger::Unsupported,
    };
    WarpTrigger::Resolved { map, warp_id }
}

/// The `(x, y, elevation)` a player arrives at after warping to `runtime`'s
/// `warp_id`-th warp event, or `None` if that map has no warp event at that
/// index or its destination cell is unavailable.
///
/// This is the destination-side half of upstream `SetupWarp`
/// (`field_control_avatar.c`) plus `SetPlayerCoordsFromWarp`
/// (`overworld.c`) and `ObjectEventUpdateElevation`
/// (`event_object_movement.c`). The warp event selects the arrival position,
/// but its elevation is only a trigger-matching filter; the player's actual
/// elevation comes from the destination grid cell. A multi-level destination
/// cell leaves the avatar at transition elevation,
/// matching upstream's initialization and `ObjectEventUpdateElevation` skip
/// rule. The source-side bookkeeping upstream's `SetupWarp` also does
/// (escape-warp tracking, Trainer Hill/Battle Pyramid floor special-casing)
/// is out of v1 scope.
#[must_use]
pub fn warp_destination_position(runtime: &MapRuntime<'_>, warp_id: u8) -> Option<(i16, i16, u8)> {
    let warp = runtime.events().warp_events.get(usize::from(warp_id))?;
    let cell = runtime.metatile_cell(i32::from(warp.x), i32::from(warp.y))?;
    let elevation = if cell.elevation == ELEVATION_MULTI_LEVEL {
        ELEVATION_TRANSITION
    } else {
        cell.elevation
    };
    Some((warp.x, warp.y, elevation))
}

/// The direction a player warping in faces, decided by the **destination**
/// tile's metatile behavior — upstream `GetAdjustedInitialDirection`
/// (`pokeemerald/src/overworld.c:929-951`), whose `metatileBehavior` argument
/// comes from `GetCenterScreenMetatileBehavior` (`overworld.c:954-957`): the
/// tile at `gSaveBlock1Ptr->pos` *after* `WarpIntoMap` has already applied
/// the warp. The tile the player left has no say in it.
///
/// The branches ported here, in upstream's own test order (which matters:
/// `MB_DEEP_SOUTH_WARP` satisfies both the first and the third predicate,
/// and the first one wins):
///
/// | upstream predicate (`src/metatile_behavior.c`) | ids | result |
/// |---|---|---|
/// | `IsDeepSouthWarp` (`:272-278`) | `MB_DEEP_SOUTH_WARP` | [`Direction::North`] |
/// | `IsNonAnimDoor` (`:262-269`) \|\| `IsDoor` (`:228-235`) | `MB_NON_ANIMATED_DOOR`, `MB_WATER_DOOR`, `MB_ANIMATED_DOOR`, `MB_PETALBURG_GYM_DOOR` | [`Direction::South`] |
/// | `IsSouthArrowWarp` (`:313-319`) | `MB_SOUTH_ARROW_WARP`, `MB_WATER_SOUTH_ARROW_WARP`, `MB_SHOAL_CAVE_ENTRANCE` | [`Direction::North`] |
/// | `IsNorthArrowWarp` (`:304-310`) | `MB_NORTH_ARROW_WARP`, `MB_STAIRS_OUTSIDE_ABANDONED_SHIP` | [`Direction::South`] |
/// | `IsWestArrowWarp` (`:296-302`) | `MB_WEST_ARROW_WARP` | [`Direction::East`] |
/// | `IsEastArrowWarp` (`:288-294`) | `MB_EAST_ARROW_WARP` | [`Direction::West`] |
/// | final `else` | everything else | [`Direction::South`] |
///
/// # Not modelled
///
/// Upstream's three remaining branches all need avatar/world state this
/// engine has no home for yet, and each of them collapses into the final
/// `else`'s [`Direction::South`] here:
///
/// - the cruise-mode branch (`FLAG_SYS_CRUISE_MODE` + `MAP_TYPE_OCEAN_ROUTE`
///   -> `DIR_EAST`, `overworld.c:931-932`) — no event-flag state;
/// - the surfing<->underwater transition branch (`overworld.c:945-947`) — no
///   [`PlayerState`](super::player::PlayerState) surf/dive avatar flags;
/// - the `MetatileBehavior_IsLadder` branch (`overworld.c:948-949`), which
///   keeps `playerStruct->direction` — the facing
///   `StoreInitialPlayerAvatarState` (`overworld.c:883-898`) captured *before*
///   the warp. Ladders are not [`is_warp_trigger`] ids here either (see
///   [`crate::overworld::metatile_behavior`]'s scope note), so a ladder tile
///   is only reachable as a destination, and this port faces such an arrival
///   south rather than preserving the pre-warp facing
///   `(behavioral-fidelity)` deviation.
// Several arms share `Direction::South` with the final `_` arm, and that is
// the point: each one is a distinct upstream branch (`IsDoor`,
// `IsNorthArrowWarp`, the closing `else`) that happens to agree today. Folding
// them into the wildcard would erase which upstream predicate a given behavior
// matched, and silently change meaning if one of them is ever re-derived.
#[allow(clippy::match_same_arms)]
#[must_use]
pub const fn warp_in_facing(destination_behavior: u8) -> Direction {
    match destination_behavior {
        // Tested before the door arm upstream, and `IsNonAnimDoor` matches
        // this id too -- the order is the whole reason it faces north.
        MB_DEEP_SOUTH_WARP => Direction::North,
        MB_NON_ANIMATED_DOOR | MB_WATER_DOOR | MB_ANIMATED_DOOR | MB_PETALBURG_GYM_DOOR => {
            Direction::South
        }
        MB_SOUTH_ARROW_WARP | MB_WATER_SOUTH_ARROW_WARP | MB_SHOAL_CAVE_ENTRANCE => {
            Direction::North
        }
        MB_NORTH_ARROW_WARP | MB_STAIRS_OUTSIDE_ABANDONED_SHIP => Direction::South,
        MB_WEST_ARROW_WARP => Direction::East,
        MB_EAST_ARROW_WARP => Direction::West,
        _ => Direction::South,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::metatile_behavior::MB_NORMAL;
    use assets::{MapConnection, MapEvents, MapHeader, MetatileAttributeTable, MetatileCell};

    fn cell_bytes(width: u16, height: u16, id_at: impl Fn(u16, u16) -> u16) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let raw = MetatileCell {
                    metatile_id: id_at(x, y),
                    collision: 0,
                    elevation: 0,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        bytes
    }

    fn attribute_table(behaviors: &[u8]) -> MetatileAttributeTable<'static> {
        // Each behavior byte becomes one attribute entry (layer type 0).
        let mut bytes = Vec::with_capacity(behaviors.len() * 2);
        for &b in behaviors {
            bytes.extend_from_slice(&u16::from(b).to_le_bytes());
        }
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        MetatileAttributeTable::new(leaked)
    }

    fn runtime_with_door(door_behavior: u8, warp: assets::WarpEvent) -> MapRuntime<'static> {
        let bytes: &'static [u8] = Box::leak(cell_bytes(2, 1, |x, _| x).into_boxed_slice());
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_WARP_TEST"),
            name: "MapWarpTest",
            width: 2,
            height: 1,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(bytes).unwrap();
        let primary_attrs = attribute_table(&[MB_NORMAL, door_behavior]);
        let header: &'static MapHeader = Box::leak(Box::new(MapHeader {
            id: assets::MapId("MAP_WARP_TEST"),
            group: 0,
            num: 0,
            name: "MapWarpTest",
            layout: assets::LayoutId("MAP_WARP_TEST"),
            music: assets::MusicId(0),
            region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: assets::Weather::None,
            map_type: assets::MapType::Indoor,
            allow_bike: false,
            allow_escape: false,
            allow_run: false,
            show_name: false,
            battle_scene: assets::BattleScene::Normal,
            connections: &[] as &'static [MapConnection],
        }));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: assets::MapId("MAP_WARP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: Box::leak(Box::new([warp])),
            coord_events: &[],
            bg_events: &[],
        }));
        MapRuntime::new(
            assets::MapId("MAP_WARP_TEST"),
            header,
            events,
            grid,
            primary_attrs,
            MetatileAttributeTable::new(&[]),
        )
    }

    fn sample_warp() -> assets::WarpEvent {
        assets::WarpEvent {
            x: 1,
            y: 0,
            elevation: 0,
            dest_map: WarpDestination::Map(MapId("MAP_DEST")),
            dest_warp_id: WarpId::Fixed(2),
        }
    }

    #[test]
    fn non_animated_door_triggers_a_resolved_warp() {
        let runtime = runtime_with_door(MB_NON_ANIMATED_DOOR, sample_warp());
        // Direction is irrelevant to a door-shaped trigger -- any of the
        // four still fires it.
        assert_eq!(
            trigger_warp(&runtime, 1, 0, 0, Direction::West),
            Some(WarpTrigger::Resolved {
                map: MapId("MAP_DEST"),
                warp_id: 2,
            })
        );
    }

    #[test]
    fn animated_door_triggers_a_resolved_warp() {
        let runtime = runtime_with_door(MB_ANIMATED_DOOR, sample_warp());
        assert_eq!(
            trigger_warp(&runtime, 1, 0, 0, Direction::South),
            Some(WarpTrigger::Resolved {
                map: MapId("MAP_DEST"),
                warp_id: 2,
            })
        );
    }

    /// Issue #174's whole point: a south arrow-warp tile (the doormat a
    /// house's front door lands on) fires when stepped onto while
    /// facing/holding South -- upstream `TryArrowWarp`.
    #[test]
    fn south_arrow_warp_triggers_a_resolved_warp_when_facing_south() {
        let runtime = runtime_with_door(MB_SOUTH_ARROW_WARP, sample_warp());
        assert_eq!(
            trigger_warp(&runtime, 1, 0, 0, Direction::South),
            Some(WarpTrigger::Resolved {
                map: MapId("MAP_DEST"),
                warp_id: 2,
            })
        );
    }

    /// The direction gate is real, not decorative: the very same tile and
    /// warp event does not trigger for any other facing -- upstream
    /// `IsArrowWarpMetatileBehavior` dispatches on direction and only one of
    /// the four `MetatileBehavior_Is*ArrowWarp` checks can ever match a
    /// given id.
    #[test]
    fn south_arrow_warp_does_not_trigger_facing_any_other_direction() {
        let runtime = runtime_with_door(MB_SOUTH_ARROW_WARP, sample_warp());
        for direction in [Direction::North, Direction::East, Direction::West] {
            assert_eq!(
                trigger_warp(&runtime, 1, 0, 0, direction),
                None,
                "MB_SOUTH_ARROW_WARP must not trigger while facing {direction:?}"
            );
        }
    }

    #[test]
    fn ordinary_ground_never_triggers_even_with_a_warp_event_present() {
        // Position 0 has MB_NORMAL behavior; no warp event sits there
        // either, but the behavior gate alone is enough to deny it even if
        // one did (fail-closed).
        let runtime = runtime_with_door(MB_NON_ANIMATED_DOOR, sample_warp());
        assert_eq!(trigger_warp(&runtime, 0, 0, 0, Direction::South), None);
    }

    #[test]
    fn unrecognized_warp_behavior_fails_closed_even_with_a_matching_warp_event() {
        // MB_LADDER (0x61): a real upstream warp trigger this slice does not
        // port. Even though a WarpEvent sits exactly at this position, the
        // unsupported behavior must suppress the trigger, in every
        // direction -- it is neither a door-shaped nor an arrow-shaped
        // trigger id.
        const MB_LADDER: u8 = 0x61;
        let runtime = runtime_with_door(MB_LADDER, sample_warp());
        for direction in [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ] {
            assert_eq!(trigger_warp(&runtime, 1, 0, 0, direction), None);
        }
    }

    #[test]
    fn wrong_elevation_does_not_trigger() {
        let runtime = runtime_with_door(
            MB_NON_ANIMATED_DOOR,
            assets::WarpEvent {
                elevation: 3,
                ..sample_warp()
            },
        );
        assert_eq!(trigger_warp(&runtime, 1, 0, 5, Direction::South), None);
    }

    /// Mirrors the door-fixture test above but for an arrow-warp tile whose
    /// direction *does* match: elevation is still checked after the
    /// behavior gate passes.
    #[test]
    fn wrong_elevation_does_not_trigger_an_arrow_warp_either() {
        let runtime = runtime_with_door(
            MB_SOUTH_ARROW_WARP,
            assets::WarpEvent {
                elevation: 3,
                ..sample_warp()
            },
        );
        assert_eq!(trigger_warp(&runtime, 1, 0, 5, Direction::South), None);
    }

    /// [`is_arrow_warp_trigger`] itself, direction by direction, including
    /// the North/South aliases
    /// ([`crate::overworld::metatile_behavior::MB_STAIRS_OUTSIDE_ABANDONED_SHIP`]/
    /// [`crate::overworld::metatile_behavior::MB_SHOAL_CAVE_ENTRANCE`]).
    #[test]
    fn is_arrow_warp_trigger_dispatches_by_direction() {
        assert!(is_arrow_warp_trigger(MB_NORTH_ARROW_WARP, Direction::North));
        assert!(is_arrow_warp_trigger(
            MB_STAIRS_OUTSIDE_ABANDONED_SHIP,
            Direction::North
        ));
        assert!(is_arrow_warp_trigger(MB_SOUTH_ARROW_WARP, Direction::South));
        assert!(is_arrow_warp_trigger(
            MB_SHOAL_CAVE_ENTRANCE,
            Direction::South
        ));
        assert!(is_arrow_warp_trigger(MB_WEST_ARROW_WARP, Direction::West));
        assert!(is_arrow_warp_trigger(MB_EAST_ARROW_WARP, Direction::East));

        assert!(!is_arrow_warp_trigger(
            MB_NORTH_ARROW_WARP,
            Direction::South
        ));
        assert!(!is_arrow_warp_trigger(MB_ANIMATED_DOOR, Direction::North));
        assert!(!is_arrow_warp_trigger(
            MB_NON_ANIMATED_DOOR,
            Direction::South
        ));
    }

    #[test]
    fn dynamic_destination_map_is_unsupported() {
        let warp = assets::WarpEvent {
            dest_map: WarpDestination::Dynamic,
            ..sample_warp()
        };
        assert_eq!(resolve_warp_event(&warp), WarpTrigger::Unsupported);
    }

    #[test]
    fn dynamic_and_secret_base_warp_ids_are_unsupported() {
        let dynamic = assets::WarpEvent {
            dest_warp_id: WarpId::Dynamic,
            ..sample_warp()
        };
        let secret_base = assets::WarpEvent {
            dest_warp_id: WarpId::SecretBase,
            ..sample_warp()
        };
        assert_eq!(resolve_warp_event(&dynamic), WarpTrigger::Unsupported);
        assert_eq!(resolve_warp_event(&secret_base), WarpTrigger::Unsupported);
    }

    /// Every ported [`warp_in_facing`] branch, keyed by the raw behavior
    /// byte (upstream `constants/metatile_behaviors.h` enum position), in
    /// upstream's own branch order (`src/overworld.c:929-951`).
    #[test]
    fn warp_in_facing_follows_the_destination_tiles_behavior() {
        // IsDeepSouthWarp, tested before the door arm that also matches it.
        assert_eq!(warp_in_facing(0x6E), Direction::North);
        // IsNonAnimDoor || IsDoor.
        assert_eq!(warp_in_facing(0x60), Direction::South);
        assert_eq!(warp_in_facing(0x6C), Direction::South);
        assert_eq!(warp_in_facing(0x69), Direction::South);
        assert_eq!(warp_in_facing(0x8D), Direction::South);
        // IsSouthArrowWarp -- the doormat a front door lands you on.
        assert_eq!(warp_in_facing(0x65), Direction::North);
        assert_eq!(warp_in_facing(0x6D), Direction::North);
        assert_eq!(warp_in_facing(0x1C), Direction::North);
        // IsNorthArrowWarp.
        assert_eq!(warp_in_facing(0x64), Direction::South);
        assert_eq!(warp_in_facing(0x1B), Direction::South);
        // IsWestArrowWarp / IsEastArrowWarp: face *back out* of the arrow.
        assert_eq!(warp_in_facing(0x63), Direction::East);
        assert_eq!(warp_in_facing(0x62), Direction::West);
    }

    /// The final `else` (`overworld.c:950-951`): every behavior with no
    /// branch of its own faces south, including the ones this port
    /// deliberately does not model (the ladder branch -- see
    /// [`warp_in_facing`]'s "Not modelled" section).
    #[test]
    fn unbranched_destination_behaviors_face_south() {
        const MB_LADDER: u8 = 0x61;
        const MB_UP_ESCALATOR: u8 = 0x6A;
        assert_eq!(warp_in_facing(MB_NORMAL), Direction::South);
        assert_eq!(warp_in_facing(MB_LADDER), Direction::South);
        assert_eq!(warp_in_facing(MB_UP_ESCALATOR), Direction::South);
    }

    /// Reviewer-verified counterexample to a source-tile rule: on the I-3
    /// path, `MAP_LITTLEROOT_TOWN`'s warp #1 sits on an `MB_ANIMATED_DOOR`
    /// (`0x69`) tile, but lands on `..._BRENDANS_HOUSE_1F`'s warp #1, whose
    /// tile is `MB_SOUTH_ARROW_WARP` (`0x65`) -- so the arrival faces
    /// *north*, the opposite of what the source tile's own door branch would
    /// have produced.
    #[test]
    fn a_door_source_tile_can_land_on_a_north_facing_destination() {
        assert_eq!(warp_in_facing(MB_ANIMATED_DOOR), Direction::South);
        assert_eq!(warp_in_facing(MB_SOUTH_ARROW_WARP), Direction::North);
    }

    #[test]
    fn warp_destination_position_uses_the_destination_cell_elevation() {
        let mut bytes = cell_bytes(2, 1, |x, _| x);
        let multi_level = MetatileCell {
            metatile_id: 0,
            collision: 0,
            elevation: ELEVATION_MULTI_LEVEL,
        }
        .pack();
        let elevated = MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        }
        .pack();
        bytes[0..2].copy_from_slice(&multi_level.to_le_bytes());
        bytes[2..4].copy_from_slice(&elevated.to_le_bytes());
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_DEST"),
            name: "MapDest",
            width: 2,
            height: 1,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let header = MapHeader {
            id: MapId("MAP_DEST"),
            group: 0,
            num: 0,
            name: "MapDest",
            layout: assets::LayoutId("MAP_DEST"),
            music: assets::MusicId(0),
            region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: assets::Weather::None,
            map_type: assets::MapType::Indoor,
            allow_bike: false,
            allow_escape: false,
            allow_run: false,
            show_name: false,
            battle_scene: assets::BattleScene::Normal,
            connections: &[],
        };
        let events = MapEvents {
            id: MapId("MAP_DEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[
                assets::WarpEvent {
                    x: 1,
                    y: 0,
                    elevation: 0,
                    dest_map: WarpDestination::Map(MapId("MAP_A")),
                    dest_warp_id: WarpId::Fixed(0),
                },
                assets::WarpEvent {
                    x: 0,
                    y: 0,
                    elevation: 4,
                    dest_map: WarpDestination::Map(MapId("MAP_B")),
                    dest_warp_id: WarpId::Fixed(0),
                },
            ],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            MapId("MAP_DEST"),
            &header,
            &events,
            layout.grid(&bytes).unwrap(),
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        assert_eq!(warp_destination_position(&runtime, 0), Some((1, 0, 3)));
        assert_eq!(warp_destination_position(&runtime, 1), Some((0, 0, 0)));
        assert_eq!(warp_destination_position(&runtime, 5), None);
    }
}
