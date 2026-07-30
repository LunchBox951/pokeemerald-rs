//! Unit tests for [`super::OverworldPhase`] and its private helpers.

use super::{advance_player_one_frame, held_direction, warp_data_index, OverworldPhase};
use crate::flow::tests::held;
use crate::new_game;
use assets::{MapEvents, MapHeader, MapId, MapLayout, MetatileCell};
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_NON_ANIMATED_DOOR, MB_SOUTH_ARROW_WARP,
};
use engine::overworld::{warp_in_facing, Direction, MapRuntime, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// A single freshly-pressed button this frame (`is_newly_pressed` true,
/// unlike `crate::flow::tests::held`'s deliberately *not*-fresh two-frame
/// hold) -- mirrors that module's own private `pressed` helper, needed
/// here too for the A-button edges the NPC dialog tests below drive.
fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// The `((x, y), metatile behavior)` of `map`'s `warp_index`-th warp
/// event's own tile, read out of the extracted pack -- so the
/// warp-facing tests below assert against the real attribute data
/// `OverworldPhase::warp_to` reads at runtime, not a restatement of
/// their own expectations. Pack-dependent: `#[ignore]`d callers only.
fn warp_tile_behavior(map: assets::MapId, warp_index: usize) -> ((i16, i16), u8) {
    let scene = crate::overworld::load_room(map).expect("run `cargo xtask extract` first");
    let header = assets::MapHeaderTable::new()
        .header(map)
        .expect("map must resolve in the generated map-header table");
    let events = assets::MapEventsTable::new()
        .resolve(map)
        .expect("map must resolve in the generated map-events table");
    let warp = events.warp_events[warp_index];
    let runtime = scene.runtime(map, header, events);
    let behavior = runtime
        .metatile_behavior(i32::from(warp.x), i32::from(warp.y))
        .expect("a warp event's own tile must decode");
    ((warp.x, warp.y), behavior)
}

/// A small, open (no collision anywhere), leaked-`'static` flat map --
/// mirrors `engine::overworld::player::tests::flat_runtime` (that
/// module's own fixture, private to its crate) so
/// [`advance_player_one_frame`] is testable against a real
/// [`MapRuntime`] without needing a local asset pack (`OverworldScene`,
/// unlike `MapRuntime`, is pack-backed -- see [`OverworldPhase`]'s own
/// pack-dependent, `#[ignore]`d tests below).
fn flat_runtime(width: u16, height: u16) -> MapRuntime<'static> {
    let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
    for _ in 0..width * height {
        let raw = MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        }
        .pack();
        bytes.extend_from_slice(&raw.to_le_bytes());
    }
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());

    let header: &'static MapHeader = Box::leak(Box::new(MapHeader {
        id: MapId("MAP_TEST"),
        group: 0,
        num: 0,
        name: "MapTest",
        layout: assets::LayoutId("MAP_TEST"),
        music: assets::MusicId(0),
        region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
        requires_flash: false,
        weather: assets::Weather::None,
        map_type: assets::MapType::Route,
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: false,
        battle_scene: assets::BattleScene::Normal,
        connections: &[],
    }));
    let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
        id: MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: &[],
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));

    let layout: &'static MapLayout = Box::leak(Box::new(MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width,
        height,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    }));
    let grid = layout.grid(bytes).unwrap();

    MapRuntime::new(
        MapId("MAP_TEST"),
        header,
        events,
        grid,
        assets::MetatileAttributeTable::new(&[]),
        assets::MetatileAttributeTable::new(&[]),
    )
}

