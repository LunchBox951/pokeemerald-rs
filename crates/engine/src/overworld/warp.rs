//! Warp triggering (S-5), ported from the warp-shaped subset of upstream
//! `src/field_control_avatar.c` (`TryStartWarpEventScript` / `TryDoorWarp` /
//! `SetupWarp`).
//!
//! **Simplification against upstream's door/arrow-warp split.** Upstream
//! distinguishes three warp-trigger paths: `TryArrowWarp` (holding a
//! direction into an arrow-warp tile, e.g. some cave-mouth ledges),
//! `TryDoorWarp` (facing north into an `MB_ANIMATED_DOOR` tile *before*
//! stepping onto it, so the door can visually swing open first), and
//! `TryStartWarpEventScript` (standing on any other warp tile after a
//! completed step). Arrow warps are not on the v1 north-star path and are
//! not ported (see `crate::overworld::metatile_behavior`'s scope note).
//! Doors and non-animated warps *are* on the path (a house's front door, an
//! interior staircase), but this port has no rendering/animation to time a
//! pre-step "door opening" against, so both collapse into one rule: **a warp
//! triggers once the player's own tile position matches a
//! [`WarpEvent`](assets::WarpEvent) whose metatile behavior is a recognized
//! trigger** ([`crate::overworld::metatile_behavior::is_warp_trigger`]) —
//! matching upstream's post-step `TryStartWarpEventScript` path for both
//! door kinds, without the extra one-tile-early door animation lead-in.

use assets::{MapId, WarpDestination, WarpEvent, WarpId};

use super::map_runtime::MapRuntime;
use super::metatile_behavior::is_warp_trigger;

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

/// Check whether standing at `(x, y, elevation)` on `runtime`'s bound map
/// triggers a warp.
///
/// Returns `None` if there is no warp event at that position at all, or if
/// one exists but its metatile behavior isn't a recognized trigger (fail
/// closed — see the module docs and
/// [`crate::overworld::metatile_behavior::is_warp_trigger`]). Returns
/// `Some(`[`WarpTrigger::Unsupported`]`)` if a triggering warp exists but its
/// destination can't be resolved by this port.
#[must_use]
pub fn trigger_warp(
    runtime: &MapRuntime<'_>,
    x: i32,
    y: i32,
    elevation: u8,
) -> Option<WarpTrigger> {
    let behavior = runtime.metatile_behavior(x, y)?;
    if !is_warp_trigger(behavior) {
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

/// The `(x, y, elevation)` a player arrives at after warping to
/// `events`'s `warp_id`-th warp event, or `None` if that map has no warp
/// event at that index.
///
/// This is the destination-side half of upstream `SetupWarp`
/// (`field_control_avatar.c`), which — after this port's simplification —
/// reduces to "read the destination map's own warp table at the given
/// index"; the source-side bookkeeping upstream's `SetupWarp` also does
/// (escape-warp tracking, Trainer Hill/Battle Pyramid floor special-casing)
/// is out of v1 scope.
#[must_use]
pub fn warp_destination_position(
    events: &assets::MapEvents,
    warp_id: u8,
) -> Option<(i16, i16, u8)> {
    events
        .warp_events
        .get(usize::from(warp_id))
        .map(|w| (w.x, w.y, w.elevation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_NON_ANIMATED_DOOR, MB_NORMAL};
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
        assert_eq!(
            trigger_warp(&runtime, 1, 0, 0),
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
            trigger_warp(&runtime, 1, 0, 0),
            Some(WarpTrigger::Resolved {
                map: MapId("MAP_DEST"),
                warp_id: 2,
            })
        );
    }

    #[test]
    fn ordinary_ground_never_triggers_even_with_a_warp_event_present() {
        // Position 0 has MB_NORMAL behavior; no warp event sits there
        // either, but the behavior gate alone is enough to deny it even if
        // one did (fail-closed).
        let runtime = runtime_with_door(MB_NON_ANIMATED_DOOR, sample_warp());
        assert_eq!(trigger_warp(&runtime, 0, 0, 0), None);
    }

    #[test]
    fn unrecognized_warp_behavior_fails_closed_even_with_a_matching_warp_event() {
        // MB_LADDER (102): a real upstream warp trigger this slice does not
        // port. Even though a WarpEvent sits exactly at this position, the
        // unsupported behavior must suppress the trigger.
        const MB_LADDER: u8 = 102;
        let runtime = runtime_with_door(MB_LADDER, sample_warp());
        assert_eq!(trigger_warp(&runtime, 1, 0, 0), None);
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
        assert_eq!(trigger_warp(&runtime, 1, 0, 5), None);
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

    #[test]
    fn warp_destination_position_reads_the_indexed_warp() {
        let events = MapEvents {
            id: MapId("MAP_DEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[
                assets::WarpEvent {
                    x: 0,
                    y: 0,
                    elevation: 0,
                    dest_map: WarpDestination::Map(MapId("MAP_A")),
                    dest_warp_id: WarpId::Fixed(0),
                },
                assets::WarpEvent {
                    x: 9,
                    y: 8,
                    elevation: 3,
                    dest_map: WarpDestination::Map(MapId("MAP_A")),
                    dest_warp_id: WarpId::Fixed(0),
                },
            ],
            coord_events: &[],
            bg_events: &[],
        };
        assert_eq!(warp_destination_position(&events, 1), Some((9, 8, 3)));
        assert_eq!(warp_destination_position(&events, 5), None);
    }
}
