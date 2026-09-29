//! Map-entry transitions: resolved warps ([`OverworldPhase::warp_to`]),
//! explicit-coordinate warps ([`OverworldPhase::warp_to_position`]), saved
//! warps ([`OverworldPhase::warp_to_saved_location`]), and map-edge
//! connection crossings ([`OverworldPhase::cross_connection`]). The
//! [`MapConnections`] edge-geometry resolver [`super::step`] feeds into
//! [`engine::overworld::PlayerState::step`], and
//! [`run_on_transition_map_script`] applies the supported on-transition
//! map-script effect.

use assets::{MapEventsTable, MapHeaderTable};
use engine::overworld::{
    warp_destination_position, warp_in_facing, ConnectedMapData, TilePos, NUM_METATILES_IN_PRIMARY,
};
use engine::save::WarpData;
use std::cell::OnceCell;

use crate::overworld;

use super::OverworldPhase;

/// Resolves map-edge connection-crossing geometry against the real
/// generated map tables and, once a candidate connection's own bounds
/// already match, the extracted asset pack -- feeding
/// [`engine::overworld::PlayerState::step`] via
/// [`super::input::advance_player_one_frame`].
///
/// [`ConnectedMapData::dimensions`] never touches the pack: a map's layout
/// `width`/`height` are metadata baked into the generated, `'static`
/// `assets::LayoutTable`, so every candidate connection's own
/// perpendicular-bounds check costs nothing -- no disk I/O merely for
/// walking near an edge with no matching connection.
/// [`ConnectedMapData::metatile_cell`] does need the pack, for the landing
/// tile's real collision/elevation, and, unlike a warp's once-per-load,
/// runs *per attempted crossing*: holding a direction into a refused
/// crossing retries every frame, so an uncached load here would re-read the
/// whole pack at frame rate. The pack is therefore memoized in the
/// [`OnceCell`] the owning [`OverworldPhase`] carries for exactly this
/// resolver ([`OverworldPhase::connection_pack`]); a *failed* load is not
/// cached, so a transient failure can still succeed on a later attempt.
///
/// `source` is the owning [`OverworldPhase`]'s own retained
/// [`crate::pack_source::PackSource`], so this per-attempt memoized load
/// honors the same pack pin every other phase-owned load does.
pub(super) struct MapConnections<'a> {
    pub(super) pack: &'a OnceCell<assets::pack::AssetPack>,
    pub(super) source: crate::pack_source::PackSource,
}

impl MapConnections<'_> {
    /// The memoized pack, loading it on the first successful call.
    fn load_cached_pack(&self) -> Option<&assets::pack::AssetPack> {
        if let Some(pack) = self.pack.get() {
            return Some(pack);
        }
        let loaded = self.source.load().ok()?;
        // Single-threaded phase: nothing else can race this set.
        let _ = self.pack.set(loaded);
        self.pack.get()
    }
}

impl ConnectedMapData for MapConnections<'_> {
    fn dimensions(&self, map: assets::MapId) -> Option<(u16, u16)> {
        let header = MapHeaderTable::new().header(map).ok()?;
        let layout = assets::LayoutTable::new().layout(header.layout).ok()?;
        Some((layout.width, layout.height))
    }

    fn metatile_cell(&self, map: assets::MapId, x: i32, y: i32) -> Option<assets::MetatileCell> {
        let header = MapHeaderTable::new().header(map).ok()?;
        let layout = assets::LayoutTable::new().layout(header.layout).ok()?;
        let name = overworld::layout_pack_name(header.layout);
        let pack = self.load_cached_pack()?;
        let bytes = pack.layout_map(&name).ok()?;
        let grid = layout.grid(bytes).ok()?;
        grid.cell_at(u16::try_from(x).ok()?, u16::try_from(y).ok()?)
    }

    fn metatile_behavior(&self, map: assets::MapId, x: i32, y: i32) -> Option<u8> {
        let cell = self.metatile_cell(map, x, y)?;
        let header = MapHeaderTable::new().header(map).ok()?;
        let layout = assets::LayoutTable::new().layout(header.layout).ok()?;
        let (tileset, metatile_id) = if cell.metatile_id < NUM_METATILES_IN_PRIMARY {
            (layout.primary_tileset, cell.metatile_id)
        } else {
            (
                layout.secondary_tileset,
                cell.metatile_id - NUM_METATILES_IN_PRIMARY,
            )
        };
        let name = overworld::resolve_tileset_pack_name(tileset).ok()?;
        let attributes = self
            .load_cached_pack()?
            .tileset(name)
            .ok()?
            .metatile_attributes;
        assets::MetatileAttributeTable::new(attributes)
            .attribute_at(metatile_id)?
            .ok()
            .map(|attribute| attribute.behavior)
    }
}