/// Senior review round 3 regression, correcting the prior (empirically
/// wrong -- see [`advance_player_one_frame`]'s own doc comment) "skip
/// the first tick" change: the first frame composed after a step begins
/// must see `step_progress() == 1`, not `0` -- upstream applies the
/// first walk-animation frame in the very call that starts the step
/// (`MovementAction_WalkNormalDown_Step0`'s `InitMovementNormal`
/// immediately followed by `Step1` -> `UpdateMovementNormal` ->
/// `NpcTakeStep`, `pokeemerald/src/event_object_movement.c:5354-5358`)
/// -- and a full tile crossing takes exactly
/// [`engine::overworld::WALK_FRAMES_PER_TILE`] (16) rendered frames.
#[test]
fn advance_player_one_frame_shows_progress_1_on_the_frame_a_step_begins_and_takes_16_frames_per_tile(
) {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);

    // Facing South already; a held South poll steps immediately (no
    // turn-in-place first, since the direction already matches facing).
    advance_player_one_frame(&mut player, Some(Direction::South), &runtime);
    assert_eq!(player.position(), (2, 3), "the step must have landed");
    assert!(player.in_transit());
    assert_eq!(
        player.step_progress(),
        1,
        "the frame that just started a step is already 1px into the \
         walk animation, matching upstream's InitMovementNormal-then- \
         immediately-Step1 shape"
    );

    // Every following frame advances the timer by exactly 1 while the
    // input stays held.
    for expected in 2..engine::overworld::WALK_FRAMES_PER_TILE {
        advance_player_one_frame(&mut player, Some(Direction::South), &runtime);
        assert_eq!(player.step_progress(), expected);
    }
    assert!(
        player.in_transit(),
        "still mid-transit one frame before settling"
    );

    // The 16th frame (this crossing's `WALK_FRAMES_PER_TILE`th) is the
    // one where the transit settles -- 16 rendered frames total to
    // cross one tile, not 17.
    advance_player_one_frame(&mut player, Some(Direction::South), &runtime);
    assert!(
        !player.in_transit(),
        "the transit must settle on exactly the 16th frame"
    );
}

/// A turn-in-place never enters transit -- `PlayerState::tick` is a
/// documented no-op while `transit_frames` is `None`
/// (`PlayerState::tick`'s own doc comment), so the unconditional tick
/// [`advance_player_one_frame`] always runs afterward must not somehow
/// start (or otherwise disturb) a transit a plain turn never begins.
#[test]
fn advance_player_one_frame_turning_in_place_never_enters_transit() {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);

    advance_player_one_frame(&mut player, Some(Direction::East), &runtime);
    assert_eq!(player.facing(), Direction::East, "must have turned");
    assert_eq!(player.position(), (2, 2), "a turn must not move the tile");
    assert!(!player.in_transit());
    assert_eq!(player.step_progress(), 0);
}

#[test]
fn held_direction_prioritizes_up_over_every_other_direction() {
    // field_control_avatar.c's own if/else-if chain order (see
    // `held_direction`'s doc comment): up beats every simultaneous
    // combination.
    assert_eq!(
        held_direction(held(
            Buttons::UP | Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT
        )),
        Some(Direction::North)
    );
    assert_eq!(
        held_direction(held(Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT)),
        Some(Direction::South)
    );
    assert_eq!(
        held_direction(held(Buttons::LEFT | Buttons::RIGHT)),
        Some(Direction::West)
    );
    assert_eq!(held_direction(held(Buttons::RIGHT)), Some(Direction::East));
    assert_eq!(held_direction(ButtonState::new()), None);
}

/// I-3 scene-flow test: once in the overworld, a held direction is fed
/// to the player every frame -- "the player movable" (issue #149's own
/// scope item 4). A turn always succeeds regardless of the room's
/// collision layout (only a *step* can be blocked), so this is a safe
/// assertion without depending on the real map's exact geometry.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn overworld_movement_input_turns_the_player() {
    // `OverworldPhase::load_default` itself (not a hand-built struct
    // literal) so this also exercises the save-state wiring (finding
    // 1) the same way production reaches this state.
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "starts facing south"
    );

    phase.step(held(Buttons::UP));

    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a fresh directional input first turns the player to face it"
    );

    // The retained save state mirrors the logical tile after every step
    // (upstream keeps `gSaveBlock1Ptr->pos` current as the player moves).
    // Walk south far enough to guarantee at least one accepted step in
    // the open room, then assert the mirror holds wherever we ended up.
    for _ in 0..40 {
        phase.step(held(Buttons::DOWN));
    }
    let (x, y) = phase.player.position();
    assert_eq!(
        (
            i32::from(phase.save1().pos.x),
            i32::from(phase.save1().pos.y)
        ),
        (x, y),
        "save1.pos must track the player's logical tile, not the spawn"
    );
    assert_ne!(
        (x, y),
        new_game::SPAWN_POSITION,
        "walking south from the spawn must actually move the player"
    );
}

