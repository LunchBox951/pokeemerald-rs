//! Warp, door, and map-edge connection transitions (module split of
//! [`crate::flow::overworld_phase`], issue #210, `oop-boundaries`): the
//! [`MapConnections`] resolver [`super::step`] feeds into
//! [`engine::overworld::PlayerState::step`], the on-transition map-script
//! effects [`run_on_transition_map_script`] applies, and the two ways a
//! [`super::OverworldPhase`] actually rebinds to a new map --
//! [`super::OverworldPhase::warp_to`] (a resolved warp) and
//! [`super::OverworldPhase::cross_connection`] (a map-edge crossing, issue
//! #177).

use assets::{MapEventsTable, MapHeaderTable};
use engine::overworld::{warp_destination_position, warp_in_facing, ConnectedMapData, TilePos};
use engine::save::WarpData;
use std::cell::OnceCell;

use crate::overworld;

use super::OverworldPhase;

/// Resolves map-edge connection-crossing geometry (issue #177) against the
/// real generated map tables and, only once a candidate connection's own
/// bounds already match, the extracted asset pack -- the
/// [`ConnectedMapData`] this integration lane feeds
/// [`engine::overworld::PlayerState::step`] (via
/// [`super::input::advance_player_one_frame`]), replacing the
/// `no_connections` stub an earlier revision of this module passed
/// unconditionally (see [`OverworldPhase::cross_connection`]'s doc comment
/// for what happens once a step actually resolves against it).
///
/// [`ConnectedMapData::dimensions`] never touches the pack: a map's layout
/// `width`/`height` are metadata baked into the generated, `'static`
/// `assets::LayoutTable` (`crates/assets/src/map_layouts.rs`'s own module
/// docs: "Grid/border bytes are not in this crate"), so every candidate
/// connection [`engine::overworld::MapRuntime::resolve_connection`]'s own
/// perpendicular-bounds check considers costs nothing -- no disk I/O merely
/// for walking near an edge with no matching connection.
/// [`ConnectedMapData::metatile_cell`] does need the pack -- the landing
/// tile's actual collision/elevation, upstream's `IsPosInConnectingMap`
/// (`pokeemerald/src/fieldmap.c:699-712`) having already passed -- and,
/// unlike [`OverworldPhase::warp_to`]'s once-per-resolved-warp load, it
/// runs *per attempted crossing*: holding a direction into an edge whose
/// crossing is refused retries every frame, so an uncached load here would
/// re-read the whole pack at frame rate. The pack is therefore memoized in
/// the [`OnceCell`] the owning [`OverworldPhase`] carries for exactly this
/// resolver ([`OverworldPhase::connection_pack`]); a *failed* load is not
/// cached (the no-pack case already fails every frame today, and caching
/// the failure would pin a session to it).
///
/// `source` is the owning [`OverworldPhase`]'s own retained
/// [`crate::pack_source::PackSource`] (issue #412), so this per-attempt
/// memoized load honors a headless-real scenario's checkout pin exactly
/// like every other load [`OverworldPhase`] performs after construction --
/// never the runtime resolver regardless of `$POKEEMERALD_PACK`.
pub(super) struct MapConnections<'a> {
    pub(super) pack: &'a OnceCell<assets::pack::AssetPack>,
    pub(super) source: crate::pack_source::PackSource,
}

impl MapConnections<'_> {
    /// The memoized pack, loading it on the first successful call.
    fn pack(&self) -> Option<&assets::pack::AssetPack> {
        if let Some(pack) = self.pack.get() {
            return Some(pack);
        }
        let loaded = self.source.load().ok()?;
        // A racing set cannot happen (single-threaded phase); if the cell
        // were somehow filled between the check and here, the existing
        // value wins, which is equally correct.
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
        let pack = self.pack()?;
        let bytes = pack.layout_map(&name).ok()?;
        let grid = layout.grid(bytes).ok()?;
        grid.cell_at(u16::try_from(x).ok()?, u16::try_from(y).ok()?)
    }
}