/// Maps whose on-transition script hides their unoccupied secret-base
/// decoration placeholders, restricted to the maps this port bundles.
pub(super) const MAPS_THAT_SET_DECORATION_FLAGS: [assets::MapId; 2] = [
    assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
    assets::MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
];

/// Applies `map`'s on-transition decoration-flag effect to `event_data`
/// before entering it. This port has no script engine, so this is a
/// targeted port of the one on-transition effect that is observable for the
/// maps it bundles; the maps' other on-transition effects are not modeled.
///
/// # Why this must run before object events spawn
///
/// The two player bedrooms' decoration placeholders use flag polarity
/// inverted from an ordinary hide flag: object-event spawning's own gate is
/// `!FlagGet(template->flagId)` (`src/event_object_movement.c:1670-1672`),
/// so an *unset* decoration flag leaves its placeholder spawned, visible,
/// and collidable, not hidden. Nothing else sets these flags on a fresh
/// save, so skipping this call would leave twelve such placeholders active
/// in both bedrooms. Upstream avoids that purely by running its own
/// equivalent (`RunOnTransitionMapScript`, `src/overworld.c:860`) before
/// `TrySpawnObjectEvents` (`src/overworld.c:2163-2178`) reaches that gate;
/// this port preserves that ordering by running from
/// `OverworldPhase::prepare_map_entry_event_data` before the destination
/// scene, and its object events, load.
///
/// # Panics
///
/// Never in practice: every
/// [`assets::object_event_flags::DECORATION_FLAGS`] id is well inside the
/// ordinary flag range `flag_set` accepts, pinned by this module's
/// `every_decoration_flag_id_is_settable` test.
pub(super) fn run_on_transition_map_script(
    map: assets::MapId,
    event_data: &mut engine::event_data::EventData,
) {
    if !MAPS_THAT_SET_DECORATION_FLAGS.contains(&map) {
        return;
    }
    for &id in assets::object_event_flags::DECORATION_FLAGS {
        event_data
            .flag_set(id)
            .expect("every DECORATION_FLAGS id is an ordinary flag id");
    }
}

impl OverworldPhase {
    /// Clears temp field event data, then applies the supported
    /// on-transition script effects, in upstream's own ordering
    /// (`LoadMapFromWarp`/`LoadMapFromCameraTransition`,
    /// `src/overworld.c:848,860` and `:798,807`). Returned uncommitted; see
    /// `Self::stage_transition`.
    fn prepare_map_entry_event_data(
        &self,
        map: assets::MapId,
        player_gender: engine::save::PlayerGender,
    ) -> engine::event_data::EventData {
        let mut event_data = self.save1.event_data.clone();
        event_data.clear_temp_field_event_data();
        run_on_transition_map_script(map, &mut event_data);
        super::first_battle_trigger::sync_route_101_state_on_entry(map, &mut event_data);
        super::route103_rival_trigger::setup_rival_gfx_id_on_transition(
            map,
            &mut event_data,
            player_gender,
        );
        event_data
    }