/// Flow-level test (the issue #163 acceptance test): stepping onto the bedroom's stair
/// warp tile at `(7, 1)` from below transitions the phase to
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`, landing the player exactly at
/// that map's own warp-event #2 arrival position -- `crate::new_game`'s
/// module docs trace this exact warp chain (`(7, 1)` on 2F ->
/// `dest_warp_id: 2` on 1F -> `(8, 2)`, `warp.rs`'s
/// `warp_destination_position`) -- facing whatever that destination
/// tile's own behavior dictates (`engine::overworld::warp_in_facing`),
/// with `save1.location`/`save1.pos` kept coherent with the new map
/// ([`OverworldPhase::warp_to`]'s own doc comment).
///
/// The transition is also asserted to happen on the frame the step
/// *finishes*, not the frame it starts: upstream gates
/// `TryStartWarpEventScript` on `input->tookStep`, set only at
/// `T_TILE_CENTER` while `runningState == MOVING`
/// (`pokeemerald/src/field_control_avatar.c:117-119, 155-161`). Here that
/// is [`WALK_FRAMES_PER_TILE`] (16) frames after the step began
/// ([`OverworldPhase::step`]'s "Warp timing" section).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let bedroom = phase.map_id;

    // `new_game::SPAWN_POSITION` is the warp tile itself (module docs),
    // so start one tile south of it instead and step north onto it --
    // "stepping onto (7, 1) from below" (DoD).
    phase.player = PlayerState::new((7, 2), new_game::SPAWN_ELEVATION, Direction::North);

    // Frame 1: the step onto (7, 1) begins. `PlayerState` commits the
    // tile immediately, but the warp must not fire yet.
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        new_game::SPAWN_POSITION,
        "the step commits the landing tile on the frame it begins"
    );
    assert_eq!(
        phase.map_id, bedroom,
        "the warp must not fire on the frame the step begins"
    );

    // Frames 2..=15: drain the walk animation with no input held. The
    // map must stay put for every one of them.
    for frame in 2..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, bedroom,
            "the warp must not fire mid-animation (frame {frame} of \
             {WALK_FRAMES_PER_TILE})"
        );
        assert!(
            phase.player.in_transit(),
            "the walk animation must still be draining on frame {frame}"
        );
    }

    // Frame 16: `PlayerState::tick` drains the animation -- upstream's
    // `tookStep` frame, and the one the warp fires on.
    phase.step(ButtonState::new());

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(
        phase.map_id, destination,
        "the completed step onto the stair warp must rebind to the 1F map \
         on the 16th frame"
    );
    assert_eq!(
        phase.player.position(),
        (8, 2),
        "the player must arrive at 1F's own warp #2 position"
    );
    // The facing is derived from the *destination* tile's own behavior,
    // so pin that behavior down too -- otherwise `South` here would also
    // be satisfied by `GetAdjustedInitialDirection`'s catch-all `else`.
    let (dest_pos, dest_behavior) = warp_tile_behavior(destination, 2);
    assert_eq!(dest_pos, (8, 2));
    assert_eq!(
        dest_behavior, MB_NON_ANIMATED_DOOR,
        "1F's warp #2 is the staircase's own non-animated-door tile"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "GetAdjustedInitialDirection's IsNonAnimDoor||IsDoor branch \
         (overworld.c:935-936) applies to that tile"
    );

    let dest_header = assets::MapHeaderTable::new()
        .header(destination)
        .expect("1F must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(dest_header.group).unwrap()
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(dest_header.num).unwrap()
    );
    assert_eq!(
        phase.save1().location.warp_id,
        2,
        "arrived via 1F's own warp-event index 2 (new_game module docs)"
    );
    assert_eq!(
        (phase.save1().location.x, phase.save1().location.y),
        (-1, -1),
        "SetWarpDestinationToMapWarp always passes -1, -1 for x/y (overworld.c:638-641)"
    );
    assert_eq!(
        (
            i32::from(phase.save1().pos.x),
            i32::from(phase.save1().pos.y)
        ),
        (8, 2),
        "save1.pos must mirror the post-warp tile, not the pre-warp one"
    );
}

/// Regression (the issue #163 acceptance test): a completed landing on an *ordinary*
/// (non-warp) tile must not transition the map, even though every
/// completed landing is now checked for a warp trigger.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_an_ordinary_tile_does_not_warp() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let starting_map = phase.map_id;

    // The spawn tile IS the warp tile; step south, away from it, onto
    // ordinary bedroom floor (already exercised, collision-wise, by
    // `overworld_movement_input_turns_the_player`). Drive the whole
    // 16-frame walk animation, since the trigger check only runs on the
    // frame it drains (`OverworldPhase::step`'s "Warp timing" section).
    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }

    assert_eq!(
        phase.map_id, starting_map,
        "stepping onto an ordinary floor tile must not transition maps"
    );
    assert_eq!(
        phase.player.position(),
        (7, 2),
        "the step itself must still have landed"
    );
    assert!(
        !phase.player.in_transit(),
        "16 frames must fully drain the step this test relies on completing"
    );
}

/// [`warp_data_index`] narrows the *generated* tables' real indices
/// (no pack needed -- `MapHeaderTable` is compiled in), cross-checked
/// against [`new_game`]'s own hand-maintained constants the same way
/// `new_game`'s `spawn_location_matches_the_generated_map_header` does.
#[test]
fn warp_data_index_narrows_the_generated_map_indices() {
    let header = assets::MapHeaderTable::new()
        .header(new_game::SPAWN_MAP_ID)
        .expect("SPAWN_MAP_ID must resolve in the generated map-header table");
    assert_eq!(
        warp_data_index(header.group, "MAP_GROUP"),
        new_game::SPAWN_MAP_GROUP
    );
    assert_eq!(
        warp_data_index(header.num, "MAP_NUM"),
        new_game::SPAWN_MAP_NUM
    );
    assert_eq!(warp_data_index(0, "warp id"), 0);
    assert_eq!(warp_data_index(127, "warp id"), 127);
}

/// The out-of-range case panics rather than fabricating a plausible
/// index: saturating to `127` would have silently written a *different,
/// real* map's group/num into the save (that function's own doc).
#[test]
#[should_panic(expected = "does not fit the i8")]
fn warp_data_index_refuses_to_fabricate_an_out_of_range_index() {
    let _ = warp_data_index(128, "MAP_GROUP");
}

/// Real-pack guard for the *destination*-tile rule
/// [`OverworldPhase::warp_to`] derives its arrival facing from
/// (`engine::overworld::warp_in_facing` <- upstream
/// `GetAdjustedInitialDirection`, `pokeemerald/src/overworld.c:929-951`).
///
/// The I-3 path's own counterexample to a source-tile rule: Brendan's
/// house front door is `MAP_LITTLEROOT_TOWN`'s warp #1, sitting on an
/// `MB_ANIMATED_DOOR` tile -- whose *own* branch would say `DIR_SOUTH` --
/// but it lands on `..._BRENDANS_HOUSE_1F`'s warp #1, whose tile is
/// `MB_SOUTH_ARROW_WARP`, so upstream faces the arrival `DIR_NORTH`
/// (back into the house). Asserted against the extracted pack's real
/// metatile attributes, not a hand-built fixture.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_front_door_warp_faces_north_from_the_destination_tiles_behavior() {
    let (source_pos, source_behavior) = warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN"), 1);
    assert_eq!(source_pos, (5, 8), "Littleroot's warp #1: the house door");
    assert_eq!(source_behavior, MB_ANIMATED_DOOR);
    assert_eq!(
        warp_in_facing(source_behavior),
        Direction::South,
        "what a (wrong) source-tile rule would have produced"
    );

    let (dest_pos, dest_behavior) =
        warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);
    assert_eq!(dest_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(dest_behavior, MB_SOUTH_ARROW_WARP);
    assert_eq!(
        warp_in_facing(dest_behavior),
        Direction::North,
        "GetAdjustedInitialDirection's IsSouthArrowWarp branch (overworld.c:937-938)"
    );
}

/// Mutation guard for [`OverworldPhase::warp_to`] itself: the front-door
/// arrival from the test above, driven through `warp_to` rather than the
/// pure `warp_in_facing`. Landing on 1F's warp #1 (`(8, 8)`,
/// `MB_SOUTH_ARROW_WARP`) must face North — a facing distinguishable from
/// both `GetAdjustedInitialDirection`'s catch-all `else` and `warp_to`'s
/// own `MB_NORMAL` fallback (which would each say South, and which the
/// bedroom-stair test cannot tell apart) — and `scene` must rebind in
/// lockstep with `map_id` (`warp_to`'s documented invariant), observed by
/// composing the phase's frame against a freshly loaded 1F scene.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_to_the_front_doormat_faces_north_and_rebinds_the_scene() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let one_f = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

    phase.warp_to(one_f, 1);

    assert_eq!(phase.map_id, one_f);
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "1F's warp #1: the doormat inside the front door"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "the doormat's own MB_SOUTH_ARROW_WARP behavior must drive the \
         facing (overworld.c:937-938) -- South would mean the destination \
         behavior was never read"
    );
    let fresh = crate::overworld::load_room(one_f).expect("1F must load from the extracted pack");
    assert!(
        phase.compose_frame()[..]
            == fresh.compose_frame(&phase.player, &phase.save1.event_data)[..],
        "warp_to must rebind `scene` to the destination map, not just `map_id`"
    );
}

/// The issue #161 acceptance test: spawn (post-#158 intro handoff) ->
/// walk down the stairs to Brendan's House 1F (the real #163 warp path,
/// already pinned by [`stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map`])
/// -> face Mom (the real, pack-loaded `OBJ_EVENT_GFX_MOM` object event at
/// `(2, 6)`, script `PlayersHouse_1F_EventScript_Mom`) -> A opens her
/// dialog with the real upstream text (`crate::overworld::npc_scripts::script_text`'s
/// own transcription of `PlayersHouse_1F_Text_IsntItNiceInHere`) -> A
/// confirms through the trailing prompt -> the dialog closes and control
/// returns cleanly to ordinary overworld movement.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert!(phase.dialog.is_none(), "no dialog is open at spawn");

    // Trigger the already-pinned 2F -> 1F stair warp: the player spawns
    // standing exactly on the warp tile (`new_game::SPAWN_POSITION`), so
    // step away from it and back onto it to generate a fresh
    // `StepOutcome::Advanced` landing (mirrors this module's own
    // `stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map`).
    let bedroom = phase.map_id;
    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.map_id, bedroom, "still upstairs after stepping south");
    phase.step(held(Buttons::UP));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    let one_f = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(
        phase.map_id, one_f,
        "stepping back onto the stair warp must land on 1F"
    );

    // Walk over to Mom and face her: she stands at (2, 6)
    // (`OBJ_EVENT_GFX_MOM`'s real map.json position); this test directly
    // places the player on the adjacent tile facing her rather than
    // simulating every intervening step across the room's own furniture
    // layout -- ordinary walking/collision is already covered by
    // `engine::overworld::player`'s own tests, and the front-door/stair
    // warp path is covered above and by this module's other real-pack
    // tests. What this test alone proves is the interaction + dialog
    // wiring against the *real* extracted Mom object.
    phase.player = PlayerState::new((2, 7), 3, Direction::North);

    // Press A: must find Mom, recognize her script, and open a dialog.
    phase.step(pressed(Buttons::A));
    assert!(
        phase.dialog.is_some(),
        "a fresh A-press facing Mom must open her dialog"
    );
    assert_eq!(
        phase.map_id, one_f,
        "opening a dialog must not itself change the room"
    );

    // While the dialog is open, movement input is frozen (module docs'
    // "NPC dialog routing" section): a held direction must not move the
    // player.
    let position_before_printing = phase.player.position();
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        position_before_printing,
        "movement must be frozen while a dialog is open"
    );

    // Drive the dialog to completion: print every glyph of the real
    // upstream text (`Mid` speed -- confirm not held, since only the
    // trailing prompt needs one), then confirm through the trailing
    // prompt, then let it close. Generous frame budgets throughout: the
    // exact per-glyph cadence is `engine::text::render::Printer`'s own,
    // already pinned by that module's tests -- this test only cares that
    // the *dialog* (not the printer internals) reaches each milestone.
    let expected_tokens =
        crate::overworld::npc_scripts::script_text("PlayersHouse_1F_EventScript_Mom")
            .expect("Mom's script must be recognized against the real map data");
    let expected_glyph_count = expected_tokens
        .iter()
        .filter(|t| matches!(t, engine::text::Token::Char(_)))
        .count();
    assert!(
        expected_glyph_count > 0,
        "the real upstream message must contain visible text"
    );

    let mut fully_printed = false;
    for _ in 0..400 {
        phase.step(ButtonState::new());
        let Some(dialog) = &phase.dialog else {
            panic!("the dialog must not close on its own before the trailing prompt confirms");
        };
        if dialog.revealed_glyph_count() == expected_glyph_count {
            fully_printed = true;
            break;
        }
    }
    assert!(
        fully_printed,
        "every glyph of the real upstream text must print within the frame budget"
    );

    // Confirm through the trailing prompt, then let the box finish
    // closing (module docs on `NpcDialog::tick`'s `Cleared` -> `Closed`
    // gap: a few post-clear reveal-delay frames drain first). Pressing A
    // fresh every frame (rather than once) is deliberate and still
    // exactly matches a single real button press's effect:
    // `engine::text::render::Printer::tick` only ever consults
    // `confirm_pressed` while awaiting the trailing prompt -- the exact
    // frame that's true on is otherwise timing-sensitive (the printer
    // must first finish draining the last glyph's own reveal delay
    // before it even reaches `AwaitingClear`), so this holds the
    // "button" down across that whole window instead of guessing the
    // one frame a single press would need to land on; the loop stops
    // the instant the dialog closes, so no press after that could
    // re-open a new one against Mom, still facing.
    let mut closed = false;
    for _ in 0..30 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(
        closed,
        "confirming through the trailing prompt must close the dialog"
    );

    // Control returns cleanly: ordinary movement input works again.
    // `phase.player` is still facing North (from facing Mom above), so
    // the first held-Down press only turns it to face South (a turn
    // never moves the tile -- `advance_player_one_frame`'s own doc
    // comment); the second commits the step immediately.
    assert_eq!(phase.player.facing(), Direction::North);
    phase.step(held(Buttons::DOWN));
    assert_eq!(phase.player.facing(), Direction::South, "must turn first");
    assert_eq!(
        phase.player.position(),
        (2, 7),
        "a turn must not move the tile"
    );
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (2, 8),
        "movement must resume normally once the dialog has closed"
    );
}