/// The maps whose `MAP_SCRIPT_ON_TRANSITION` calls
/// `SecretBase_EventScript_SetDecorationFlags` -- transcribed from those
/// maps' own `scripts.inc`
/// (`data/maps/LittlerootTown_BrendansHouse_2F/scripts.inc:6-12`,
/// `data/maps/LittlerootTown_MaysHouse_2F/scripts.inc:6-12`), restricted to
/// the maps this port bundles. Secret-base maps run the same script via
/// `data/scripts/shared_secret_base.inc:12-16` and are out of scope.
///
/// See [`run_on_transition_map_script`] for what this is for and why it
/// matters for collision.
pub(super) const MAPS_THAT_SET_DECORATION_FLAGS: [assets::MapId; 2] = [
    assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
    assets::MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
];

/// Apply `map`'s `MAP_SCRIPT_ON_TRANSITION` effects to `event_data`, on
/// entering it.
///
/// This port has no script engine, so this is a targeted port of the one
/// on-transition effect that is *observable* for the maps it bundles:
/// `SecretBase_EventScript_SetDecorationFlags`
/// (`data/scripts/secret_base.inc:233-248`), which sets every
/// [`assets::object_event_flags::DECORATION_FLAGS`] id. Same shape as
/// [`crate::new_game`]'s partial port of `EventScript_ResetAllMapFlags` --
/// the effect, without the interpreter.
///
/// # Why this is load-bearing, not cosmetic
///
/// The two player bedrooms declare twelve `OBJ_EVENT_GFX_VAR_*` decoration
/// *placeholders* at staging coordinates (`map.json:32-175` in each), each
/// behind its own `FLAG_DECORATION_*`. Their polarity is inverted from an
/// ordinary `FLAG_HIDE_*`: an empty slot is the *set* state. Nothing sets
/// them at new-game time -- `InitEventData` (`src/event_data.c:32-37`)
/// zeroes the flag array and `EventScript_ResetAllMapFlags` never mentions
/// them -- so without this, a fresh save reads all twelve as *visible*.
///
/// Upstream avoids that purely by ordering: `RunOnTransitionMapScript`
/// (`src/overworld.c:860`, in `LoadMapFromWarp`) runs this script *before*
/// `InitObjectEventsLocal` reaches `TrySpawnObjectEvents`
/// (`src/overworld.c:2163-2178`), whose `!FlagGet(template->flagId)` gate
/// (`src/event_object_movement.c:1670-1672`) then skips all twelve. The
/// occupied slots are re-cleared afterwards, one at a time, by
/// `InitSecretBaseDecorationSprites` (`src/secret_base.c:552-632`) from the
/// `MAP_SCRIPT_ON_WARP_INTO_MAP_TABLE` script -- and on a fresh save
/// `playerRoomDecorations[]` is all `DECOR_NONE` (`ClearSav1`,
/// `src/load_save.c:64-67`), so none are.
///
/// Two consequences if this is skipped, both real:
/// - **Collision.** A spawned placeholder is a hard blocker. Nothing
///   upstream exempts it: `DoesObjectCollideWithObjectAt`
///   (`src/event_object_movement.c:4724-4742`) consults only `active`,
///   coordinates and elevation -- never `invisible` -- so even
///   `MOVEMENT_TYPE_INVISIBLE` would still block (and these use
///   `MOVEMENT_TYPE_LOOK_AROUND` anyway). Seven of Brendan's bedroom's
///   twelve sit on walkable floor down the room's left column, `(1, 2)`
///   among them; all twelve of May's do.
/// - **Rendering.** `GetObjectEventGraphicsInfo`
///   (`src/event_object_movement.c:1914-1931`) resolves
///   `OBJ_EVENT_GFX_VAR_n` through `VAR_OBJ_GFX_ID_0 + n`, which is `0` on a
///   fresh save -- i.e. `OBJ_EVENT_GFX_BRENDAN_NORMAL`. Twelve Brendan
///   clones, not invisible markers.
///
/// # Not ported
///
/// The `ON_WARP_INTO_MAP` half (`InitSecretBaseDecorationSprites`) that
/// *clears* a flag per placed decoration, since this port has no
/// `playerRoomDecorations` save state for anything to be placed in -- a
/// fresh save's slots are all empty, which is exactly the state this
/// produces. A future decoration slice adds that half; it needs no change
/// here, but it **must** model both of that function's writes together --
/// upstream clears `FLAG_DECORATION_n` *and* sets `VAR_OBJ_GFX_ID_0 + n`
/// per placed decoration, and `crate::overworld::npc`'s
/// `OBJ_EVENT_GFX_VAR_0` exception resolves through that var (its module
/// docs carry the hazard). The other on-transition effects of these maps
/// (`VAR_LITTLEROOT_RIVAL_STATE`/`VAR_LITTLEROOT_INTRO_STATE` branches,
/// `setvar VAR_SECRET_BASE_INITIALIZED`) drive story progression this port
/// does not model yet.
///
/// # Panics
///
/// Never in practice: every
/// [`assets::object_event_flags::DECORATION_FLAGS`] id is a transcribed
/// `include/constants/flags.h` literal (`0xAE..=0xBB`) well inside the
/// ordinary flag range `flag_set` accepts -- the same reasoning
/// [`crate::new_game::init_save_blocks`]'s own `RESET_MAP_FLAGS`
/// application rests on, and pinned by this module's
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
    /// Execute a [`engine::overworld::WarpTrigger::Resolved`] warp: load
    /// `map`'s room ([`overworld::load_room`]) and resolve its `warp_id`-th
    /// warp event's arrival position/elevation ([`warp_destination_position`]),
    /// then place `player` there facing whatever the *destination* tile's
    /// own metatile behavior dictates ([`warp_in_facing`] -- upstream
    /// `GetAdjustedInitialDirection`, `pokeemerald/src/overworld.c:929-951`)
    /// and assign `map_id`/`scene` together.
    ///
    /// Those two fields move in lockstep on purpose:
    /// [`crate::overworld::OverworldScene::runtime`] stamps `map_id` onto a
    /// [`engine::overworld::MapRuntime`] built from `scene`'s own decoded
    /// grid/tileset bytes, so updating one without the other would render
    /// one map's layout against another map's collision/warp/event data.
    /// Both are assigned here, after every fallible lookup has already
    /// succeeded, so there is no window in which they disagree.
    ///
    /// Keeps `save1.location` coherent with the new map, mirroring upstream
    /// `SetWarpData`/`ApplyCurrentWarp`
    /// (`pokeemerald/src/overworld.c:554-560, 540-545`): `x`/`y` are left at
    /// `-1` since the player arrives via a resolved warp id, not fixed
    /// coordinates -- the exact shape `SetWarpDestinationToMapWarp`
    /// (`overworld.c:638-641`) passes to `SetWarpDestination`.
    ///
    /// If the destination map's header/events/room data can't be loaded, or
    /// it has no warp event at `warp_id` -- both unreachable against a real
    /// pack for any warp this port's own tables reference -- logs and
    /// leaves the player exactly where they stood before the warp
    /// (module docs' "log-or-ignore is fine" policy), rather than
    /// half-applying the transition.
    ///
    /// # Panics
    ///
    /// If the destination's generated `MAP_GROUP`/`MAP_NUM` index doesn't fit
    /// the `i8` upstream's `struct WarpData` stores it in -- see
    /// [`warp_data_index`], which no real extraction can trip.
    pub(super) fn warp_to(&mut self, map: assets::MapId, warp_id: u8) {
        let Ok(header) = MapHeaderTable::new().header(map) else {
            eprintln!("warp: unknown destination map {map:?} -- staying put");
            return;
        };
        let Ok(events) = MapEventsTable::new().resolve(map) else {
            eprintln!("warp: no event data for destination map {map:?} -- staying put");
            return;
        };
        // `RunOnTransitionMapScript` (`src/overworld.c:860`, in
        // `LoadMapFromWarp`) -- computed on a scratch clone, before the
        // scene decodes and before anything reads the destination map's
        // object events, mirroring upstream's ordering against
        // `TrySpawnObjectEvents`: `route103_rival_trigger::setup_rival_gfx_id_on_transition`
        // decides which sprite Route 103's rival object event resolves to
        // (`crate::overworld::npc`'s own module docs), which the scene
        // decode below needs to already know. Not committed to
        // `self.save1.event_data` until the whole warp is known to succeed
        // (module docs' "leaves the player exactly where they stood"
        // failure contract) -- see the assignment near the end of this
        // method.
        let mut transitioned_event_data = self.save1.event_data.clone();
        // `ClearTempFieldEventData` (`src/overworld.c:848`, in
        // `LoadMapFromWarp`, ahead of `RunOnTransitionMapScript` at `:860`):
        // per-map-load temporary state -- the temp flag/var ranges Route
        // 103's cuttable-tree object events now make load-bearing
        // (`FLAG_TEMP_12`/`_13`, `assets::object_event_flags`) -- never
        // survives into the entered map.
        transitioned_event_data.clear_temp_field_event_data();
        run_on_transition_map_script(map, &mut transitioned_event_data);
        // Route 101's own on-frame `VAR_ROUTE101_STATE` bump (issue #231,
        // `super::first_battle_trigger`'s module docs) -- a no-op unless
        // `map` is Route 101 itself.
        super::first_battle_trigger::sync_route_101_state_on_entry(
            map,
            &mut transitioned_event_data,
        );
        // Route 103's own `VAR_OBJ_GFX_ID_0` rival-sprite setup (issue #248,
        // `super::route103_rival_trigger`'s module docs) -- a no-op unless
        // `map` is Route 103 itself.
        super::route103_rival_trigger::setup_rival_gfx_id_on_transition(
            map,
            &mut transitioned_event_data,
            self.save2.player_gender,
        );

        let Ok(scene) = overworld::load_room_from_source(
            self.pack_source,
            map,
            self.save2.player_gender.into(),
            &transitioned_event_data,
        ) else {
            eprintln!("warp: failed to load destination map {map:?} -- staying put");
            return;
        };
        let destination = {
            let runtime = scene.runtime(map, header, events);
            warp_destination_position(&runtime, warp_id).map(|(x, y, elevation)| {
                // GetCenterScreenMetatileBehavior (overworld.c:954-957) reads
                // the tile the player has just been placed on. An
                // undecodable attribute entry can't happen for a cell
                // `warp_destination_position` just resolved, but falling back
                // to MB_NORMAL keeps that case on `GetAdjustedInitialDirection`'s
                // own final-else path rather than inventing a facing.
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
        // The departed map's latched landing tile must not survive into the
        // destination map, where its coordinates would name an unrelated
        // tile in the next frame's door check.
        self.pending_landing = None;
        self.scene = scene;
        self.map_id = map;
        // Upstream's own `InitTilesetAnimations` reset (struct docs on
        // `tick`): the destination map's animated tiles start over from
        // their own tick 0, not wherever the departed map's counter was.
        self.tick = 0;
        self.save1.event_data = transitioned_event_data;
        // `RestartWildEncounterImmunitySteps` (`LoadMapFromWarp`,
        // `src/overworld.c:850`): the first four steps on the destination
        // map roll nothing, so stepping out of a door never drops the
        // player straight into a battle (issue #169).
        self.wild.restart_immunity_steps();
        self.save1.location = WarpData {
            map_group: warp_data_index(header.group, "MAP_GROUP"),
            map_num: warp_data_index(header.num, "MAP_NUM"),
            warp_id: warp_data_index(warp_id, "warp id"),
            x: -1,
            y: -1,
        };
    }

    /// Execute an *explicit-coordinate* warp: land on `(x, y)` of `map`
    /// directly, rather than resolving a warp event's own position the way
    /// [`OverworldPhase::warp_to`] does. [`OverworldPhase::warp_to`]'s
    /// sibling for the two callers with no warp event to resolve at all --
    /// the white-out's `SetWarpDestinationToLastHealLocation` +
    /// `WarpIntoMap` (`pokeemerald/src/overworld.c:364-365`,
    /// `crate::flow::overworld_phase::white_out`, issue #261) and the
    /// scripted first-battle conclusion's own
    /// `warp MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5`
    /// (`crate::flow::overworld_phase::first_battle_conclusion`) — mirroring
    /// `SetPlayerCoordsFromWarp`'s own `WARP_ID_NONE` branch
    /// (`src/overworld.c:611-617`, "the given coords are valid, use those
    /// instead"): a heal location, like a `warp` command's own literal
    /// coordinates, names a raw tile, not a warp event index.
    ///
    /// Same shape as [`OverworldPhase::warp_to`] otherwise -- on-transition
    /// effects, temp-field-data clear, the two Route 101/103 targeted
    /// effects, atomic scene/`map_id` rebind, `tick` reset,
    /// `RestartWildEncounterImmunitySteps` -- and the same "leaves the
    /// player exactly where they stood" failure contract if `map`'s
    /// header/events/room can't be resolved, or `(x, y)` is outside the
    /// destination's decoded grid.
    ///
    /// Lands at the destination tile's own elevation, exactly as
    /// [`OverworldPhase::warp_to`]'s resolved-warp landing does. Upstream
    /// does not leave the wildcard sentinel in place until the player
    /// moves: `InitObjectEventStateFromTemplate` sets a freshly spawned
    /// object event's `triggerGroundEffectsOnMove = TRUE`
    /// (`pokeemerald/src/event_object_movement.c:1301`), so the very next
    /// `UpdateObjectEventCurrentMovement` call -- on the spawn frame
    /// itself, during the warp fade and before input unlocks -- runs
    /// `DoGroundEffects_OnSpawn` (`event_object_movement.c:4931`), which
    /// calls `UpdateObjectEventElevationAndPriority`
    /// (`event_object_movement.c:7737`), which calls
    /// `ObjectEventUpdateElevation` (`event_object_movement.c:7759-7771`)
    /// to read the landing tile's real elevation off the destination grid
    /// and overwrite the sentinel before the player ever takes a step. Both
    /// of this method's own callers' destinations reach this: the white-out's
    /// heal-location relocation to the player's house 2F at its bed tile `(4, 2)`
    /// (`crate::flow::overworld_phase::white_out`,
    /// [`crate::new_game::default_last_heal_location`]) and the scripted
    /// first-battle conclusion's return to Birch's lab at `(6, 5)`
    /// (`crate::flow::overworld_phase::first_battle_conclusion`) land
    /// on elevation-`3` tiles, so leaving the sentinel in place would be
    /// wrong until the player's first step, not merely imprecise.
    ///
    /// `warp_to_position` has no warp event to hand a `warp_id`, so it can't
    /// call [`warp_destination_position`] directly -- instead it reads the
    /// destination cell itself, through the same
    /// [`engine::overworld::MapRuntime::arrival_elevation`] helper
    /// `warp_destination_position` and [`super::placement::saved_tile_placement`]
    /// both call, including its multi-level-to-transition substitution
    /// (issue #379: one read shared by all three placement paths).
    ///
    /// Unlike [`OverworldPhase::warp_to`]'s resolved-warp landing,
    /// `save1.location.x`/`.y` are **not** `-1`: `ApplyCurrentWarp`
    /// (`overworld.c:540-546`) copies `sWarpDestination` verbatim, and
    /// `SetWarpDestinationToLastHealLocation` (`overworld.c:665-668`) sets
    /// that to `gSaveBlock1Ptr->lastHealLocation` as-is -- a real `(x, y)`
    /// pair, not the `WARP_ID_NONE`-plus-sentinel-coords shape a resolved
    /// warp event leaves behind.
    ///
    /// # Panics
    ///
    /// Same as [`OverworldPhase::warp_to`]: if the destination's generated
    /// `MAP_GROUP`/`MAP_NUM` index doesn't fit the `i8` upstream's `struct
    /// WarpData` stores it in ([`warp_data_index`]).
    pub(super) fn warp_to_position(&mut self, map: assets::MapId, x: i16, y: i16) {
        let Ok(header) = MapHeaderTable::new().header(map) else {
            eprintln!("warp: unknown destination map {map:?} -- staying put");
            return;
        };
        let Ok(events) = MapEventsTable::new().resolve(map) else {
            eprintln!("warp: no event data for destination map {map:?} -- staying put");
            return;
        };
        let mut transitioned_event_data = self.save1.event_data.clone();
        transitioned_event_data.clear_temp_field_event_data();
        run_on_transition_map_script(map, &mut transitioned_event_data);
        super::first_battle_trigger::sync_route_101_state_on_entry(
            map,
            &mut transitioned_event_data,
        );
        super::route103_rival_trigger::setup_rival_gfx_id_on_transition(
            map,
            &mut transitioned_event_data,
            self.save2.player_gender,
        );

        let Ok(scene) = overworld::load_room_from_source(
            self.pack_source,
            map,
            self.save2.player_gender.into(),
            &transitioned_event_data,
        ) else {
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

    /// Rebind `map_id`/`scene`/`save1.location` (issue #177) after
    /// `self.player` has already stepped across a map-edge connection into
    /// `to_map`'s own coordinate space -- [`engine::overworld::StepOutcome::Crossed`],
    /// resolved against [`MapConnections`] by
    /// [`super::input::advance_player_one_frame`]'s call into
    /// [`engine::overworld::PlayerState::step`], and deferred to here by
    /// [`OverworldPhase::step`] (its own "Map-edge connection crossing"
    /// comment) so this mutable borrow of `self.scene` never overlaps the
    /// frame's still-live `runtime`.
    ///
    /// **Same atomic-rebind discipline as [`OverworldPhase::warp_to`], one
    /// field different.** `map_id` and `scene` move together, exactly as
    /// there -- so a later frame's `self.scene.runtime(self.map_id, ...)`
    /// never renders one map's layout against another map's collision/event
    /// data -- `pending_landing` is re-latched onto `to_position` so the
    /// door-warp drain-frame check (`OverworldPhase::step`'s "Warp timing"
    /// section) evaluates against the *entered* map once this step's walk
    /// animation finishes, and `tick` keeps running -- upstream's
    /// `LoadMapFromCameraTransition` re-inits only the secondary tileset
    /// counter (`InitSecondaryTilesetAnimation`, `overworld.c:815`), never
    /// the primary one `tick` models (see the body comment). Unlike
    /// [`OverworldPhase::warp_to`], `self.player` itself is left entirely
    /// alone: [`engine::overworld::PlayerState::step`] already committed its
    /// position/elevation into `to_map`'s coordinate space before ever
    /// returning [`engine::overworld::StepOutcome::Crossed`] (that variant's
    /// own doc comment) -- rebuilding a fresh
    /// [`engine::overworld::PlayerState`] here, the way a warp does, would
    /// discard the facing and in-progress walk animation an ordinary step
    /// must keep.
    ///
    /// `save1.location` mirrors upstream's own connection-crossing
    /// bookkeeping, not a warp's: `CameraMove` (`pokeemerald/src/fieldmap.c:603-624`)
    /// calls `LoadMapFromCameraTransition(connection->mapGroup,
    /// connection->mapNum)` (`src/overworld.c:784-786`), which itself calls
    /// `SetWarpDestination(mapGroup, mapNum, WARP_ID_NONE, -1, -1)`
    /// (`:633`) then `ApplyCurrentWarp`
    /// (`:540-546`, `gSaveBlock1Ptr->location = sWarpDestination`) --
    /// `warp_id` is the `WARP_ID_NONE` sentinel (`-1`), not a resolved warp
    /// index, because a connection crossing names only the destination map,
    /// never a warp event; `x`/`y` stay `-1` for the same reason
    /// [`OverworldPhase::warp_to`]'s own do (a resolved landing tile, not
    /// fixed coordinates -- here, the tile
    /// [`engine::overworld::PlayerState::step`] already computed via
    /// [`MapConnections`], rather than a warp id).
    ///
    /// If `to_map`'s header can't be resolved or its room can't be loaded --
    /// both unreachable against a real pack for any connection this port's
    /// own generated tables reference, since [`MapConnections`] already
    /// proved both resolvable before [`engine::overworld::PlayerState::step`]
    /// ever committed the crossing -- logs, touches nothing, and returns
    /// `false` so the caller can restore the pre-step stance
    /// ([`OverworldPhase::step`]'s crossing branch does exactly that): the
    /// player stays put on the departed map, the same "leaves the player
    /// exactly where they stood" contract [`OverworldPhase::warp_to`]
    /// documents for its own unreachable failure cases. Returns `true` on a
    /// completed rebind.
    pub(super) fn cross_connection(&mut self, to_map: assets::MapId, to_position: TilePos) -> bool {
        let Ok(header) = MapHeaderTable::new().header(to_map) else {
            eprintln!(
                "connection: unknown destination map {to_map:?} -- staying on the departed \
                 map's data"
            );
            return false;
        };
        // Same "compute on a scratch clone, commit only on success" shape
        // as `Self::warp_to` (that method's own doc comment): the scene
        // decode below needs to see the entered map's on-transition effects
        // -- Route 103's rival-sprite setup among them -- before it ever
        // runs.
        let mut transitioned_event_data = self.save1.event_data.clone();
        // `ClearTempFieldEventData` (`src/overworld.c:798`, in
        // `LoadMapFromCameraTransition` -- upstream's connection-crossing
        // load path clears per-map-load temporary state exactly like the
        // warp path does, ahead of `RunOnTransitionMapScript` at `:807`):
        // Route 103's cuttable-tree object events make the temp flag range
        // load-bearing (`FLAG_TEMP_12`/`_13`,
        // `assets::object_event_flags`), so a stale temp flag must not keep
        // a tree hidden across a re-entry.
        transitioned_event_data.clear_temp_field_event_data();
        run_on_transition_map_script(to_map, &mut transitioned_event_data);
        // Route 101's own on-frame `VAR_ROUTE101_STATE` bump (issue #231,
        // `super::first_battle_trigger`'s module docs) -- a no-op unless
        // `to_map` is Route 101 itself.
        super::first_battle_trigger::sync_route_101_state_on_entry(
            to_map,
            &mut transitioned_event_data,
        );
        // Route 103's own `VAR_OBJ_GFX_ID_0` rival-sprite setup (issue #248,
        // `super::route103_rival_trigger`'s module docs) -- a no-op unless
        // `to_map` is Route 103 itself. This is in fact the one entry point
        // this port's own connection chain (Littleroot<->Route101<->Oldale<->Route103)
        // ever reaches Route 103 through.
        super::route103_rival_trigger::setup_rival_gfx_id_on_transition(
            to_map,
            &mut transitioned_event_data,
            self.save2.player_gender,
        );

        let Ok(scene) = overworld::load_room_from_source(
            self.pack_source,
            to_map,
            self.save2.player_gender.into(),
            &transitioned_event_data,
        ) else {
            eprintln!(
                "connection: failed to load destination map {to_map:?} -- staying on the \
                 departed map's data"
            );
            return false;
        };

        self.scene = scene;
        self.map_id = to_map;
        // Re-latch onto the entered map's own coordinate space (doc comment
        // above) -- the crossing step's landing tile, in the same role
        // `OverworldPhase::step`'s ordinary `Advanced` branch already
        // latches for a door check 16 frames from now, once the walk
        // animation drains.
        self.pending_landing = Some(to_position);
        // Deliberately no `self.tick = 0` here: `LoadMapFromCameraTransition`
        // (`src/overworld.c:784-825`) never calls `InitTilesetAnimations` --
        // it calls `InitSecondaryTilesetAnimation` (`:815`), which resets
        // only `sSecondaryTilesetAnimCounter` (`tileset_anims.c:581-583`)
        // and leaves the primary counter running. `tick` models the
        // *primary* counter (the only one this port animates -- see
        // `crate::overworld::tileset_anims`), so the faithful counterpart
        // of that secondary-only re-init is a no-op: the shared
        // water/flower animation continues uninterrupted across a seamless
        // crossing, unlike a warp's full map load.
        self.save1.event_data = transitioned_event_data;
        // `RestartWildEncounterImmunitySteps` (`LoadMapFromCameraTransition`,
        // `src/overworld.c:800`) -- the piece of that function issue #177
        // deferred to this slice. Crossing Littleroot's north edge into
        // Route 101 therefore buys four encounter-free steps before the
        // grass can roll (issue #169).
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

/// Narrow a generated map-table index (`MAP_GROUP`, `MAP_NUM`, or a warp
/// event index) into the `i8` upstream's `struct WarpData`
/// (`include/global.h`, transcribed as [`WarpData`]) stores it in.
///
/// # Panics
///
/// If `value` exceeds `i8::MAX`. Unreachable against any real extraction:
/// upstream declares all three fields `s8`, and the generated
/// [`MapHeaderTable`] tops out at 34 map groups of at most 108 maps each --
/// same "the constants are cross-checked against the generated table"
/// reasoning [`crate::new_game::SPAWN_MAP_GROUP`]/[`crate::new_game::SPAWN_MAP_NUM`]
/// rest on. Panicking (rather than saturating to a fabricated `127`, which
/// would silently write a *different, real* map's group/num into the save)
/// is the honest failure mode if a future extraction ever breaks that
/// assumption.
pub(super) fn warp_data_index(value: u8, what: &str) -> i8 {
    i8::try_from(value).unwrap_or_else(|_| {
        panic!("{what} {value} does not fit the i8 upstream's struct WarpData stores it in")
    })
}