    /// Prepares `map`'s post-transition event data and scene without
    /// mutating `self`, so every caller below can validate the destination
    /// and commit the transition only once every fallible step has already
    /// succeeded. `None` on any load failure, having touched nothing.
    fn stage_transition(
        &self,
        map: assets::MapId,
    ) -> Option<(engine::event_data::EventData, overworld::OverworldScene)> {
        let event_data = self.prepare_map_entry_event_data(map, self.save2.player_gender);
        let scene = overworld::load_room_from_source(
            self.pack_source,
            map,
            self.save2.player_gender.into(),
            &event_data,
        )
        .ok()?;
        Some((event_data, scene))
    }

    /// Executes a resolved [`engine::overworld::WarpTrigger::Resolved`]
    /// warp: loads `map`'s room, resolves its `warp_id`-th warp event's
    /// arrival position and elevation ([`warp_destination_position`]), and
    /// faces the player the way the destination tile's own metatile
    /// behavior dictates ([`warp_in_facing`]; upstream
    /// `GetAdjustedInitialDirection`, `src/overworld.c:929-951`).
    ///
    /// `map_id` and `scene` are assigned together, after every fallible
    /// lookup has already succeeded:
    /// [`crate::overworld::OverworldScene::runtime`] stamps `map_id` onto a
    /// [`engine::overworld::MapRuntime`] built from `scene`'s own decoded
    /// grid, so the two must never disagree.
    ///
    /// A resolved warp has no fixed destination coordinates, so
    /// `save1.location.x`/`.y` are left at `-1` -- the shape upstream's
    /// `SetWarpDestinationToMapWarp` (`overworld.c:638-641`) passes to
    /// `SetWarpDestination`.
    ///
    /// If the destination can't be resolved -- an unknown map, missing
    /// event data, a room that fails to load, or no warp event at
    /// `warp_id` -- logs and leaves the player exactly where they stood.
    ///
    /// # Panics
    ///
    /// If the destination's generated `MAP_GROUP`/`MAP_NUM` index doesn't
    /// fit the `i8` upstream's `WarpData` stores it in; see
    /// [`warp_data_index`].
    pub(super) fn warp_to(&mut self, map: assets::MapId, warp_id: u8) {
        let Ok(header) = MapHeaderTable::new().header(map) else {
            eprintln!("warp: unknown destination map {map:?} -- staying put");
            return;
        };
        let Ok(events) = MapEventsTable::new().resolve(map) else {
            eprintln!("warp: no event data for destination map {map:?} -- staying put");
            return;
        };
        let Some((transitioned_event_data, scene)) = self.stage_transition(map) else {
            eprintln!("warp: failed to load destination map {map:?} -- staying put");
            return;
        };
        let destination = {
            let runtime = scene.runtime(map, header, events);
            warp_destination_position(&runtime, warp_id).map(|(x, y, elevation)| {
                // An undecodable attribute can't happen for a cell
                // `warp_destination_position` just resolved; MB_NORMAL keeps
                // that unreachable case on `GetAdjustedInitialDirection`'s
                // own default branch instead of inventing a facing.
                let behavior = runtime
                    .metatile_behavior(i32::from(x), i32::from(y))
                    .unwrap_or(engine::overworld::metatile_behavior::MB_NORMAL);
                (x, y, elevation, warp_in_facing(behavior))
            })
        };
        let Some((x, y, elevation, facing)) = destination else {
            eprintln!("warp: destination map {map:?} has no warp event #{warp_id} -- staying put");
            return;
        };

        self.player =
            engine::overworld::PlayerState::new((i32::from(x), i32::from(y)), elevation, facing);
        // The departed map's latched door-check tile must not survive into
        // the destination map.
        self.pending_landing = None;
        self.scene = scene;
        self.map_id = map;
        // Upstream's own `InitTilesetAnimations` reset: animated tiles
        // start over on entry to a new map via warp.
        self.tick = 0;
        self.save1.event_data = transitioned_event_data;
        // `RestartWildEncounterImmunitySteps` (`LoadMapFromWarp`,
        // `src/overworld.c:850`): a warp's first four steps roll no wild
        // encounter.
        self.wild.restart_immunity_steps();
        self.save1.location = WarpData {
            map_group: warp_data_index(header.group, "MAP_GROUP"),
            map_num: warp_data_index(header.num, "MAP_NUM"),
            warp_id: warp_data_index(warp_id, "warp id"),
            x: -1,
            y: -1,
        };
    }

    /// Executes an explicit-coordinate warp: lands on `(x, y)` of `map`
    /// directly, rather than resolving a warp event's own position the way
    /// [`OverworldPhase::warp_to`] does -- mirroring
    /// `SetPlayerCoordsFromWarp`'s own `WARP_ID_NONE` branch
    /// (`src/overworld.c:611-617`).
    ///
    /// Same shape as [`OverworldPhase::warp_to`] otherwise, including its
    /// failure contract, except the landing position is validated directly
    /// against the destination's decoded grid rather than resolved through
    /// a warp event.
    ///
    /// Lands at the destination tile's own elevation immediately, rather
    /// than leaving upstream's spawn-time elevation sentinel in place until
    /// the player's first step: `InitObjectEventStateFromTemplate` sets a
    /// freshly spawned object's `triggerGroundEffectsOnMove = TRUE`
    /// (`event_object_movement.c:1301`), so the very next
    /// `UpdateObjectEventCurrentMovement` call already runs
    /// `DoGroundEffects_OnSpawn` (`:4931`), which reaches
    /// `ObjectEventUpdateElevation` (`:7759-7771`) and overwrites the
    /// sentinel with the real elevation before the player's first step --
    /// this port reads that same elevation up front instead of modeling the
    /// intervening ground-effect frame.
    ///
    /// Unlike [`OverworldPhase::warp_to`], `save1.location.x`/`.y` are
    /// **not** `-1`: `ApplyCurrentWarp` (`overworld.c:540-546`) copies
    /// `sWarpDestination` verbatim, and this method's own `x`/`y` become
    /// that value as-is.
    ///
    /// # Panics
    ///
    /// Same as [`OverworldPhase::warp_to`].
    pub(super) fn warp_to_position(&mut self, map: assets::MapId, x: i16, y: i16) {
        let Ok(header) = MapHeaderTable::new().header(map) else {
            eprintln!("warp: unknown destination map {map:?} -- staying put");
            return;
        };
        let Ok(events) = MapEventsTable::new().resolve(map) else {
            eprintln!("warp: no event data for destination map {map:?} -- staying put");
            return;
        };
        let Some((transitioned_event_data, scene)) = self.stage_transition(map) else {
            eprintln!("warp: failed to load destination map {map:?} -- staying put");
            return;
        };
        let (elevation, facing) = {
            let runtime = scene.runtime(map, header, events);
            let Some(elevation) = runtime.arrival_elevation(i32::from(x), i32::from(y)) else {
                eprintln!(
                    "warp: destination position ({x}, {y}) is outside map {map:?} -- staying put"
                );
                return;
            };
            let behavior = runtime
                .metatile_behavior(i32::from(x), i32::from(y))
                .unwrap_or(engine::overworld::metatile_behavior::MB_NORMAL);
            (elevation, warp_in_facing(behavior))
        };

        self.player =
            engine::overworld::PlayerState::new((i32::from(x), i32::from(y)), elevation, facing);
        self.pending_landing = None;
        self.scene = scene;
        self.map_id = map;
        self.tick = 0;
        self.save1.event_data = transitioned_event_data;
        self.wild.restart_immunity_steps();
        self.save1.location = WarpData {
            map_group: warp_data_index(header.group, "MAP_GROUP"),
            map_num: warp_data_index(header.num, "MAP_NUM"),
            warp_id: -1,
            x,
            y,
        };
        self.save1.pos = engine::save::Coords16 { x, y };
    }

    /// Executes a saved-location warp: lands using the complete saved
    /// [`WarpData`] `destination` names, mirroring upstream's
    /// `SetWarpDestinationToLastHealLocation` + `WarpIntoMap` chain
    /// (`src/overworld.c:665-668, 626-631`). [`saved_warp_position`] tries
    /// `destination`'s three branches in upstream's own order.
    ///
    /// Same shape as [`OverworldPhase::warp_to`] otherwise, including its
    /// failure contract.
    ///
    /// Unlike [`OverworldPhase::warp_to_position`], `save1.location` is set
    /// to `destination` **verbatim**, warp id included: a later white-out
    /// or continue must read back the exact value this warp saw, not a
    /// resolved-position sentinel.
    ///
    /// # Panics
    ///
    /// Same as [`OverworldPhase::warp_to`].
    pub(super) fn warp_to_saved_location(&mut self, map: assets::MapId, destination: WarpData) {
        let Ok(header) = MapHeaderTable::new().header(map) else {
            eprintln!("warp: unknown destination map {map:?} -- staying put");
            return;
        };
        let Ok(events) = MapEventsTable::new().resolve(map) else {
            eprintln!("warp: no event data for destination map {map:?} -- staying put");
            return;
        };
        let Some((transitioned_event_data, scene)) = self.stage_transition(map) else {
            eprintln!("warp: failed to load destination map {map:?} -- staying put");
            return;
        };
        let position = {
            let runtime = scene.runtime(map, header, events);
            saved_warp_position(&runtime, events, destination).map(|(x, y, elevation)| {
                let behavior = runtime
                    .metatile_behavior(i32::from(x), i32::from(y))
                    .unwrap_or(engine::overworld::metatile_behavior::MB_NORMAL);
                (x, y, elevation, warp_in_facing(behavior))
            })
        };
        let Some((x, y, elevation, facing)) = position else {
            eprintln!(
                "warp: saved location {destination:?} names no position inside map {map:?} -- \
                 staying put"
            );
            return;
        };

        self.player =
            engine::overworld::PlayerState::new((i32::from(x), i32::from(y)), elevation, facing);
        self.pending_landing = None;
        self.scene = scene;
        self.map_id = map;
        self.tick = 0;
        self.save1.event_data = transitioned_event_data;
        self.wild.restart_immunity_steps();
        self.save1.location = destination;
        self.save1.pos = engine::save::Coords16 { x, y };
    }

    /// Rebinds `map_id`/`scene`/`save1.location` after `self.player` has
    /// already stepped across a map-edge connection into `to_map`'s own
    /// coordinate space ([`engine::overworld::StepOutcome::Crossed`],
    /// resolved against [`MapConnections`] and deferred to here by
    /// [`OverworldPhase::step`] so this mutable borrow of `self.scene`
    /// never overlaps the frame's still-live runtime).
    ///
    /// Same atomic `map_id`/`scene` rebind discipline as
    /// [`OverworldPhase::warp_to`]. Unlike a warp, `self.player` itself is
    /// left alone -- [`engine::overworld::PlayerState::step`] already
    /// committed its position, elevation, and facing before returning
    /// `Crossed`, so rebuilding a fresh
    /// [`engine::overworld::PlayerState`] here would discard its
    /// in-progress walk animation.
    ///
    /// `tick` is deliberately **not** reset: upstream's own
    /// connection-crossing load (`LoadMapFromCameraTransition`,
    /// `src/overworld.c:784-825`) re-inits only the secondary tileset
    /// counter (`InitSecondaryTilesetAnimation`, `:815`), never the primary
    /// one `tick` models, so a crossing's animated tiles continue
    /// uninterrupted across the seamless transition.
    ///
    /// `save1.location` mirrors upstream's own connection-crossing
    /// bookkeeping: `warp_id`/`x`/`y` are all `-1`, since
    /// `SetWarpDestination(mapGroup, mapNum, WARP_ID_NONE, -1, -1)`
    /// (`overworld.c:633`) names only the destination map, never a warp
    /// event or fixed coordinates.
    ///
    /// If `to_map`'s header can't be resolved or its room can't be loaded,
    /// logs, touches nothing, and returns `false` so the caller can restore
    /// the pre-step stance. Returns `true` on a completed rebind.
    pub(super) fn cross_connection(&mut self, to_map: assets::MapId, to_position: TilePos) -> bool {
        let Ok(header) = MapHeaderTable::new().header(to_map) else {
            eprintln!(
                "connection: unknown destination map {to_map:?} -- staying on the departed \
                 map's data"
            );
            return false;
        };
        let Some((transitioned_event_data, scene)) = self.stage_transition(to_map) else {
            eprintln!(
                "connection: failed to load destination map {to_map:?} -- staying on the \
                 departed map's data"
            );
            return false;
        };

        self.scene = scene;
        self.map_id = to_map;
        // Re-latches onto the entered map, unlike a warp's `None`: the
        // crossing step's own landing tile still needs its door check
        // evaluated against `to_map`.
        self.pending_landing = Some(to_position);
        self.save1.event_data = transitioned_event_data;
        // `RestartWildEncounterImmunitySteps` (`LoadMapFromCameraTransition`,
        // `src/overworld.c:800`).
        self.wild.restart_immunity_steps();
        self.save1.location = WarpData {
            map_group: warp_data_index(header.group, "MAP_GROUP"),
            map_num: warp_data_index(header.num, "MAP_NUM"),
            warp_id: -1,
            x: -1,
            y: -1,
        };
        true
    }
}

/// `SetPlayerCoordsFromWarp` (`src/overworld.c:603-624`): a
/// `destination.warp_id` naming a real warp event on `runtime`'s map lands
/// at that event's own position ([`warp_destination_position`]); otherwise
/// a non-negative `destination.x`/`.y` lands there directly; otherwise the
/// destination map's own center tile.
///
/// `None` when the chosen branch names a position outside the destination's
/// decoded grid -- including a valid `warp_id` whose own event cell cannot
/// decode, which must not fall through to the coordinate branch below it:
/// upstream has already committed to the valid-id branch by that point.
fn saved_warp_position(
    runtime: &engine::overworld::MapRuntime<'_>,
    events: &assets::MapEvents,
    destination: WarpData,
) -> Option<(i16, i16, u8)> {
    if let Ok(warp_id) = u8::try_from(destination.warp_id) {
        if usize::from(warp_id) < events.warp_events.len() {
            return warp_destination_position(runtime, warp_id);
        }
    }
    if destination.x >= 0 && destination.y >= 0 {
        let elevation =
            runtime.arrival_elevation(i32::from(destination.x), i32::from(destination.y))?;
        return Some((destination.x, destination.y, elevation));
    }
    let layout = assets::LayoutTable::new()
        .layout(runtime.header().layout)
        .ok()?;
    let x = i16::try_from(layout.width / 2).ok()?;
    let y = i16::try_from(layout.height / 2).ok()?;
    let elevation = runtime.arrival_elevation(i32::from(x), i32::from(y))?;
    Some((x, y, elevation))
}

/// Narrows a generated map-table index (`MAP_GROUP`, `MAP_NUM`, or a warp
/// event index) into the `i8` upstream's `struct WarpData`
/// (transcribed as [`WarpData`]) stores it in.
///
/// # Panics
///
/// If `value` exceeds `i8::MAX` -- unreachable against any real extraction.
/// Panicking, rather than saturating to a fabricated `127`, avoids silently
/// writing a different, real map's group/num into the save.
pub(super) fn warp_data_index(value: u8, what: &str) -> i8 {
    i8::try_from(value).unwrap_or_else(|_| {
        panic!("{what} {value} does not fit the i8 upstream's struct WarpData stores it in")
    })
}
