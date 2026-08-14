//! Unit tests for [`super::OverworldPhase`] and its private helpers.

use super::connections::warp_data_index;
use super::input::{advance_player_one_frame, held_direction};
use super::OverworldPhase;
use crate::flow::tests::held;
use crate::new_game;
use assets::{MapEvents, MapHeader, MapId, MapLayout, MetatileCell};
use battle::BattleOutcome;
use engine::event_data::EventData;
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_EAST_ARROW_WARP, MB_NON_ANIMATED_DOOR, MB_NORMAL, MB_SOUTH_ARROW_WARP,
    MB_TALL_GRASS,
};
use engine::overworld::{
    warp_in_facing, ConnectedMapData, Direction, MapRuntime, PlayerState, StepOutcome,
    WALK_FRAMES_PER_TILE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// A fresh event-flag store: nothing hidden. Used by the
/// [`advance_player_one_frame`] tests below, whose fixture map
/// ([`flat_runtime`]) has no object events at all -- the phase-level tests
/// instead go through [`OverworldPhase::step`], which threads the phase's
/// own real save state.
const NO_FLAGS: EventData = EventData::new();

/// No map is ever connected -- mirrors
/// `engine::overworld::player::tests::no_connections` (that module's own
/// private fixture), needed here too now that [`advance_player_one_frame`]
/// takes its `maps` resolver generically (issue #177) rather than
/// hardcoding [`super::connections::MapConnections`].
fn no_connections(_: MapId) -> Option<(u16, u16)> {
    None
}

/// A single connected neighbour map, keyed by id -- the coordinate-
/// translation fixture for the headless crossing tests below. Mirrors
/// `engine::overworld::player::tests::SingleConnectedMap` (that module's own
/// private fixture): this crate can't import it directly (private to
/// `engine`), but the shape this issue's [`ConnectedMapData`] consumers
/// need is identical -- a neighbour's dimensions plus one decoded landing
/// cell.
#[derive(Debug, Clone, Copy)]
struct SingleConnectedMap {
    id: MapId,
    dimensions: (u16, u16),
    landing_position: (i32, i32),
    landing_cell: MetatileCell,
}

impl ConnectedMapData for SingleConnectedMap {
    fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
        (map == self.id).then_some(self.dimensions)
    }

    fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
        (map == self.id && (x, y) == self.landing_position).then_some(self.landing_cell)
    }
}

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
    let scene = crate::overworld::load_room(
        map,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
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

// -- Headless phase fixtures -------------------------------------------------

/// Brendan's House 1F: the map the headless interaction tests below drive,
/// picked because its real object events include Mom at `(2, 6)` — script
/// `PlayersHouse_1F_EventScript_Mom`, the one
/// [`crate::overworld::npc_scripts::script_text`] recognizes — visible on a
/// fresh save.
///
/// Those object events are *solid* (issue #161's collision fix — see
/// [`engine::overworld::PlayerState::step`]'s "# Collision" section), so
/// the routes below deliberately avoid occupied tiles. Under the fresh-save
/// state [`OverworldPhase::for_test`] builds, exactly **three** of this
/// map's seven object events are visible and therefore block: `(2, 6)` Mom
/// and the two Vigoroth at `(1, 3)` and `(4, 5)`. The other four are hidden,
/// by two different scripts:
///
/// - `EventScript_ResetAllMapFlags` (`data/scripts/new_game.inc`) hides Dad
///   at `(5, 6)` (`FLAG_HIDE_PLAYERS_HOUSE_DAD`) and the rival at `(8, 8)`
///   (`FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_BRENDAN`).
/// - The male branch of the skipped truck sequence
///   (`data/maps/InsideOfTruck/scripts.inc:29-30`, applied by
///   [`crate::new_game::init_save_blocks`]) hides the *rival's* mother at
///   `(2, 7)` and the rival's sibling at `(1, 5)` — they belong in May's
///   house, not this one.
///
/// Both of the latter two used to be visible here, which is why some routes
/// below still avoid `(2, 7)`: harmless now, and left alone rather than
/// re-routed for its own sake.
const ONE_F: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

/// An [`OverworldPhase`] over a **synthetic** 10x10 open room
/// (`crate::overworld::tests::synthetic_scene`) but a *real* `map_id`, so
/// no local pack is needed while [`OverworldPhase::step`]'s per-frame
/// `MapHeaderTable`/`MapEventsTable` lookups still resolve and its
/// collision/interaction run against that map's real object events. The
/// scene only supplies the layout grid the runtime walks -- flat, open, and
/// large enough for every position these tests use.
fn synthetic_phase(
    player: PlayerState,
    dialog: Option<crate::overworld::NpcDialog>,
) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        ONE_F,
        player,
        dialog,
    )
}

/// Regression for the new-game-to-overworld RNG handoff: trainer-ID
/// initialization consumes the first two `Random()` draws, and encounters
/// must continue from that advanced state rather than restarting at seed 0.
#[test]
fn new_game_rng_stream_continues_after_trainer_id_draws() {
    let phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
    let mut expected = Rng::new(new_game::NEW_GAME_RNG_SEED);
    expected.next_u16();
    expected.next_u16();

    assert_eq!(
        phase.rng.state(),
        expected.state(),
        "the phase must retain the RNG state after both trainer-ID draws"
    );
}

/// A [`MapRuntime`] over `phase`'s own scene and [`ONE_F`]'s real event
/// data -- the exact runtime [`OverworldPhase::step`] rebuilds each frame.
fn runtime_for(phase: &OverworldPhase) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new().header(ONE_F).unwrap();
    let events = assets::MapEventsTable::new().resolve(ONE_F).unwrap();
    phase.scene.runtime(ONE_F, header, events)
}

/// Senior review regression, headless: upstream discards an A press made
/// *during* a tile crossing outright -- `FieldGetPlayerInput` only sets
/// `input->pressedAButton` at `T_TILE_CENTER`/`T_NOT_MOVING`
/// (`pokeemerald/src/field_control_avatar.c:95-107`), the gate every
/// `TryStartInteractionScript` call site sits behind (`:172`). Same
/// position, same facing, same fresh A edge, only
/// [`PlayerState::in_transit`] differing: mid-step must find nothing, at
/// rest must find Mom.
///
/// Approaches Mom from the east rather than from the south: `(2, 7)`, the
/// tile directly below her, is the rival's mom's tile -- she is hidden on a
/// fresh save since the truck-intro flags landed ([`ONE_F`]'s docs), so the
/// route is kept only for stability, not necessity. `(3, 6)` and `(4, 6)` are
/// both clear of visible object events.
#[test]
fn a_pressed_mid_step_is_discarded_and_the_same_press_at_rest_interacts() {
    // Two tiles east of Mom, facing west.
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);

    // Already facing west, so a held Left steps immediately onto (3, 6) --
    // the tile from which Mom, at (2, 6), is directly ahead.
    phase.step(held(Buttons::LEFT));
    assert_eq!(phase.player.position(), (3, 6));
    assert_eq!(phase.player.facing(), Direction::West);
    assert!(
        phase.player.in_transit(),
        "the step's walk animation must still be running"
    );

    {
        let runtime = runtime_for(&phase);
        assert!(
            phase
                .interaction_tokens_this_frame(pressed(Buttons::A), &runtime)
                .is_none(),
            "an A press during a tile crossing must be discarded"
        );
    }

    // Drain the rest of the crossing with no input held.
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert!(!phase.player.in_transit(), "the crossing must have settled");
    assert_eq!(phase.player.position(), (3, 6), "same tile as above");
    assert_eq!(phase.player.facing(), Direction::West, "same facing");

    {
        let runtime = runtime_for(&phase);
        assert!(
            phase
                .interaction_tokens_this_frame(pressed(Buttons::A), &runtime)
                .is_some(),
            "at rest, the identical press must find Mom and her recognized \
             script"
        );
        assert!(
            phase
                .interaction_tokens_this_frame(ButtonState::new(), &runtime)
                .is_none(),
            "and only a fresh A edge interacts at all"
        );
    }
}

/// Headless counterpart to the real-pack acceptance test: while a dialog is
/// open, [`OverworldPhase::step`] must not feed movement to the player at
/// all (module docs' "NPC dialog routing" section -- upstream's `lock`,
/// which stops `RunFieldInput` being polled while a message box owns
/// input). Dropping that early return lets the held direction through and
/// fails here, with no local pack needed.
#[test]
fn an_open_dialog_freezes_movement_until_it_closes() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    // Facing south already, so an un-frozen held Down would step
    // immediately -- no turn-in-place frame to absorb it. (7, 5), the tile
    // below, is clear of visible object events, so this test measures the
    // dialog freeze and nothing else -- unlike (4, 5), which a Vigoroth now
    // occupies solidly; see [`ONE_F`]'s docs.
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.player.position(),
            (7, 4),
            "movement must be frozen while a dialog is open"
        );
        assert!(!phase.player.in_transit(), "no step may even have started");
    }
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "and no turn either"
    );
    assert!(phase.dialog.is_some(), "the dialog must still be open");

    // Confirm through the trailing prompt (same held-A-across-the-window
    // reasoning as this module's real-pack dialog test) and let it close.
    let mut closed = false;
    for _ in 0..40 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed, "confirming must close the synthetic dialog");

    // Control returns: the very next held Down steps.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (7, 5),
        "ordinary movement must resume once the box has closed"
    );
}

/// Mutation guard for [`OverworldPhase::step`]'s tileset-animation tick
/// (issue #160): `self.tick` must advance by exactly one per `step` call,
/// and must keep advancing while a dialog box is open -- an explicit
/// fidelity claim in the field's own docs (upstream's
/// `UpdateTilesetAnimations` runs every `VBlank` regardless of message-box
/// state, so background tiles keep animating behind a frozen player).
/// Nothing else in the suite observes `tick` on a headless phase: deleting
/// the increment, moving it below `step`'s dialog early-return, or making it
/// conditional must all fail here.
#[test]
fn step_advances_the_tileset_animation_tick_once_per_frame_even_behind_a_dialog() {
    use engine::text::Token;

    // No dialog: one tick per frame, moving or not.
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), None);
    assert_eq!(phase.tick, 0, "a freshly built phase starts at tick 0");
    phase.step(ButtonState::new());
    assert_eq!(phase.tick, 1, "an idle frame still advances the animation");
    for expected in 2..=WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.tick,
            u32::from(expected),
            "every frame of a walk step advances the tick exactly once"
        );
    }

    // Dialog open: movement is frozen (see
    // `an_open_dialog_freezes_movement_until_it_closes`), the tick is not.
    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    let mut frozen = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));
    for expected in 1..=10u32 {
        frozen.step(held(Buttons::DOWN));
        assert_eq!(
            frozen.tick, expected,
            "tileset animation must keep running while a dialog freezes movement"
        );
    }
    assert!(
        frozen.dialog.is_some(),
        "the dialog must still be open -- otherwise the frames above weren't frozen ones"
    );
    assert_eq!(
        frozen.player.position(),
        (7, 4),
        "and movement really was frozen for all of them"
    );
}

/// The other half of the [`OverworldPhase`] tick wiring (issue #160): every
/// map (re)load restarts the room's animation counter at 0, mirroring
/// upstream's `InitTilesetAnimations` call sites
/// (`pokeemerald/src/overworld.c`). Real-pack, because both constructors
/// under test load rooms: [`OverworldPhase::load_default`] must hand back a
/// phase at tick 0, and [`OverworldPhase::warp_to`] must reset a
/// *already-advanced* counter -- deleting `self.tick = 0` from `warp_to`
/// leaves the destination map's flowers/water mid-cycle and must fail here.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn loading_a_room_and_warping_both_restart_the_tileset_animation_tick() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.tick, 0,
        "a freshly loaded room starts its animation counter at 0"
    );

    // Advance the counter well past 0 (idle frames: the spawn tile is the
    // stair warp, so this deliberately holds nothing).
    for _ in 0..37 {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.tick, 37, "37 steps, 37 ticks");

    phase.warp_to(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);
    assert_eq!(
        phase.tick, 0,
        "a warp is a map load: the destination map's animated tiles must start \
         from their own tick 0, not from the departed map's counter"
    );
}

/// Issue #207 review, round 2: the production starter *wiring*, not just the
/// starter constructor -- [`OverworldPhase::load_default`] must hand back a
/// phase whose party lead is the provisional starter, or a real playthrough
/// rolls I-4 encounters it can never fight. Mutation-pinned: deleting
/// `load_default`'s `party_lead` assignment fails only here, because every
/// pack-free test builds its phase through `for_test`, which deliberately
/// leaves the lead `None`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn load_default_hands_back_a_fightable_provisional_starter() {
    let phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let lead = phase
        .party_lead
        .as_ref()
        .expect("a fresh game starts with the provisional starter (issue #207 review)");
    assert_eq!(lead.species(), new_game::PROVISIONAL_STARTER_SPECIES);
    assert_eq!(lead.level(), new_game::PROVISIONAL_STARTER_LEVEL);
    assert!(!lead.is_fainted(), "the lead must be able to fight");
}

/// The finding-1 regression at the phase level, on real map data: holding a
/// direction into a visible NPC must stop the player on the adjacent tile.
/// Before object-event collision landed, [`OverworldPhase::step`] walked the
/// avatar straight through Mom.
///
/// Uses [`ONE_F`]'s real object events (Mom at `(2, 6)`, visible on a fresh
/// save) over a synthetic open layout, so no extracted pack is needed --
/// every tile on the approach is walkable as far as the *grid* is
/// concerned, which is what makes the stop attributable to Mom alone.
#[test]
fn holding_a_direction_into_a_visible_npc_stops_the_player_adjacent_to_it() {
    // Two tiles below Mom, facing north; approach from the east ((4, 6) ->
    // (3, 6) -> blocked by Mom) -- see ONE_F's note on the (2, 7) routes.
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);

    // First step lands on (3, 6), the tile east of Mom.
    phase.step(held(Buttons::LEFT));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::LEFT));
    }
    assert_eq!(phase.player.position(), (3, 6));
    assert!(!phase.player.in_transit());

    // Keep holding: every further poll is denied, and the player never
    // reaches (2, 6). A generous budget, so this fails on *any* frame that
    // lets the step through, not just the first.
    for _ in 0..(4 * u32::from(WALK_FRAMES_PER_TILE)) {
        phase.step(held(Buttons::LEFT));
        assert_eq!(
            phase.player.position(),
            (3, 6),
            "the player must stop on the tile adjacent to Mom, never enter hers"
        );
        assert!(
            !phase.player.in_transit(),
            "a blocked step must not start a walk animation"
        );
    }
    assert_eq!(
        phase.player.facing(),
        Direction::West,
        "bumping into an NPC leaves the avatar facing it (PlayerNotOnBikeCollide)"
    );

    // And that same standing position interacts, proving the stop is
    // adjacency rather than the interaction lookup and the collision check
    // disagreeing about where Mom is.
    let runtime = runtime_for(&phase);
    assert!(
        phase
            .interaction_tokens_this_frame(pressed(Buttons::A), &runtime)
            .is_some(),
        "the tile the player was stopped on must be the tile Mom is \
         interactable from"
    );
}

/// The complement, same fixture shape: a *hidden* object event does not
/// block. `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`'s Dad
/// (`OBJ_EVENT_GFX_NORMAN` at `(5, 6)`) is hidden by
/// `EventScript_ResetAllMapFlags`' `setflag FLAG_HIDE_PLAYERS_HOUSE_DAD`
/// (`pokeemerald/data/scripts/new_game.inc`), and upstream never spawns a
/// hidden template (`event_object_movement.c:1670-1672`) -- so the player
/// walks over his tile exactly as if it were empty.
#[test]
fn a_hidden_npcs_tile_is_walkable() {
    let mut phase = synthetic_phase(PlayerState::new((5, 7), 3, Direction::North), None);
    let dad = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .unwrap()
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_NORMAN")
        .expect("1F's object events include Dad");
    assert_eq!(
        (dad.x, dad.y),
        (5, 6),
        "fixture precondition: Dad's real map.json position"
    );
    assert!(
        !engine::overworld::object_event_is_visible(dad, &phase.save1().event_data),
        "fixture precondition: a fresh save hides Dad"
    );

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (5, 6),
        "a hidden object event's tile must be walkable"
    );
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
    advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
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
        advance_player_one_frame(
            &mut player,
            Some(Direction::South),
            &runtime,
            &no_connections,
            &NO_FLAGS,
        );
        assert_eq!(player.step_progress(), expected);
    }
    assert!(
        player.in_transit(),
        "still mid-transit one frame before settling"
    );

    // The 16th frame (this crossing's `WALK_FRAMES_PER_TILE`th) is the
    // one where the transit settles -- 16 rendered frames total to
    // cross one tile, not 17.
    advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
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

    advance_player_one_frame(
        &mut player,
        Some(Direction::East),
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
    assert_eq!(player.facing(), Direction::East, "must have turned");
    assert_eq!(player.position(), (2, 2), "a turn must not move the tile");
    assert!(!player.in_transit());
    assert_eq!(player.step_progress(), 0);
}

// -- Map-edge connection crossing (issue #177): coordinate translation -----
//
// Headless, pack-free: `advance_player_one_frame`'s `maps` parameter is
// generic (this module's own doc comment on why), so these exercise the
// exact integration function `OverworldPhase::step` calls, over a synthetic
// two-map graph, without needing a local asset pack. The real
// Littleroot Town <-> Route 101 crossing (offset 0 in both directions) is
// additionally pinned end to end, through the whole `OverworldPhase`, by
// the `#[ignore]`d `real_pack_*` tests further down this file.

/// A small flat map whose header carries one connection in `direction`, to
/// `target` with `offset` -- the fixture the coordinate-translation tests
/// below step off the edge of. Mirrors [`flat_runtime`] plus
/// `engine::overworld::map_runtime::tests::south_connected_runtime`'s own
/// shape (that fixture is private to `engine`, so this is a small
/// reimplementation, not a shared helper).
fn connected_runtime(
    width: u16,
    height: u16,
    direction: assets::Direction,
    offset: i32,
    target: MapId,
) -> MapRuntime<'static> {
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

    let connections: &'static [assets::MapConnection] =
        Box::leak(Box::new([assets::MapConnection {
            direction,
            offset,
            target,
        }]));
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
        connections,
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

/// The straight case: an `offset: 0` south connection (Route 101's own
/// south connection to Littleroot Town, per `data/maps/Route101/map.json`,
/// is exactly this shape) carries the stepped-off x coordinate straight
/// across unchanged, landing on the neighbour's opposite (north) edge.
#[test]
fn advance_player_one_frame_crosses_a_zero_offset_connection_unchanged() {
    let runtime = connected_runtime(5, 5, assets::Direction::South, 0, MapId("MAP_SOUTH"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (3, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    };
    let mut player = PlayerState::new((3, 4), 3, Direction::South);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        &runtime,
        &maps,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Crossed {
            to_map: MapId("MAP_SOUTH"),
            to_position: (3, 0),
        }
    );
    assert_eq!(player.position(), (3, 0));
    assert!(player.in_transit(), "a crossing is a step like any other");
}

/// The offset case: a nonzero `offset` on an east/west connection shifts
/// the *cross axis* (`y`, for an east/west edge) coordinate by
/// `-offset` when carrying it into the neighbour's own space -- upstream
/// `IsPosInConnectingMap`/`SetPositionFromConnection`
/// (`pokeemerald/src/fieldmap.c:578-598, 699-712`), already exercised at the
/// engine layer (`engine::overworld::map_runtime::tests::
/// resolve_connection_applies_perpendicular_offset`) -- pinned again here at
/// the actual integration entry point [`OverworldPhase`] wires
/// [`engine::overworld::PlayerState::step`] through
/// ([`advance_player_one_frame`]), not just the engine function underneath
/// it.
#[test]
fn advance_player_one_frame_crosses_an_offset_connection_shifting_the_cross_axis() {
    let runtime = connected_runtime(6, 6, assets::Direction::East, -2, MapId("MAP_EAST"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_EAST"),
        dimensions: (10, 10),
        // Stepping east off x=5 (width) at y=5: target_y = y - offset =
        // 5 - (-2) = 7, landing at the neighbour's west edge (x=0).
        landing_position: (0, 7),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    };
    let mut player = PlayerState::new((5, 5), 3, Direction::East);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::East),
        &runtime,
        &maps,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Crossed {
            to_map: MapId("MAP_EAST"),
            to_position: (0, 7),
        }
    );
    assert_eq!(player.position(), (0, 7));
}

/// The same offset rule on the *north/south* axis -- the axis the
/// Littleroot -> Route 101 north-star crossing actually uses (both its real
/// connections happen to carry `offset: 0`, which is exactly why a sign
/// regression in `target_x = x - offset` would survive every real-map
/// test; this synthetic pin is the guard).
#[test]
fn advance_player_one_frame_crosses_an_offset_north_connection_shifting_x() {
    let runtime = connected_runtime(6, 6, assets::Direction::North, -2, MapId("MAP_NORTH"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_NORTH"),
        dimensions: (10, 10),
        // Stepping north off y=0 at x=5: target_x = x - offset =
        // 5 - (-2) = 7, landing at the neighbour's south edge (y = 9).
        landing_position: (7, 9),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    };
    let mut player = PlayerState::new((5, 0), 3, Direction::North);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::North),
        &runtime,
        &maps,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Crossed {
            to_map: MapId("MAP_NORTH"),
            to_position: (7, 9),
        }
    );
    assert_eq!(player.position(), (7, 9));
}

/// Different lateral positions along the *same* edge each translate to
/// their own distinct landing tile (not, say, a single hardcoded landing
/// position regardless of where the player actually crossed) -- the
/// property that makes walking along Littleroot's whole north edge land on
/// the matching column of Route 101, not just one fixed spot.
#[test]
fn advance_player_one_frame_crossing_at_different_lateral_positions_lands_at_the_matching_column() {
    let runtime = connected_runtime(8, 8, assets::Direction::North, 0, MapId("MAP_NORTH"));

    for x in [0_i32, 3, 7] {
        let maps = SingleConnectedMap {
            id: MapId("MAP_NORTH"),
            dimensions: (8, 8),
            landing_position: (x, 7),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 3,
            },
        };
        let mut player = PlayerState::new((x, 0), 3, Direction::North);
        let outcome = advance_player_one_frame(
            &mut player,
            Some(Direction::North),
            &runtime,
            &maps,
            &NO_FLAGS,
        );
        assert_eq!(
            outcome,
            StepOutcome::Crossed {
                to_map: MapId("MAP_NORTH"),
                to_position: (x, 7),
            },
            "column x={x} must land at the neighbour's matching column, not a fixed spot"
        );
    }
}

/// A candidate connection whose perpendicular bounds don't cover the
/// stepped-off position at all is rejected outright -- the step is simply
/// blocked, exactly as if there were no connection there (mirrors
/// `engine::overworld::map_runtime::tests::
/// resolve_connection_rejects_position_outside_target_bounds`, again pinned
/// through this module's own integration entry point).
#[test]
fn advance_player_one_frame_rejects_a_crossing_outside_the_neighbours_bounds() {
    // The neighbour is much narrower than this map: only x in 0..3 lands.
    let runtime = connected_runtime(10, 3, assets::Direction::South, 0, MapId("MAP_NARROW"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_NARROW"),
        dimensions: (3, 3),
        landing_position: (8, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    };
    let mut player = PlayerState::new((8, 2), 3, Direction::South);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        &runtime,
        &maps,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: engine::overworld::Collision::Impassable,
        },
        "x=8 is outside the 3-wide neighbour's bounds, so the crossing must fail closed"
    );
    assert_eq!(
        player.position(),
        (8, 2),
        "a rejected crossing must not move the player"
    );
}

/// The real-pack acceptance test for issue #177: Littleroot Town's actual
/// north connection to Route 101 (`offset: 0` in both directions --
/// `data/maps/LittlerootTown/map.json:15-21`,
/// `data/maps/Route101/map.json`'s own south connection back), crossed both
/// ways through the whole [`OverworldPhase`].
///
/// `x = 10` is real map data's own walkable path column: Littleroot's row
/// `y = 0` and Route 101's row `y = 19` (both maps are `20x20`, so `19` is
/// Route 101's own last row) are each open ground at `x` in `{10, 11}` --
/// confirmed by hand against `data/layouts/LittlerootTown/map.bin` and
/// `data/layouts/Route101/map.bin` -- flanked by collision-1 fence tiles
/// everywhere else along both edges, and no object event sits anywhere near
/// either row (both maps' own `map.json`).
///
/// **`(10, 19)` is also the Route 101 rescue coord-event trigger tile (issue
/// #231).** That is deliberate and left alone: this phase is built with no
/// `party_lead`, so the trigger fires on the crossing's drain frame and
/// `super::first_battle_trigger`'s own `begin_first_battle` takes its
/// documented "no party mon yet" arm -- it consumes the trigger
/// (`VAR_ROUTE101_STATE` goes to `2`, upstream's mid-cutscene ordering) and
/// logs, but builds no battle, so nothing freezes the frame and the walk back
/// south below proceeds exactly as it did before that slice existed. This
/// test's subject is the *connection crossing*, and giving it a lead to keep
/// the trigger quiet would only trade a logged no-op for a battle it would
/// then have to play out. The trigger's own behaviour on this tile is pinned
/// by `real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
#[allow(clippy::too_many_lines)] // one continuous two-crossing walk; splitting would re-set-up the pack scene
fn walking_off_littlerootss_north_edge_crosses_into_route_101_and_back() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    // Two ordinary tiles south of the north edge, already facing the
    // direction that will carry it there. No `party_lead` is assigned: see
    // the doc comment on the rescue trigger this walk lands on.
    let player = PlayerState::new((10, 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, littleroot, player, None);

    // Two ordinary steps north, each draining its own walk animation, land
    // on the map's own last interior row (y = 0).
    for _ in 0..2 {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
        assert_eq!(phase.map_id, littleroot, "still on Littleroot's own grid");
    }
    assert_eq!(phase.player.position(), (10, 0));

    // The third step walks off the grid's own top edge -- the crossing.
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.tick, 33,
        "a crossing is NOT a map load: upstream's LoadMapFromCameraTransition re-inits only \
         the secondary tileset counter (InitSecondaryTilesetAnimation, overworld.c:815), so \
         the primary counter this port models keeps running -- 32 walk frames plus the \
         crossing step's own increment, with no reset (contrast the warp test above)"
    );
    assert_eq!(
        phase.pending_landing,
        Some((10, 19)),
        "the crossing step's landing tile re-latches in the *entered* map's coordinate \
         space, so the drain-frame door check evaluates against Route 101"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.pending_landing, None,
        "the drain frame consumed the latched landing (Route 101's south edge is no door)"
    );

    assert_eq!(
        phase.map_id, route101,
        "walking off Littleroot's north edge must rebind to Route 101"
    );
    assert_eq!(
        phase.player.position(),
        (10, 19),
        "offset 0 carries x straight across, landing on Route 101's own south edge \
         (height 20, so y = 19)"
    );
    assert_eq!(
        phase.player.elevation(),
        3,
        "the landing cell's own elevation, adopted the same way an ordinary step's is"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a plain connection crossing does not alter facing -- the player was already \
         walking north when it fired"
    );
    assert!(
        !phase.player.in_transit(),
        "the walk animation the crossing step started has already fully drained by now"
    );

    let route101_header = assets::MapHeaderTable::new()
        .header(route101)
        .expect("Route 101 must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(route101_header.group).unwrap()
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(route101_header.num).unwrap()
    );
    assert_eq!(
        phase.save1().location.warp_id,
        -1,
        "a connection crossing names no warp event -- WARP_ID_NONE (overworld.c:633)"
    );
    assert_eq!(
        (phase.save1().location.x, phase.save1().location.y),
        (-1, -1),
        "LoadMapFromCameraTransition's own SetWarpDestination call shape (overworld.c:786)"
    );
    assert_eq!(
        (
            i32::from(phase.save1().pos.x),
            i32::from(phase.save1().pos.y)
        ),
        (10, 19),
        "save1.pos must mirror the post-crossing tile"
    );

    // Cross right back south: (10, 19) is already Route 101's own last row,
    // so a single south step walks straight off that edge too.
    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }

    assert_eq!(
        phase.map_id, littleroot,
        "walking south off Route 101's own south edge must cross back into Littleroot"
    );
    assert_eq!(
        phase.player.position(),
        (10, 0),
        "landing back on Littleroot's own north edge, same x"
    );
    assert_eq!(phase.player.elevation(), 3);

    let littleroot_header = assets::MapHeaderTable::new()
        .header(littleroot)
        .expect("Littleroot Town must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(littleroot_header.group).unwrap()
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(littleroot_header.num).unwrap()
    );
    assert_eq!(phase.save1().location.warp_id, -1);
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
    let fresh = crate::overworld::load_room(
        one_f,
        crate::overworld::PlayerCharacter::Brendan,
        &phase.save1.event_data,
    )
    .expect("1F must load from the extracted pack");
    assert!(
        phase.compose_frame()[..]
            == fresh.compose_frame(&phase.player, &phase.save1.event_data, 0)[..],
        "warp_to must rebind `scene` to the destination map, not just `map_id` -- `tick` is 0 \
         on both sides since `warp_to` resets `phase.tick` and `fresh` has never `step`ped"
    );
}

/// `ClearTempFieldEventData` (`overworld.c:848`, in `LoadMapFromWarp`,
/// ahead of `RunOnTransitionMapScript` at `:860`): a warp clears the
/// per-map-load temp flag/var ranges -- load-bearing since Route 103's
/// cuttable-tree object events ride `FLAG_TEMP_12`/`_13`
/// (`assets::object_event_flags`, issue #248) -- while ordinary persistent
/// state survives untouched. The connection-crossing sibling
/// (`LoadMapFromCameraTransition`, `:798`) is pinned by
/// `route103_rival_tests::walking_north_from_route_101_crosses_oldale_town_into_route_103`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_clears_temp_field_event_data_but_not_persistent_flags() {
    // `FLAG_TEMP_12` (`TEMP_FLAGS_START + 0x12`) and `VAR_TEMP_3`
    // (`TEMP_VARS_START + 0x3`) -- independently transcribed, the same
    // "each module cites its own constant" convention as everywhere else.
    const FLAG_TEMP_12: u16 = 0x12;
    const VAR_TEMP_3: u16 = 0x4003;
    // An ordinary persistent flag far outside the temp range
    // (`FLAG_HIDE_ROUTE_103_RIVAL`, `include/constants/flags.h:772`).
    const FLAG_HIDE_ROUTE_103_RIVAL: u16 = 0x2D3;

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    phase.save1.event_data.flag_set(FLAG_TEMP_12).unwrap();
    phase.save1.event_data.var_set(VAR_TEMP_3, 7).unwrap();
    phase
        .save1
        .event_data
        .flag_set(FLAG_HIDE_ROUTE_103_RIVAL)
        .unwrap();

    phase.warp_to(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);

    assert_eq!(
        phase.save1.event_data.flag_get(FLAG_TEMP_12),
        Ok(false),
        "a warp is a map load -- the temp flag range must clear"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_TEMP_3),
        Ok(0),
        "and the temp var range with it"
    );
    assert_eq!(
        phase.save1.event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(true),
        "while ordinary persistent flags survive the load untouched"
    );
}

/// A phase standing on 1F's own floor, for the doormat tests below: a real
/// pack-loaded [`ONE_F`] scene with the player placed at `position` facing
/// `facing`, at rest.
fn one_f_phase(position: (i32, i32), facing: Direction) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::load_room(
            ONE_F,
            crate::overworld::PlayerCharacter::Brendan,
            &engine::event_data::EventData::new(),
        )
        .expect("run `cargo xtask extract` first"),
        ONE_F,
        PlayerState::new(position, 3, facing),
        None,
    )
}

/// The issue #174 acceptance test: from inside 1F, walking onto the
/// doormat (warp #1, `(8, 8)`, `MB_SOUTH_ARROW_WARP`) while holding South
/// fires the arrow-warp trigger -- upstream `TryArrowWarp`/
/// `IsArrowWarpMetatileBehavior` (`field_control_avatar.c:688-699, 767-780`,
/// polled every frame `input->heldDirection && input->dpadDirection ==
/// playerDirection` holds, `:164-168` -- `OverworldPhase::step`'s "Warp
/// timing" section) -- and exits through the front door to
/// `MAP_LITTLEROOT_TOWN`'s own warp #1 (`(5, 8)`, `MB_ANIMATED_DOOR`,
/// already pinned by
/// [`the_front_door_warp_faces_north_from_the_destination_tiles_behavior`]),
/// landing facing South per that tile's own `IsNonAnimDoor||IsDoor` branch
/// (`overworld.c:935-936`). This is the doormat this port's whole warp
/// stack exists to fire: without it, the player can walk down into
/// Brendan's house but never back out through the front door.
///
/// Down is held for the **whole** crossing, which is what upstream's gate
/// actually requires -- see
/// [`releasing_south_mid_step_does_not_exit_through_the_doormat`] for the
/// sibling that releases it and must therefore *not* warp.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_doormat_holding_south_exits_through_the_front_door() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    // One tile north of the doormat, already facing South -- so the very
    // first held-Down frame steps directly onto it with no turn frame
    // needed first.
    let mut phase = one_f_phase((8, 7), Direction::South);

    // Frame 1: the step onto the doormat begins. `PlayerState` commits the
    // tile immediately, but no warp may fire while the crossing is still
    // animating -- upstream's `heldDirection` is only ever set at
    // `T_TILE_CENTER`/`T_NOT_MOVING` (`field_control_avatar.c:95-112`).
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the step onto the doormat must commit on the frame it begins"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "the arrow warp must not fire on the frame the step begins"
    );

    // Frames 2..16: keep Down held (upstream polls the *currently held*
    // direction, so releasing it would be a different test) while the walk
    // animation drains. The player is mid-crossing throughout, so the poll
    // stays closed and the map must not change.
    for frame in 2..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.map_id, ONE_F,
            "the warp must not fire mid-animation (frame {frame} of {WALK_FRAMES_PER_TILE})"
        );
    }

    // Frame 16: the walk animation drains, the player is at rest on the
    // doormat, Down is still held and still equals their facing -- so
    // `TryArrowWarp`'s gate opens and the doormat's own
    // MB_SOUTH_ARROW_WARP behavior matches it.
    phase.step(held(Buttons::DOWN));

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN");
    assert_eq!(
        phase.map_id, destination,
        "the doormat's arrow-warp trigger must fire on the first at-rest \
         frame with South still held"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the front door's MB_ANIMATED_DOOR behavior drives the arrival \
         facing (overworld.c:935-936)"
    );
    assert!(
        !phase.player.in_transit(),
        "the warp lands the player at rest, not mid-step"
    );
}

/// Senior-review regression (#174 finding 1b, the over-trigger half):
/// upstream's arrow-warp gate reads the direction **currently held**
/// (`input->heldDirection && input->dpadDirection == playerDirection`,
/// `field_control_avatar.c:164-168`), not the direction some earlier step
/// happened to be taken in. So tapping Down for one frame and releasing it
/// during the crossing walks the player onto the doormat and leaves them
/// there -- `tookStep` would be true, but `heldDirection` is false.
///
/// Identical setup and frame count to
/// [`walking_onto_the_doormat_holding_south_exits_through_the_front_door`];
/// the only difference is the released button, so a regression that keys
/// arrow warps off the landing rather than the held keys fails here.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn releasing_south_mid_step_does_not_exit_through_the_doormat() {
    let mut phase = one_f_phase((8, 7), Direction::South);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the step onto the doormat must still commit"
    );

    // Down released from frame 2 onward, including the frame the crossing
    // completes -- the same 16-step count as the holding sibling, so the
    // released-button case lands exactly on the drain frame.
    for _ in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
    }

    assert!(
        !phase.player.in_transit(),
        "the crossing must have fully drained -- otherwise this test proves nothing"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "a released direction must not warp: upstream needs heldDirection on \
         the frame the check runs, not merely a completed step"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the player stays standing on the doormat"
    );
}

/// Review regression (#191, Codex P2): upstream evaluates `TryArrowWarp`
/// *before* `PlayerStep` mutates the player, so the gate compares the held
/// direction against the facing the frame *started* with. A player who just
/// warped onto the doormat facing North and taps Down for a single frame is
/// only *turned* South by that frame (`field_control_avatar.c:164-168` reads
/// the pre-movement `playerDirection`); the warp may not fire until a later
/// frame still holds Down against the now-South facing. Reading the
/// post-turn facing instead would exit the house on the tap frame itself.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_one_frame_down_tap_on_the_doormat_facing_north_turns_without_warping() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    // The state a warp *into* the house leaves the player in: standing on
    // the doormat, facing North (see
    // `warping_to_the_front_doormat_faces_north_and_rebinds_the_scene`).
    let mut phase = one_f_phase((8, 8), Direction::North);

    // The tap frame: Down is held against a North facing, so upstream's
    // pre-movement gate stays closed and the frame only turns the player.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "the tap frame must only turn the player, not fire the arrow warp"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the tap turned the player"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "still standing on the doormat"
    );

    // Down released before the next frame: the gate reads the *currently*
    // held direction, so nothing fires and the player stays put.
    phase.step(ButtonState::new());
    assert_eq!(
        phase.map_id, ONE_F,
        "a released tap leaves the player standing on the doormat, as upstream does"
    );

    // Holding Down again -- now against the already-South facing -- is the
    // ordinary arrow-warp case: the gate opens and the doormat fires.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "holding Down against the South facing fires the doormat normally"
    );
}

/// Senior-review regression (#174 finding 1a, the under-trigger half, and
/// the v1 north-star doormat interaction): the state a warp *into* the
/// house leaves the player in -- standing on the doormat at `(8, 8)`,
/// facing North, because the mat's own `MB_SOUTH_ARROW_WARP` drives
/// `GetAdjustedInitialDirection` that way (`overworld.c:937-938`, pinned by
/// [`warping_to_the_front_doormat_faces_north_and_rebinds_the_scene`]).
///
/// From there, holding Down must exit. Upstream gets this from the
/// every-frame `heldDirection` poll: the turn-in-place makes
/// `playerDirection` South, the poll matches, `TryArrowWarp` fires. It
/// cannot come from a step, because `(8, 9)` is off-map -- the step is
/// blocked forever, so a landing-gated arrow warp makes the front door a
/// permanent one-way trip into the house.
///
/// The poll reads the frame's *pre-movement* facing (review finding on
/// #191, matching upstream's `TryArrowWarp`-before-`PlayerStep` order), so
/// the first held-Down frame only turns the player and the exit fires on
/// the second, when the held direction meets the already-South facing --
/// exactly upstream's frame anatomy. The released-tap half of the same
/// finding is pinned by
/// [`a_one_frame_down_tap_on_the_doormat_facing_north_turns_without_warping`].
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn standing_on_the_doormat_facing_north_and_holding_south_exits() {
    let mut phase = one_f_phase((0, 0), Direction::South);

    // Arrive the way the front door actually arrives, rather than asserting
    // the post-warp state by hand.
    phase.warp_to(ONE_F, 1);
    assert_eq!(phase.player.position(), (8, 8), "1F's warp #1: the doormat");
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a warp-in onto a south-arrow tile faces back out of it"
    );

    // Hold Down. Frame 1 only turns in place: the poll compares against the
    // pre-movement (still-North) facing, so it stays closed while the turn
    // happens -- upstream's `TryArrowWarp` runs before `PlayerStep`.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "the turn frame must not warp: the poll reads the pre-turn facing"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "frame 1 turned the player to face the held direction"
    );

    // Frame 2: Down is still held and now meets the already-South facing,
    // so the every-frame poll fires the doormat.
    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "holding Down while standing on the doormat must exit the house -- \
         the step south is blocked (off-map), so only the every-frame \
         heldDirection poll can ever fire this"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(phase.player.facing(), Direction::South);
}

/// The companion to the test above: a warp-in alone must *not* warp back
/// out, however many frames pass with nothing held. This is what makes the
/// every-frame poll safe -- [`warp_in_facing`] lands the arrival facing out
/// of the arrow, so its direction can never match a held one without a
/// deliberate turn first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn arriving_on_the_doormat_does_not_immediately_warp_back_out() {
    let mut phase = one_f_phase((0, 0), Direction::South);
    phase.warp_to(ONE_F, 1);

    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) * 2 {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, ONE_F,
            "an arrival on an arrow tile must not re-fire it"
        );
    }
    assert_eq!(phase.player.position(), (8, 8));
    assert_eq!(phase.player.facing(), Direction::North);
}

/// Regression: the same doormat, same position, but facing/holding a
/// direction other than South must not fire the arrow-warp trigger --
/// `IsArrowWarpMetatileBehavior` only matches the one direction its id
/// belongs to (`engine::overworld::warp::is_arrow_warp_trigger`'s own unit
/// tests cover the predicate directly; this is the phase-level version:
/// landing on the doormat by walking sideways past it must not exit).
///
/// Right is held for the whole crossing *and* past it, so the every-frame
/// poll really does run with a direction held that equals the player's
/// facing -- only the id-vs-direction match denies it.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_doormat_facing_east_does_not_exit() {
    // One tile west of the doormat, already facing East, so the first
    // held-Right frame steps directly onto (8, 8) facing East instead of
    // South.
    let mut phase = one_f_phase((7, 8), Direction::East);

    phase.step(held(Buttons::RIGHT));
    assert_eq!(phase.player.position(), (8, 8));
    // 15 more frames: the crossing drains on the last of them, and Right is
    // still held on that frame -- so the arrow poll really does run, with a
    // held direction that equals the player's facing.
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(
        !phase.player.in_transit(),
        "the crossing must have fully drained on the last frame Right was polled"
    );

    assert_eq!(
        phase.map_id, ONE_F,
        "the doormat must not fire while facing East -- only South matches \
         its MB_SOUTH_ARROW_WARP behavior"
    );
    assert_eq!(phase.player.position(), (8, 8));
}

/// The issue #194 acceptance test: upstream runs `ProcessPlayerFieldInput`
/// *before* `PlayerStep` and skips the step entirely once it consumes the
/// input (`pokeemerald/src/overworld.c:1444-1455`), so a *legal, walkable*
/// step in an arrow-warp tile's own direction warps instead of stepping.
/// No bundled real map can exercise this -- every arrow tile this port's own
/// data reaches has its arrow direction impassable, the doormat's `(8, 9)`
/// off-map among them (`OverworldPhase::step`'s "Warp timing" docs) -- so
/// this borrows the doormat's own real, static warp-event data (`ONE_F`'s
/// warp #1, `(8, 8)` -> `MAP_LITTLEROOT_TOWN` warp #1; needs no pack, see
/// [`warp_tile_behavior`]'s own doc comment on why `MapEventsTable` is
/// always available) but drops it onto a **synthetic** scene
/// ([`crate::overworld::tests::synthetic_scene_with_special_tile`]) where
/// `(8, 9)` -- unlike the real off-map tile -- is ordinary, walkable ground.
///
/// The player starts already standing on the doormat, facing and holding
/// South -- the same state
/// [`standing_on_the_doormat_facing_north_and_holding_south_exits`]'s second
/// frame reaches, which already proves the ordinary (post-movement) poll
/// fires when the step south is *blocked*. This test is that one's
/// walkable-exit counterpart: before issue #194, `advance_player_one_frame`
/// ran first here, the (now legal) step to `(8, 9)` landed, `in_transit`
/// closed the poll for the whole crossing, and by the time it reopened the
/// player was standing on `(8, 9)` -- ordinary ground, no warp, ever.
///
/// # What this test does and does not prove (pack-free ratchet)
///
/// This one runs in a plain `cargo test --workspace`, with no extracted
/// pack, and that is the point: it is the *ratchet* that keeps the #194
/// ordering from regressing in the default gate. But `warp_to` finishes
/// through [`crate::overworld::load_room`], which needs a local pack for
/// the real `MAP_LITTLEROOT_TOWN` destination, so with no pack the warp
/// resolves, bails out at the load, and leaves the player where they were.
/// Hence the assertions here are deliberately only the *negative* half --
/// the player must never reach `(8, 9)`, and must never be left
/// mid-crossing -- which is exactly and only what the pre-#194
/// movement-first ordering violated.
///
/// What it therefore does **not** prove is that a warp actually fired: a
/// regression that skipped the step while producing no warp at all (a
/// soft-lock -- input consumed, nothing happening) would still satisfy both
/// assertions. That positive half is pinned by the pack-gated sibling
/// [`a_legal_step_in_the_arrow_direction_lands_the_warp`], which runs this
/// same synthetic scene and asserts the destination map and tile.
#[test]
fn a_legal_step_in_the_arrow_direction_warps_instead_of_stepping() {
    let mut phase = walkable_south_arrow_phase();

    phase.step(held(Buttons::DOWN));

    assert_ne!(
        phase.player.position(),
        (8, 9),
        "a legal step in the arrow direction must never happen -- the warp preempts it \
         (overworld.c:1444-1455); the pre-#194 movement-first ordering would have stepped \
         the player onto (8, 9) here, before the poll ever got a chance to fire"
    );
    assert!(
        !phase.player.in_transit(),
        "no walk animation was ever started -- the step never ran"
    );
}

/// The positive half of
/// [`a_legal_step_in_the_arrow_direction_warps_instead_of_stepping`]: the
/// same synthetic scene and the same single held-South frame, but asserting
/// that the preempting warp actually *landed* rather than only that the
/// step never happened -- so a regression that skips movement without
/// warping (a soft-lock) fails here even though it passes the pack-free
/// ratchet.
///
/// Needs a local pack because `warp_to` loads the real destination room
/// ([`crate::overworld::load_room`] over extracted tileset/map data). The
/// destination pins are the same ones
/// [`standing_on_the_doormat_facing_north_and_holding_south_exits`] uses
/// for the real front door: `MAP_LITTLEROOT_TOWN`, warp #1 at `(5, 8)`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_legal_step_in_the_arrow_direction_lands_the_warp() {
    let mut phase = walkable_south_arrow_phase();

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "the preempting arrow warp must actually land, not merely eat the step"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_ne!(
        phase.player.position(),
        (8, 9),
        "and the step in the arrow direction still never happened"
    );
}

/// The shared fixture behind the two tests above: the doormat's own real,
/// static warp-event data on a **synthetic** scene where the tile south of
/// it is walkable (see
/// [`a_legal_step_in_the_arrow_direction_warps_instead_of_stepping`]'s doc
/// comment for why no real map can stand in). Asserts its own
/// preconditions, so a fixture that stopped describing the intended scene
/// fails loudly rather than passing vacuously.
fn walkable_south_arrow_phase() -> OverworldPhase {
    // `MapEventsTable` is generated at build time from checked-in map data
    // (`warp_tile_behavior`'s own doc comment), so this real warp event is
    // available with no `cargo xtask extract` pack.
    let events = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .expect("ONE_F must resolve in the generated map-events table");
    let doormat = events.warp_events[1];
    assert_eq!(
        (doormat.x, doormat.y),
        (8, 8),
        "1F's warp #1: the doormat inside"
    );

    let scene = crate::overworld::tests::synthetic_scene_with_special_tile(
        10,
        10,
        (8, 8),
        MB_SOUTH_ARROW_WARP,
    );
    let phase = OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((8, 8), 3, Direction::South),
        None,
    );

    // Fixture preconditions: the fabricated tile really is the arrow
    // behavior these tests mean to exercise, and the tile south of it --
    // unlike the real doormat's off-map `(8, 9)` -- really is walkable.
    let runtime = runtime_for(&phase);
    assert_eq!(runtime.metatile_behavior(8, 8), Some(MB_SOUTH_ARROW_WARP));
    assert!(
        runtime
            .metatile_cell(8, 9)
            .is_some_and(|cell| cell.collision == 0),
        "fixture precondition: (8, 9) must be walkable, unlike the real doormat's off-map tile"
    );

    phase
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

/// The issue #217 acceptance test's real-pack half: the *bundled* Mom
/// object event (`OBJ_EVENT_GFX_MOM` at `(2, 6)` on the real
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`, the same NPC
/// [`walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog`]
/// walks up to) must stay glued to the scrolling background through every
/// frame of a real player step driven through the real
/// [`OverworldPhase::step`] loop -- pack-loaded art, real map data, real
/// button input, no synthetic fixture anywhere.
///
/// The bug this pins (`npc`'s module docs): `PlayerState::step` commits the
/// destination tile on frame one, so before the shared
/// `viewport::camera_lag_px` term existed, Mom's OAM position jumped a full
/// metatile the instant the player pressed a direction and then sat still
/// for 16 frames while the background slid smoothly under her. The
/// signature is therefore *per-frame*: a 16 px first-frame jump instead of
/// 1 px, and an OAM delta that stops matching the BG scroll's.
///
/// Checked at the boundary frames (at rest, the first transit frame, the
/// last transit frame, and the first resting frame) and at an intermediate
/// one (`step_progress() == 8`), against **both** halves of the composed
/// frame at once: Mom's OAM `y` and the BG `scroll_y` every layer shares.
/// The unit-level counterparts -- progress 0 in all four directions, and
/// the BG scroll's own intermediate value -- live in
/// `crate::overworld::npc::tests` and `crate::overworld::viewport::tests`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_past_mom_keeps_her_oam_glued_to_the_scrolling_background() {
    /// Mom's OAM `y` and this frame's shared BG `scroll_y`, read off the
    /// same half-composed frame [`OverworldPhase::compose_frame`] would
    /// rasterize. Entry 0 is always the player; entry 1 is Mom (pinned by
    /// `crate::overworld::tests::real_pack_1f_oam_entries_cover_every_drawn_fresh_save_npc`,
    /// which also proves nobody else on 1F draws on a fresh save).
    fn mom_and_scroll(phase: &OverworldPhase) -> (i32, i32) {
        let (entries, (_, scroll_y)) = phase
            .scene
            .oam_entries_and_bg_scroll(&phase.player, &phase.save1().event_data);
        assert_eq!(entries.len(), 2, "1F draws the player and Mom, nobody else");
        (i32::from(entries[1].y()), i32::from(scroll_y))
    }

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

    // Down to 1F through the real stair warp -- same step-off/step-back
    // sequence the dialog test above uses.
    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    phase.step(held(Buttons::UP));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    let one_f = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(phase.map_id, one_f, "the stair warp must land on 1F");

    // One tile south of Mom, *already facing south* so the first held Down
    // commits a step rather than spending a frame turning.
    phase.player = PlayerState::new((2, 7), 3, Direction::South);
    let (rest_y, rest_scroll) = mom_and_scroll(&phase);
    assert_eq!(rest_scroll, 0, "at rest the BG scroll is 0");

    // Frame 1 of a real south step. The regression: this must be a *one
    // pixel* move, not the one-metatile snap the old code produced.
    phase.step(held(Buttons::DOWN));
    assert_eq!(phase.player.position(), (2, 8), "the tile commits at once");
    assert_eq!(phase.player.step_progress(), 1);
    let (frame1_y, frame1_scroll) = mom_and_scroll(&phase);
    assert_eq!(
        frame1_y - rest_y,
        -1,
        "walking south slides Mom one pixel *up* the screen on the first \
         frame -- a jump of -16 here is exactly the bug issue #217 fixed"
    );

    // Seven more frames to the midpoint, then to the last transit frame.
    for _ in 0..7 {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.player.step_progress(), 8, "halfway through the step");
    let (mid_y, mid_scroll) = mom_and_scroll(&phase);

    for _ in 0..7 {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.player.step_progress(), 15);
    assert!(phase.player.in_transit(), "frame 15 is still mid-step");
    let (last_y, last_scroll) = mom_and_scroll(&phase);

    phase.step(ButtonState::new());
    assert!(!phase.player.in_transit(), "frame 16 settles the step");
    let (settled_y, settled_scroll) = mom_and_scroll(&phase);

    // Lockstep, between transit frames: Mom's OAM moves up by exactly as
    // many pixels as the background scrolls down. (The at-rest frames are
    // compared on OAM alone -- `build_tilemaps` adds a metatile of
    // direction-dependent tilemap padding for the duration of a transit,
    // which shifts `scroll_y`'s origin by a constant that cancels only
    // between two frames on the same side of that boundary.)
    assert_eq!(
        (mid_y - frame1_y, mid_scroll - frame1_scroll),
        (-7, 7),
        "frames 1-8: 7 px of NPC travel up, 7 px of BG scroll down"
    );
    assert_eq!(
        (last_y - mid_y, last_scroll - mid_scroll),
        (-7, 7),
        "frames 8-15: the same, with no drift"
    );
    assert_eq!(
        (mid_scroll, last_scroll),
        (8, 15),
        "and in absolute terms the scroll is just the elapsed frame count"
    );

    // The boundary frames, and the total.
    assert_eq!(
        last_y - settled_y,
        1,
        "frame 15 owes exactly one last pixel"
    );
    assert_eq!(settled_scroll, 0, "back at rest, the BG scroll is 0 again");
    assert_eq!(
        rest_y - settled_y,
        16,
        "one whole metatile of travel across the step, and no more"
    );

    // The settled frame is the plain resting placement, not an
    // approximation of it: a fresh player standing on the destination
    // composes identically.
    let fresh = PlayerState::new((2, 8), 3, Direction::South);
    let (fresh_entries, fresh_scroll) = phase
        .scene
        .oam_entries_and_bg_scroll(&fresh, &phase.save1().event_data);
    assert_eq!(
        (i32::from(fresh_entries[1].y()), i32::from(fresh_scroll.1)),
        (settled_y, settled_scroll),
        "the first resting frame must equal a never-transited player's"
    );
}

// -- Message-box confirm input: A *or* B ------------------------------------

/// Upstream's down-arrow wait prompt takes `JOY_NEW(A_BUTTON | B_BUTTON)`
/// (`TextPrinterWaitWithDownArrow`, `pokeemerald/src/text.c:865-882`), not A
/// alone -- as do the mid-page wait (`TextPrinterWait`, `:884-900`) and the
/// hold-to-speed-up path (`RunTextPrinter`, `:944`/`:950`). Before this,
/// only a fresh A edge reached [`crate::overworld::NpcDialog::tick`], so a
/// player pressing B at the prompt was stuck with the box open forever.
///
/// Headless, on a synthetic dialog: B alone must drive it to close.
#[test]
fn b_alone_advances_and_closes_a_dialog() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));

    // Never press A: only B. Same held-across-the-window shape as this
    // module's other dialog tests (the exact frame the prompt becomes
    // receptive is printer-timing-dependent); the loop stops the instant
    // the box closes.
    let mut closed = false;
    for _ in 0..40 {
        phase.step(pressed(Buttons::B));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(
        closed,
        "a fresh B edge must advance the trailing prompt and close the box"
    );
}

/// The complement: neither confirm button is special-cased away, and B does
/// not leak into anything else on the frame it closes a box.
///
/// The dialog branch of [`OverworldPhase::step`] returns before movement and
/// interaction are reached, so a B press that closes the box cannot also
/// move the player; and with no box open, B is not read at all (interaction
/// is A-only, matching `FieldInput::pressedAButton` --
/// `field_control_avatar.c:172`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn b_does_not_move_the_player_or_open_a_dialog() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    // Facing south with a clear tile below, so an un-frozen step would show.
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));
    // Bounded, like this module's other dialog loops: a regression that
    // stops B closing the box must *fail* here, not spin forever.
    let mut closed = false;
    for _ in 0..40 {
        phase.step(pressed(Buttons::B));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed, "B must close the box within the frame budget");
    assert_eq!(
        phase.player.position(),
        (7, 4),
        "the B press that closed the box must not also have moved the player"
    );

    // With no dialog open, B facing Mom must not open one -- only A
    // interacts. (2, 6) is Mom's tile; stand east of her facing west.
    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.step(pressed(Buttons::B));
    assert!(
        phase.dialog.is_none(),
        "B is not an interaction button -- upstream gates \
         TryStartInteractionScript on pressedAButton alone"
    );
    // ...and the identical press with A does open one, so the check above is
    // about the button rather than the position.
    phase.step(pressed(Buttons::A));
    assert!(phase.dialog.is_some(), "A still interacts");
}

// -- ON_TRANSITION decoration flags -----------------------------------------

/// Every [`assets::object_event_flags::DECORATION_FLAGS`] id must be one
/// [`EventData::flag_set`] accepts -- what makes
/// [`super::connections::run_on_transition_map_script`]'s `expect` unreachable rather
/// than a latent panic.
#[test]
fn every_decoration_flag_id_is_settable() {
    let mut data = EventData::new();
    for &id in assets::object_event_flags::DECORATION_FLAGS {
        assert!(
            data.flag_set(id).is_ok(),
            "{id:#x} must be an ordinary flag id"
        );
    }
    assert_eq!(
        assets::object_event_flags::DECORATION_FLAGS.len(),
        14,
        "SecretBase_EventScript_SetDecorationFlags sets FLAG_DECORATION_1..14"
    );
}

/// The transcribed [`super::connections::MAPS_THAT_SET_DECORATION_FLAGS`] list must be
/// exactly the set of bundled maps that actually carry decoration
/// placeholders -- the tripwire against the list going stale if the
/// extraction pipeline ever bundles another bedroom or secret base.
#[test]
fn the_decoration_flag_map_list_covers_every_bundled_map_with_placeholders() {
    /// The maps `crates/xtask/src/extract/mod.rs`'s `LAYOUTS` bundles --
    /// mirrored here as `crate::overworld::npc`'s own tests mirror it (this
    /// crate cannot depend on `xtask`; that module's
    /// `the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it` is
    /// the tripwire for the list itself growing).
    const BUNDLED_MAPS: [&str; 6] = [
        "MAP_LITTLEROOT_TOWN",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
    ];

    let table = assets::MapEventsTable::new();
    let mut with_placeholders: Vec<MapId> = BUNDLED_MAPS
        .iter()
        .map(|m| MapId(m))
        .filter(|map| {
            table.resolve(*map).is_ok_and(|events| {
                events
                    .object_events
                    .iter()
                    .any(|o| o.flag.starts_with("FLAG_DECORATION_"))
            })
        })
        .collect();
    with_placeholders.sort_unstable_by_key(|m| m.0);

    let mut listed = super::connections::MAPS_THAT_SET_DECORATION_FLAGS.to_vec();
    listed.sort_unstable_by_key(|m| m.0);
    assert_eq!(
        with_placeholders, listed,
        "every bundled map declaring FLAG_DECORATION_* object events must run \
         SecretBase_EventScript_SetDecorationFlags on transition, and no other"
    );
}

/// The finding-5 regression: on a fresh save the player's bedroom must be
/// walkable, not fenced in by its own decoration placeholders.
///
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F` declares twelve
/// `OBJ_EVENT_GFX_VAR_*` placeholders down the room's left column
/// (`map.json:32-175`), `(1, 2)` among them. Nothing sets
/// `FLAG_DECORATION_*` at new-game time, so once object events became solid
/// these turned into invisible walls. Upstream hides them from the map's own
/// `MAP_SCRIPT_ON_TRANSITION` (see
/// [`super::connections::run_on_transition_map_script`]), which
/// [`OverworldPhase::load_default`] now mirrors.
#[test]
fn the_bedrooms_decoration_placeholders_do_not_block_a_fresh_save() {
    const BEDROOM: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    let events = assets::MapEventsTable::new().resolve(BEDROOM).unwrap();

    let placeholder = events
        .object_events
        .iter()
        .find(|o| (o.x, o.y) == (1, 2))
        .expect("the bedroom declares a decoration placeholder at (1, 2)");
    assert_eq!(
        (placeholder.graphics_id, placeholder.flag),
        ("OBJ_EVENT_GFX_VAR_8", "FLAG_DECORATION_9"),
        "fixture precondition: its real map.json graphics id and flag"
    );

    // A phase entering the bedroom -- the same path `load_default` takes,
    // minus the pack.
    let phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        BEDROOM,
        PlayerState::new((1, 3), 3, Direction::North),
        None,
    );
    assert!(
        !engine::overworld::object_event_is_visible(placeholder, &phase.save1().event_data),
        "an empty decoration slot is the flag-SET state; entering the map \
         must have set it"
    );

    // And the tile is genuinely walkable: step north onto it.
    let mut phase = phase;
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (1, 2),
        "the placeholder's tile must be walkable on a fresh save"
    );
}

/// The same map entered *without* the on-transition script would block --
/// pinning that the test above measures the fix rather than some unrelated
/// property of the fixture (e.g. the placeholder being unreachable anyway).
#[test]
fn a_decoration_placeholder_would_block_if_its_flag_were_clear() {
    const BEDROOM: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    let events = assets::MapEventsTable::new().resolve(BEDROOM).unwrap();
    let placeholder = events
        .object_events
        .iter()
        .find(|o| (o.x, o.y) == (1, 2))
        .expect("the bedroom declares a decoration placeholder at (1, 2)");

    // A store with the decoration flags deliberately left clear -- i.e. the
    // pre-fix state, and also the state upstream reaches for a slot that
    // really does hold a decoration.
    let data = EventData::new();
    assert!(
        engine::overworld::object_event_is_visible(placeholder, &data),
        "with its flag clear the placeholder is spawned -- upstream would \
         resolve OBJ_EVENT_GFX_VAR_8 through VAR_OBJ_GFX_ID_8 and draw a real \
         decoration there"
    );

    let header = assets::MapHeaderTable::new().header(BEDROOM).unwrap();
    let scene = crate::overworld::tests::synthetic_scene(10, 10);
    let runtime = scene.runtime(BEDROOM, header, events);
    let mut player = PlayerState::new((1, 3), 3, Direction::North);
    let no_connections = |_: MapId| -> Option<(u16, u16)> { None };
    assert!(
        matches!(
            player.step(Some(Direction::North), &runtime, &no_connections, &data),
            engine::overworld::StepOutcome::Blocked { .. }
        ),
        "a spawned decoration is a solid object event -- upstream never \
         exempts one from DoesObjectCollideWithObjectAt, so a future \
         decoration-placement slice inherits this blocking behaviour"
    );
}

/// The **production** pin for the decoration-flag fix, on the acceptance
/// path itself: [`OverworldPhase::load_default`] spawns the player into
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F` -- the very room carrying the
/// twelve `OBJ_EVENT_GFX_VAR_*` placeholders -- so that constructor's own
/// `run_on_transition_map_script` call is what makes the room walkable in
/// the real game, not just in the `for_test` fixtures above.
///
/// Deleting that call from `load_default` must fail *here*: the headless
/// tests all build their phase through `for_test`, which has its own call,
/// so none of them can catch it.
///
/// Real-pack, because `load_default` is the pack-loading constructor.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn load_default_hides_the_spawn_bedrooms_decoration_placeholders() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "fixture precondition: the spawn map is the bedroom with the placeholders"
    );

    // Every flag the map's own ON_TRANSITION script sets.
    for &id in assets::object_event_flags::DECORATION_FLAGS {
        assert_eq!(
            phase.save1().event_data.flag_get(id),
            Ok(true),
            "{id:#x} must be set on entering the bedroom"
        );
    }

    // ...and therefore every placeholder is absent, by the engine's own
    // visibility query rather than by restating the flag check.
    let events = assets::MapEventsTable::new()
        .resolve(new_game::SPAWN_MAP_ID)
        .unwrap();
    let placeholders: Vec<_> = events
        .object_events
        .iter()
        .filter(|o| o.graphics_id.starts_with("OBJ_EVENT_GFX_VAR_"))
        .collect();
    assert_eq!(
        placeholders.len(),
        12,
        "the bedroom declares twelve decoration slots (DECOR_MAX_PLAYERS_HOUSE)"
    );
    for placeholder in &placeholders {
        assert!(
            !engine::overworld::object_event_is_visible(placeholder, &phase.save1().event_data),
            "{} at ({}, {}) must be absent on a fresh save",
            placeholder.graphics_id,
            placeholder.x,
            placeholder.y
        );
    }

    // And the observable consequence: the tile a placeholder stages on is
    // walkable. `(1, 2)` is `OBJ_EVENT_GFX_VAR_8`/`FLAG_DECORATION_9` at
    // elevation 3, on open floor in the real layout -- so nothing but the
    // object event could block it.
    //
    // Placed directly rather than walked from the spawn tile (which is the
    // stair warp at `SPAWN_POSITION`, so stepping off it and back would
    // leave the room): the same direct-placement shape, for the same
    // reason, as this module's real-pack Mom dialog test.
    phase.player = PlayerState::new((1, 3), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (1, 2),
        "an empty decoration slot's tile must be walkable in the real room"
    );
}

/// The same pin for the *other* production call site,
/// [`OverworldPhase::warp_to`] -- every subsequent entry into a bedroom,
/// not just the first.
///
/// Calls `warp_to` directly rather than walking the 1F stairs: it is the
/// production method (the one `step` invokes on a resolved warp), and
/// driving it straight makes the assertion about the transition itself
/// rather than about the route taken to reach it. The flags are cleared
/// first so the assertion cannot pass on the state `load_default` already
/// left behind.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_into_a_bedroom_hides_its_decoration_placeholders() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

    // Clear what `load_default` set, so only `warp_to` can restore it.
    for &id in assets::object_event_flags::DECORATION_FLAGS {
        phase.save1.event_data.flag_clear(id).unwrap();
    }
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(assets::object_event_flags::DECORATION_FLAGS[0]),
        Ok(false),
        "fixture precondition: the flags start clear for this transition"
    );

    // Warp into the bedroom (its only warp event, id 0, is the stair tile).
    phase.warp_to(new_game::SPAWN_MAP_ID, 0);
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "the warp must have landed"
    );

    for &id in assets::object_event_flags::DECORATION_FLAGS {
        assert_eq!(
            phase.save1().event_data.flag_get(id),
            Ok(true),
            "{id:#x} must be set again by the arrival transition"
        );
    }

    // Same walkability consequence as the spawn case.
    phase.player = PlayerState::new((1, 3), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(phase.player.position(), (1, 2));
}

/// The reported regression on the **production** path: a fresh run spawned
/// straight into the bedroom must not have the rival's Poké Ball staged
/// there as an invisible collider.
///
/// `(3, 4)` is `OBJ_EVENT_GFX_ITEM_BALL` at elevation `0` -- the transition
/// wildcard, so it collides with a player at *any* elevation -- on open
/// floor in the real layout, and nothing in this port draws an item ball.
/// Its `FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F_POKE_BALL` is set only
/// by the male branch of the skipped truck sequence
/// (`data/maps/InsideOfTruck/scripts.inc:31`), which
/// [`crate::new_game::init_save_blocks`] now applies.
///
/// Real-pack and driven through [`OverworldPhase::load_default`] for the
/// same reason as this module's other two production pins: the headless
/// fixtures build their save through the same constructor, so only a test
/// that walks the real spawn path proves the acceptance route is clear.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_spawn_bedroom_has_no_invisible_poke_ball_collider() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

    let ball = assets::MapEventsTable::new()
        .resolve(new_game::SPAWN_MAP_ID)
        .unwrap()
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_ITEM_BALL")
        .expect("the bedroom declares the rival's Poké Ball");
    assert_eq!(
        (ball.x, ball.y, ball.elevation),
        (3, 4, 0),
        "fixture precondition: its real map.json position, at the \
         transition-wildcard elevation that collides with anything"
    );
    assert!(
        !engine::overworld::object_event_is_visible(ball, &phase.save1().event_data),
        "a fresh male save must not have the rival's Poké Ball spawned"
    );

    // The observable consequence: the tile is walkable. (3, 5) and (3, 4)
    // are both open floor in the real layout, so only the object event
    // could have blocked it.
    phase.player = PlayerState::new((3, 5), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (3, 4),
        "the Poké Ball's tile must be walkable on a fresh save"
    );
}

// -- The bedroom bed (S-5, issue #218) --------------------------------------

/// The bed's **side columns**, which are walkable end to end and must stay
/// that way. The bed sits at layout-local `x=0..2, y=4..6` with an authored
/// `0 -> 4 -> 0` elevation run down each side column
/// (`assets::MetatileCell::from_raw`'s decode of
/// `pokeemerald/data/layouts/LittlerootTown_BrendansHouse_2F/map.bin`,
/// cross-checked directly against the real pack here). Walking straight up
/// the west column from the room's south wall, across the bed's raised
/// edge, and out its north side is `COLLISION_NONE` at *every* step, in
/// both this port and upstream: [`engine::overworld::elevation_mismatch`]
/// is byte-for-byte upstream `IsElevationMismatchAt`
/// (`event_object_movement.c:7707-7723`), and that function's only
/// mover-side input is `currentElevation` -- never `previousElevation` --
/// so the current-vs-previous split cannot change this outcome (full
/// derivation: `engine::overworld::player::PlayerState::step`'s "Elevation
/// adoption" doc section). This test pins that *parity* against the real
/// map: the escape issue #218 reports is a different step (see
/// [`bedroom_bed_center_pillow_cannot_be_crossed_lengthwise`], which pins
/// the tile that actually was permitted here and blocked upstream), and
/// "fixing" it by tightening the transition wildcard globally would break
/// both these columns, the bedroom's own stair warp (issue #163), and every
/// bridge/staircase landing built on the identical rule.
///
/// It also pins the render-side half of issue #218 directly:
/// `previous_elevation` retains the bed's raised `4` across the transition
/// tile on its far side rather than resetting to the wildcard -- the exact
/// render-selection input `crate::overworld::avatar::priority_for_elevation`
/// consumes (unit-tested there; this is the real-map source of the values
/// it's fed).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "fixture precondition: the spawn map is the bedroom carrying the bed"
    );

    // South of the bed, on open floor, already facing its west column.
    phase.player = PlayerState::new((0, 7), 3, Direction::North);

    // Each segment: the first `step` call of a fresh (non-transit) poll
    // commits the tile move immediately (StepOutcome::Advanced sets
    // position synchronously); the drain loop matches this module's other
    // multi-tile real-pack walks (e.g. `taking_the_stairs_does_not_land_...`).
    let walk_one_tile_north = |phase: &mut OverworldPhase| {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    };

    // (0,7) -> (0,6): onto the bed's south-edge transition tile.
    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 6));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 3),
        "transition wildcard adopted into elevation; previous_elevation \
         still reads the floor behind"
    );

    // (0,6) -> (0,5): onto the bed's raised elevation-4 edge itself --
    // COLLISION_NONE, not COLLISION_ELEVATION_MISMATCH: the transition
    // wildcard on the *mover's* side (elevation 0) is compatible with any
    // destination.
    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 5));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (4, 4),
        "the raised edge is real, non-transition elevation: both fields \
         adopt it"
    );

    // (0,5) -> (0,4): onto the bed's north-edge transition tile -- again
    // COLLISION_NONE: the *destination* being the wildcard is unconditionally
    // compatible, regardless of the mover's concrete elevation (4).
    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 4));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 4),
        "previous_elevation must still read the raised edge's 4 while \
         standing on the wildcard, not reset by it"
    );

    // (0,4) -> (0,3): out the bed's north side onto ordinary floor --
    // COLLISION_NONE, matching upstream exactly (see this test's own doc
    // comment for why this step is not, and must not become, the "fix").
    walk_one_tile_north(&mut phase);
    assert_eq!(
        phase.player.position(),
        (0, 3),
        "the full side-column crossing succeeds in both this port and \
         upstream -- these columns are faithful via elevation, and are not \
         where the issue's escape lives (see \
         bedroom_bed_center_pillow_cannot_be_crossed_lengthwise)"
    );
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (3, 3)
    );
}

/// The issue #218 escape itself, on the real map: the bed's **center pillow
/// tile** at layout-local `(1, 4)` carries metatile `0x284`, whose attribute
/// entry in `data/tilesets/secondary/brendans_mays_house/metatile_attributes.bin`
/// is `0x00C0` -- behavior `MB_IMPASSABLE_SOUTH_AND_NORTH`. Its collision
/// bits are `0` and its elevation (`3`) matches the floor north of it, so
/// neither of the two checks this port ran before could see it, and the
/// avatar could walk lengthwise off the bed and out through its headboard.
/// Upstream refuses both halves of that crossing inside
/// `GetCollisionAtCoords` via `IsMetatileDirectionallyImpassable`
/// (`event_object_movement.c:4663, 4715-4722`), now modeled as
/// [`engine::overworld::directionally_impassable`].
///
/// Walks the real route rather than teleporting onto the pillow: up the
/// bed's west side column to `(0, 4)` (the same walk the sibling test
/// above pins as faithful), then east onto the pillow -- which upstream
/// *does* allow, since `0xC0` walls off only the north and south edges --
/// and only then north, which must not complete. The mirror (entering the
/// pillow southward from `(1, 3)`) is checked from a placed start, since
/// `(1, 3)` is not reachable from the pillow once the fix is in.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn bedroom_bed_center_pillow_cannot_be_crossed_lengthwise() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "fixture precondition: the spawn map is the bedroom carrying the bed"
    );

    let walk = |phase: &mut OverworldPhase, buttons: Buttons| {
        phase.step(held(buttons));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    };

    // Up the west side column to the bed's north-west corner, exactly as
    // `bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation`
    // walks it.
    phase.player = PlayerState::new((0, 7), 3, Direction::North);
    for expected in [(0, 6), (0, 5), (0, 4)] {
        walk(&mut phase, Buttons::UP);
        assert_eq!(phase.player.position(), expected);
    }

    // East onto the pillow. Permitted -- `MB_IMPASSABLE_SOUTH_AND_NORTH`
    // leaves the east/west edges open, which is the only reason the tile is
    // reachable at all. No turn frame is spent: `runningState` is still
    // MOVING from the walk above, which is upstream's corner-cut
    // (CheckMovementInputNotOnBike -- PlayerState::step's "# Turn vs. step"
    // docs).
    walk(&mut phase, Buttons::RIGHT);
    assert_eq!(
        phase.player.position(),
        (1, 4),
        "the pillow tile is enterable sideways in upstream too"
    );
    assert_eq!(
        phase.player.elevation(),
        3,
        "the pillow is authored at elevation 3 -- the same as the floor \
         north of it, which is why the elevation check cannot be what \
         blocks the next step"
    );

    // North, out through the headboard: the escape this issue reports.
    // Blocked by the standing tile's own behavior. Before the fix this
    // landed on (1, 3). The first held frame is stepped explicitly so the
    // collision *outcome* is pinned too: a blocked step must cancel before
    // transit ever starts, not start-then-snap-back to the same tile.
    phase.step(held(Buttons::UP));
    assert!(
        !phase.player.in_transit(),
        "a blocked northward exit must never enter transit"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.position(),
        (1, 4),
        "MB_IMPASSABLE_SOUTH_AND_NORTH on the tile the avatar stands on \
         must stop a northward exit (IsMetatileDirectionallyImpassable's \
         gOppositeDirectionBlockedMetatileFuncs half)"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a blocked step still turns the avatar, like walking into a wall"
    );

    // The mirror: stepping *onto* the pillow from the floor north of it is
    // blocked by the destination tile's behavior instead -- same
    // no-transit collision outcome, opposite function table.
    phase.player = PlayerState::new((1, 3), 3, Direction::South);
    phase.step(held(Buttons::DOWN));
    assert!(
        !phase.player.in_transit(),
        "a blocked southward entry must never enter transit"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.position(),
        (1, 3),
        "the same tile must also refuse entry from the north \
         (IsMetatileDirectionallyImpassable's gDirectionBlockedMetatileFuncs \
         half)"
    );
}

/// The 1F half of the same regression, on the production path: after taking
/// the stairs, the player's own house must not contain the *rival's* family
/// -- no duplicated mother beside the real one, and no undrawn ninja-boy
/// blocker.
///
/// Both flags come from the male truck branch
/// (`data/maps/InsideOfTruck/scripts.inc:29-30`). Walks the real 2F -> 1F
/// stair warp rather than assigning `map_id`, so the arrival goes through
/// [`OverworldPhase::warp_to`] exactly as play would.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn taking_the_stairs_does_not_land_in_a_house_full_of_the_rivals_family() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

    // Step off the spawn tile (which *is* the stair warp) and back onto it,
    // to generate a fresh landing -- the same shape as this module's other
    // real-pack warp tests.
    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    phase.step(held(Buttons::UP));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.map_id, ONE_F, "the stairs must have landed on 1F");

    let events = assets::MapEventsTable::new().resolve(ONE_F).unwrap();
    let find = |gfx: &str| {
        events
            .object_events
            .iter()
            .find(|o| o.graphics_id == gfx)
            .unwrap_or_else(|| panic!("1F must declare a {gfx}"))
    };
    let data = &phase.save1().event_data;

    // The player's own mother is home, exactly once.
    let mom = find("OBJ_EVENT_GFX_MOM");
    assert!(
        engine::overworld::object_event_is_visible(mom, data),
        "the player's own mother must be home"
    );
    // The rival's mother is not -- this is the duplicated-mother bug.
    let rival_mom = find("OBJ_EVENT_GFX_WOMAN_4");
    assert!(
        !engine::overworld::object_event_is_visible(rival_mom, data),
        "the rival's mother must not be duplicated into the player's house"
    );
    // Nor the rival's sibling, whose sprite this port cannot draw at all --
    // so a spawned one is a pure invisible blocker.
    let sibling = find("OBJ_EVENT_GFX_NINJA_BOY");
    assert!(
        !engine::overworld::object_event_is_visible(sibling, data),
        "the rival's sibling must not be an invisible blocker at ({}, {})",
        sibling.x,
        sibling.y
    );

    // Their tiles are walkable, which is what an invisible blocker would
    // have broken. Both sit on open floor in the real 1F layout.
    for tile in [(rival_mom.x, rival_mom.y), (sibling.x, sibling.y)] {
        let (x, y) = (i32::from(tile.0), i32::from(tile.1));
        phase.player = PlayerState::new((x, y + 1), 3, Direction::North);
        phase.step(held(Buttons::UP));
        assert_eq!(
            phase.player.position(),
            (x, y),
            "({x}, {y}) must be walkable once its object event is hidden"
        );
    }
}

/// Review regression (#192): the last link of the tick wiring -- the
/// phase's own counter must actually reach
/// [`crate::overworld::OverworldScene::compose`]. The three other links
/// (increment per step, reset on load/warp, `compose(tick)` reaching
/// pixels) each have their own mutation guard above and in
/// `crate::overworld::tests`, but hard-coding tick 0 in
/// [`OverworldPhase::compose_frame`] left the whole suite green: nothing
/// joined the counter to the composition. This does -- Littleroot Town's
/// flower view (`crate::overworld::tests`' own real-pack fixture position)
/// composes different pixels at tick 60 than at tick 0, so 60 idle frames
/// must change the phase's composed output.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn idle_frames_animate_the_composed_tileset_pixels() {
    let town = assets::MapId("MAP_LITTLEROOT_TOWN");
    let mut phase = OverworldPhase::for_test(
        crate::overworld::load_room(
            town,
            crate::overworld::PlayerCharacter::Brendan,
            &engine::event_data::EventData::new(),
        )
        .expect("run `cargo xtask extract` first"),
        town,
        PlayerState::new((10, 17), 3, Direction::South),
        None,
    );

    let base = phase.compose_frame();
    for _ in 0..60 {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.tick, 60, "60 idle frames, 60 ticks");
    let animated = phase.compose_frame();
    assert_ne!(
        &*base, &*animated,
        "60 idle frames must animate the flower tiles through the phase's own \
         tick -- if this fails, `compose_frame` is not passing `self.tick`"
    );
}

/// The tile row Route 101's own extracted layout makes solid tall grass:
/// `y == 4`, `x` in `0..=5` (the same real data
/// [`crate::flow::wild_encounter::tests`]' own real-pack lane walks, one row
/// over). Six tiles in a straight line is exactly what the two
/// immunity-window tests below need -- four immune steps and then a fifth
/// that really rolls, with no direction change to complicate the frame
/// counting.
const ROUTE_101_GRASS_ROW: [(i32, i32); 6] = [(0, 4), (1, 4), (2, 4), (3, 4), (4, 4), (5, 4)];

/// Route 101's own elevation on [`ROUTE_101_GRASS_ROW`].
const ROUTE_101_GRASS_ELEVATION: u8 = 3;

/// A seed whose first four draws are all non-zero, so "the stream never
/// moved" and "the stream moved" are distinguishable by state alone. Any
/// seed would do; this is [`crate::flow::wild_encounter::tests`]' own
/// `ENCOUNTER_SEED`, reused so the two files' scenarios stay comparable.
const IMMUNITY_SEED: u32 = 17;

/// Walk one whole tile east and let its walk animation drain, so the frame
/// the encounter roll happens on (the drain frame -- `OverworldPhase::step`'s
/// "Warp timing" docs) is included.
fn walk_one_tile_east(phase: &mut OverworldPhase) {
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
}

/// Drive `state` through one more step onto tall grass against Route 101's
/// *real* wild table, reporting whether that step touched `rng`.
///
/// The two tests below use this to assert the shape of the window a map
/// transition restarts: four steps that draw nothing at all, then one that
/// does. It goes through [`super::OverworldPhase::wild`] rather than a fresh
/// [`engine::overworld::wild_encounter::WildEncounterState`] precisely
/// because the claim under test is about *the phase's own* state after the
/// transition, not about the counter in the abstract.
fn grass_step_draws(
    state: &mut engine::overworld::wild_encounter::WildEncounterState,
    rng: &mut Rng,
) -> bool {
    let header = assets::WildEncounterTable::new().get_by_map(MapId("MAP_ROUTE101"));
    let before = rng.state();
    state.check_standard_wild_encounter(MB_TALL_GRASS, header, rng);
    rng.state() != before
}

/// `RestartWildEncounterImmunitySteps` at upstream's `LoadMapFromWarp` call
/// site (`src/overworld.c:850`), pinned through the whole phase: a warp must
/// buy four fresh encounter-free steps, however many the player had already
/// spent before taking it.
///
/// Deleting `self.wild.restart_immunity_steps()` from
/// [`OverworldPhase::warp_to`] fails here -- the counter stays at
/// [`WILD_ENCOUNTER_IMMUNITY_STEPS`] and the very first grass step after the
/// warp draws.
///
/// The four-silent-steps tail is driven against Route 101's table through
/// the phase's own [`OverworldPhase::wild`] rather than walked, because no
/// warp this port can resolve lands anywhere with a wild table at all: the
/// destination is Brendan's house, whose `gWildMonHeaders` entry does not
/// exist, so walking it would assert nothing.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_restarts_the_wild_encounter_immunity_window() {
    // A real `map_id` with a real wild table over a synthetic open room, so
    // the pre-warp steps are ordinary walking on ordinary ground.
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        MapId("MAP_ROUTE101"),
        PlayerState::new((2, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);

    // Spend the window the phase started with.
    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        walk_one_tile_east(&mut phase);
    }
    assert_eq!(
        phase.wild.immunity_steps(),
        WILD_ENCOUNTER_IMMUNITY_STEPS,
        "four ordinary steps must exhaust the post-transition window"
    );

    phase.warp_to(ONE_F, 1);
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "a warp is a full map load -- LoadMapFromWarp calls \
         RestartWildEncounterImmunitySteps (overworld.c:850)"
    );

    // And the window it just granted really is four RNG-silent steps.
    let mut rng = Rng::new(IMMUNITY_SEED);
    for step in 1..=WILD_ENCOUNTER_IMMUNITY_STEPS {
        assert!(
            !grass_step_draws(&mut phase.wild, &mut rng),
            "grass step {step} after a warp must be immune, and immune means RNG-silent"
        );
    }
    assert!(
        grass_step_draws(&mut phase.wild, &mut rng),
        "the fifth step is out of the window and rolls for real"
    );
}

/// The same claim at upstream's *other* map-transition call site,
/// `LoadMapFromCameraTransition` (`src/overworld.c:800`) -- and this one is
/// walked end to end, because a connection crossing really can land the
/// player in grass.
///
/// Deleting `self.wild.restart_immunity_steps()` from
/// [`OverworldPhase::cross_connection`] fails here: the counter is still
/// exhausted on arrival and the very first grass step draws.
///
/// The player is repositioned onto [`ROUTE_101_GRASS_ROW`] after the real
/// crossing rather than walked there: the crossing lands at `(10, 19)` on
/// Route 101's south edge and the nearest tall grass is eleven tiles away,
/// which would spend the whole window before reaching it. Everything the
/// test actually asserts -- the counter at the crossing, and the four silent
/// steps after it -- is untouched by where the player is standing.
///
/// **`VAR_ROUTE101_STATE` is pre-set to its spent value (issue #231).**
/// `(10, 19)` is also the rescue coord-event trigger tile, and a *live*
/// trigger there would legitimately keep this step from ever reaching
/// `CheckStandardWildEncounter` at all -- upstream's `TryStartStepBasedScript`
/// returns TRUE at `:155-161`, two lines above it, so the immunity counter
/// (decremented inside that function, `:668-686`) would not move. That is the
/// coord event's behaviour, pinned in its own section further down this file;
/// asserting it here too would only turn this test's own subject -- the
/// crossing's immunity restart -- into a hostage of an unrelated slice. The
/// var starts at the post-rescue `2` instead, exactly the state upstream
/// leaves behind once the Birch rescue has run, which makes `(10, 19)` the
/// ordinary landing tile this test has always meant it to be.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn crossing_a_map_connection_restarts_the_wild_encounter_immunity_window() {
    let littleroot = MapId("MAP_LITTLEROOT_TOWN");
    let route101 = MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let mut phase = OverworldPhase::for_test(
        scene,
        littleroot,
        PlayerState::new((10, 4), 3, Direction::North),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);
    // The rescue trigger on the landing tile is already spent (doc comment).
    phase
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");

    // Four ordinary steps north up Littleroot's own walkable column. Its map
    // has no wild table, so they draw nothing -- but upstream's
    // sWildEncounterImmunitySteps counts steps, not encounters, so the
    // window is spent all the same.
    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    }
    assert_eq!(phase.player.position(), (10, 0));
    assert_eq!(phase.wild.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);
    assert_eq!(
        phase.rng.state(),
        Rng::new(IMMUNITY_SEED).state(),
        "a map with no wild table draws nothing, window or no window"
    );

    // The fifth step walks off Littleroot's north edge into Route 101 -- the
    // real crossing, through `PlayerState::step` and `cross_connection`.
    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, route101, "the crossing must have landed");
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "a connection crossing calls RestartWildEncounterImmunitySteps too \
         (LoadMapFromCameraTransition, overworld.c:800)"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.wild.immunity_steps(),
        1,
        "the crossing step's own landing then spends the first of the four it was just \
         granted -- upstream reaches CheckStandardWildEncounter for that step too, the \
         crossing having happened inside it"
    );

    // Stand in Route 101's own tall grass (doc comment) and walk it: three
    // immune steps are left, and the fourth rolls.
    phase.player = PlayerState::new(
        ROUTE_101_GRASS_ROW[0],
        ROUTE_101_GRASS_ELEVATION,
        Direction::East,
    );
    phase.pending_landing = None;
    let remaining = usize::from(WILD_ENCOUNTER_IMMUNITY_STEPS - phase.wild.immunity_steps());
    for (index, tile) in ROUTE_101_GRASS_ROW
        .iter()
        .copied()
        .enumerate()
        .take(remaining + 1)
        .skip(1)
    {
        walk_one_tile_east(&mut phase);
        assert_eq!(phase.player.position(), tile);
        assert_eq!(
            phase.rng.state(),
            Rng::new(IMMUNITY_SEED).state(),
            "grass step {index} is inside the window the crossing restarted, so it must not \
             draw at all"
        );
        assert!(phase.wild_battle.is_none());
    }
    assert_eq!(phase.wild.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);

    walk_one_tile_east(&mut phase);
    assert_ne!(
        phase.rng.state(),
        Rng::new(IMMUNITY_SEED).state(),
        "the step past the window's end rolls for real"
    );
}

// ---------------------------------------------------------------------------
// The Route 101 scripted first-battle trigger (issue #231).
// ---------------------------------------------------------------------------

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`), transcribed
/// independently of `first_battle_trigger`'s own (private) copy so these
/// tests pin the real upstream id rather than restating that module's.
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// One of the real Route 101 rescue coord-event trigger tiles
/// (`pokeemerald/data/maps/Route101/map.json`'s `coord_events`, elevation 3)
/// -- `first_battle_trigger`'s own module docs have the full citation.
const ROUTE_101_TRIGGER_TILE: (i32, i32) = (10, 19);

/// `ROUTE_101_TRIGGER_TILE`'s own elevation.
const ROUTE_101_TRIGGER_ELEVATION: u8 = 3;

/// A synthetic, fully open (no collision, elevation 3 throughout) room named
/// `MAP_ROUTE101`: the layout grid is fabricated, but `map_id` still resolves
/// to Route 101's *real* generated [`assets::MapEventsTable`] entry -- the
/// same "real events over a synthetic grid" split
/// `crate::flow::wild_encounter::tests::route_101_phase` uses for the wild
/// table, here for the coord-event trigger instead. 25x25 is large enough to
/// place the player next to [`ROUTE_101_TRIGGER_TILE`].
fn route_101_trigger_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(25, 25),
        MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

/// [`route_101_trigger_phase`] with metatile behaviors painted onto named
/// cells -- the precedence tests below need the trigger tile to *also* be
/// something (tall grass, a door, an arrow), which is the only way this
/// port's own precedence between the coord event and the rest of
/// `ProcessPlayerFieldInput` becomes observable at all.
fn route_101_trigger_phase_with_special_tiles(
    player: PlayerState,
    specials: &[((u16, u16), u8)],
) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles(25, 25, specials),
        MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

/// [`ROUTE_101_TRIGGER_TILE`] as the `(u16, u16)` the synthetic-scene
/// fixtures index cells by.
fn trigger_tile_cell() -> (u16, u16) {
    let (x, y) = ROUTE_101_TRIGGER_TILE;
    (
        u16::try_from(x).expect("the trigger tile is inside the fixture"),
        u16::try_from(y).expect("the trigger tile is inside the fixture"),
    )
}

/// Walk `tiles` whole tiles east, each with its own drain frame
/// ([`walk_one_tile_east`]).
fn walk_east(phase: &mut OverworldPhase, tiles: usize) {
    for _ in 0..tiles {
        walk_one_tile_east(phase);
    }
}

/// Step 1 of the upstream chain (`first_battle_trigger`'s module docs):
/// `Route101_OnFrame`'s guard bumps a fresh save's `VAR_ROUTE101_STATE` from
/// `0` to `1` the moment the player is on the map, or this slice's trigger
/// (gated on that same value) could never gate open in real play.
#[test]
fn entering_route_101_bumps_the_fresh_save_rescue_var_to_one() {
    let phase = route_101_trigger_phase(PlayerState::new((0, 0), 3, Direction::East));
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "a fresh save's VAR_ROUTE101_STATE must read 1 on Route 101, not its 0 default"
    );
}

/// Entering any *other* map must never touch `VAR_ROUTE101_STATE` -- the
/// on-frame bump is gated on the map, matching upstream's own map-scoped
/// `MAP_SCRIPT_ON_FRAME_TABLE`.
#[test]
fn entering_a_different_map_leaves_the_rescue_var_at_its_fresh_save_zero() {
    let phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((0, 0), 3, Direction::East),
        None,
    );
    assert_eq!(phase.save1.event_data.var_get(VAR_ROUTE101_STATE), Ok(0));
}

/// The entry bump's fresh-save behavior *and* its documented idempotency,
/// pinned through the real continue constructor: `OverworldPhase::from_saved`
/// must run `sync_route_101_state_on_entry` exactly as `Self::new`,
/// `Self::warp_to`, and `Self::cross_connection` already do, so a fresh save
/// (var `0`) resumed on Route 101 still gets bumped to `1`, and every later
/// story state (`1`, `2`, `3`) resumes unchanged -- without that guard,
/// re-entering the map on a post-rescue save would re-open the trigger and
/// re-fight the scripted Zigzagoon (issue #231 review).
#[test]
fn continuing_on_route_101_only_advances_the_fresh_rescue_state() {
    for (saved_state, expected_state) in [(0, 1), (1, 1), (2, 2), (3, 3)] {
        let (mut block1, block2) = new_game::init_save_blocks_for_new_game();
        block1
            .event_data
            .var_set(VAR_ROUTE101_STATE, saved_state)
            .unwrap();
        let resumed = OverworldPhase::from_saved(
            crate::overworld::tests::synthetic_scene(25, 25),
            MapId("MAP_ROUTE101"),
            block1,
            block2,
        );
        assert_eq!(
            resumed.save1.event_data.var_get(VAR_ROUTE101_STATE),
            Ok(expected_state),
            "a saved VAR_ROUTE101_STATE of {saved_state} must resume as {expected_state}"
        );
    }
}

/// The acceptance path this issue exists for: stepping onto the real Route
/// 101 rescue trigger tile, with the var in its pre-rescue state, starts the
/// scripted first battle through the real [`OverworldPhase::step`] path --
/// not [`crate::flow::first_battle::start_first_battle`] called directly.
/// Pins (a) from the issue's test list.
#[test]
fn stepping_onto_the_route_101_trigger_tile_starts_the_scripted_first_battle() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "entering the map already primed the pre-rescue state"
    );

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.wild_battle.is_none(),
        "this trigger must never route through the ordinary wild-encounter path"
    );
    let battle = phase
        .first_battle
        .as_ref()
        .expect("the rescue trigger must start the scripted first battle");
    assert_eq!(
        battle.enemy().species(),
        assets::SpeciesId(288),
        "SPECIES_ZIGZAGOON"
    );
    assert_eq!(battle.enemy().level(), 2);
    assert!(
        phase.party_lead.is_none(),
        "the lead mon moved into the battle"
    );
}

/// Route 101's second rescue coord event must trigger independently of the
/// first one: approach `(11, 19)` from the east and exercise the complete
/// [`OverworldPhase::step`] path through the landing drain frame.
#[test]
fn stepping_west_onto_the_second_route_101_trigger_tile_starts_the_scripted_first_battle() {
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (12, 19),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::West,
    ));
    phase.party_lead = Some(new_game::provisional_starter());

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::LEFT));
    }

    assert_eq!(phase.player.position(), (11, 19));
    assert!(
        phase.first_battle.is_some(),
        "the second rescue coord event must start the scripted first battle"
    );
}

/// The no-party-lead arm of `super::first_battle_trigger`'s
/// `begin_first_battle`, which its own doc comment claims outright ("The
/// trigger is still consumed in that case, exactly as it is upstream -- the
/// coord event fires, the cutscene it stands in for runs, and the tile is
/// spent whether or not this port could build a battle out of it") and which
/// `walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`'s
/// own doc comment relies on to explain why that pack-only walk stays quiet.
/// Nothing else pins it: every other trigger test assigns a lead, so moving
/// the `var_set` below `begin_first_battle`'s `let Some(lead) = ... else`
/// early return survives the whole suite without this.
#[test]
fn the_trigger_is_consumed_even_when_there_is_no_party_lead_to_fight_with() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    // Deliberately no `party_lead`: `OverworldPhase::for_test` leaves it
    // `None`, which is the bare-test-phase case that arm exists for.
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.first_battle.is_none(),
        "there was no lead mon to build a battle out of"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "the coord event still fired, so the trigger is spent -- upstream's own \
         setvar VAR_ROUTE101_STATE, 2 runs mid-cutscene, long before any battle"
    );
}

/// A step that lands elsewhere on Route 101 -- one tile short of the trigger
/// -- must not start anything: the coord-event lookup is exact-position, not
/// a nearby-tile fuzzy match.
#[test]
fn a_step_that_does_not_land_on_the_trigger_tile_starts_nothing() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 2, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx - 1, ty));
    assert!(phase.first_battle.is_none());
    assert!(phase.wild_battle.is_none());
}

/// (b) from the issue's test list: the battle the trigger starts plays to a
/// real terminal outcome through the per-frame driver
/// ([`OverworldPhase::step`] -> [`crate::flow::first_battle::advance_first_battle`]),
/// freezing the overworld exactly as an ordinary wild battle does, and hands
/// the player's mon back on the frame it ends.
#[test]
fn the_scripted_first_battle_plays_to_a_terminal_outcome_and_hands_the_lead_back() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());
    walk_one_tile_east(&mut phase);
    assert!(phase.first_battle.is_some(), "setup: the trigger must fire");

    let frozen_at = phase.player.position();
    let mut frames = 0;
    while phase.first_battle.is_some() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(
            frames < 500,
            "the headless first-battle driver must terminate"
        );
        assert_eq!(
            phase.player.position(),
            frozen_at,
            "the overworld is frozen while the scripted first battle runs"
        );
    }
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the battle writes the lead mon back on the frame it ends");
    assert_eq!(lead.species(), new_game::PROVISIONAL_STARTER_SPECIES);
    assert_eq!(
        phase.first_battle_outcome,
        Some(BattleOutcome::PlayerWon),
        "an emptied battle slot must retain the real terminal outcome"
    );
}

/// (c) from the issue's test list: by the time the battle ends,
/// `VAR_ROUTE101_STATE` has advanced past the trigger's own gate (upstream's
/// own `setvar VAR_ROUTE101_STATE, 2`, written at trigger time --
/// `first_battle_trigger`'s "When the var advances" section), so stepping
/// onto the same tile again starts nothing -- no second battle, and the roll
/// doesn't silently fall through to an ordinary wild encounter either.
#[test]
fn after_the_battle_ends_the_trigger_tile_cannot_refire() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());
    walk_one_tile_east(&mut phase);
    let mut frames = 0;
    while phase.first_battle.is_some() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 500, "setup: the driver must terminate");
    }
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "the fired trigger advanced the var, so it can't refire"
    );

    // Walk off the tile and back onto it -- a fresh completed step, the same
    // kind that fired the trigger the first time.
    phase.player = PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East);
    phase.pending_landing = None;
    let rng_before = phase.rng.state();
    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.first_battle.is_none(),
        "the trigger must not refire once VAR_ROUTE101_STATE has advanced"
    );
    assert!(
        phase.wild_battle.is_none(),
        "the tile must not fall through to an ordinary wild-encounter roll either"
    );
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "ordinary ground (this synthetic room's own default) draws nothing either way"
    );
}

/// (e) from the issue's test list: the trigger draws off the phase's one
/// shared stream, in [`crate::flow::first_battle::start_first_battle`]'s own
/// documented order -- stepping onto the tile must produce *exactly* the
/// battle a direct call to that function, off an identically-seeded
/// reference generator, would.
#[test]
fn the_trigger_draws_off_the_phases_single_shared_stream() {
    const SEED: u32 = 4242;
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(SEED);
    let lead = new_game::provisional_starter();
    phase.party_lead = Some(lead.clone());

    walk_one_tile_east(&mut phase);
    let battle = phase.first_battle.as_ref().expect("the trigger must fire");

    // Replay the identical construction off an independent, identically
    // seeded generator: no overworld step precedes it, so if the phase drew
    // anything of its own before handing off, the enemy's rolled identity
    // (and the stream's final position) would disagree.
    let mut reference = Rng::new(SEED);
    let expected = crate::flow::first_battle::start_first_battle(lead, &mut reference)
        .expect("the same construction must succeed off a reference stream");
    assert_eq!(battle.enemy().personality(), expected.enemy().personality());
    assert_eq!(battle.enemy().ivs(), expected.enemy().ivs());
    assert_eq!(
        phase.rng.state(),
        reference.state(),
        "the trigger must draw exactly as many values as start_first_battle documents -- no \
         more, no fewer"
    );
}

/// The `cross_connection` half of [`OverworldPhase`]'s map-entry bump
/// (`super::first_battle_trigger::sync_route_101_state_on_entry`'s own
/// "called at every one of `OverworldPhase`'s map-entry points" claim):
/// walking into Route 101 over its map-edge connection primes
/// `VAR_ROUTE101_STATE` from a fresh save's `0` to `1` **on arrival**, before
/// the crossing step's own walk animation has drained and therefore before
/// the rescue trigger this slice gates on that value is ever tested.
///
/// Real-pack, and unavoidably so: `OverworldPhase::cross_connection` loads
/// the entered map's room through `crate::overworld::load_room` and bails out
/// before any arrival effect when no pack is extracted, so no synthetic
/// fixture in this file can reach the call site at all.
///
/// Deleting `cross_connection`'s `sync_route_101_state_on_entry` call fails
/// here directly (the var stays `0`), and fails
/// `real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle`
/// below as a consequence -- the trigger's gate never opens, so no battle
/// starts. This test is the direct one: it isolates the arrival effect from
/// the trigger that consumes it.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_crossing_into_route_101_primes_the_rescue_var_on_arrival() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    // One tile south of Littleroot's own last interior row (the walkable
    // `x = 10` column `walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`
    // documents), so exactly two steps reach the crossing.
    let player = PlayerState::new((10, 1), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, littleroot, player, None);
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(0),
        "setup: a phase built on Littleroot leaves the fresh-save value alone"
    );

    phase.step(held(Buttons::UP));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.player.position(), (10, 0));
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(0),
        "still on Littleroot -- the bump is map-scoped"
    );

    // The step that walks off the north edge: the crossing itself.
    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, route101, "the crossing must have landed");
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "cross_connection runs Route101_OnFrame's own guard on arrival, so the rescue \
         trigger's gate is already open when this step's landing drains"
    );
    assert!(
        phase.first_battle.is_none(),
        "the crossing step is still mid-animation -- the coord event is only tested once \
         its landing drains"
    );
}

/// The end-to-end acceptance path against the real extracted pack: walking
/// off Littleroot Town's north edge crosses into Route 101 and lands *exactly*
/// on the real rescue coord-event trigger tile
/// (`walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`
/// above already pins the landing itself at `(10, 19)`), which must start the
/// scripted first battle and play it to a real outcome through
/// [`OverworldPhase::step`] alone -- no direct call into
/// [`crate::flow::first_battle`] anywhere in this test.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let player = PlayerState::new((10, 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, littleroot, player, None);
    // The production stand-in for the un-ported Birch-bag handout
    // (`crate::new_game::provisional_starter`'s own docs) -- without it
    // there would be nothing to fight with, exactly as `OverworldPhase::load_default`
    // assigns it before the player can ever reach the overworld for real.
    phase.party_lead = Some(new_game::provisional_starter());

    // Two ordinary steps north to the edge, then the crossing step itself,
    // then its own walk animation drains -- the same setup
    // `walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`
    // uses, continued one step further into the rescue trigger.
    for _ in 0..3 {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    }

    assert_eq!(
        phase.map_id, route101,
        "the third step must have crossed into Route 101"
    );
    assert_eq!(
        phase.player.position(),
        (10, 19),
        "Route 101's real south-edge landing tile is the real rescue trigger tile"
    );
    assert!(phase.wild_battle.is_none());
    let battle = phase
        .first_battle
        .as_ref()
        .expect("landing on the real trigger tile must start the scripted first battle");
    assert_eq!(battle.enemy().species(), assets::SpeciesId(288));
    assert_eq!(battle.enemy().level(), 2);

    // Play it out through the real per-frame driver.
    let mut frames = 0;
    while phase.first_battle.is_some() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(
            frames < 500,
            "the headless first-battle driver must terminate"
        );
    }
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the battle writes the lead mon back once it ends");
    assert_eq!(lead.species(), new_game::PROVISIONAL_STARTER_SPECIES);
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "the real trigger tile must be consumed once the battle ends"
    );
}

/// Review regression (#231, finding 1): an **aborted** first battle must
/// consume the trigger just the same.
///
/// `crate::flow::first_battle::advance_first_battle`'s own doc comment spells
/// the abort contract out — a turn the engine cannot play (here: a lead whose
/// slot 0 has no PP left, `battle::BattleError::NoPpRemaining(0)` out of
/// `Battle::take_turn`'s pre-draw validation) empties the slot, writes the
/// lead back, and returns **`None`**, never an outcome. An earlier revision
/// of this slice advanced `VAR_ROUTE101_STATE` only on `Some(outcome)`, which
/// left the var at `1` on exactly this path and the coord-event tile live, so
/// the next step onto it started the whole thing over. The var now moves at
/// trigger time, upstream's own ordering (`scripts.inc:40`, mid-cutscene) —
/// see `super::first_battle_trigger`'s "When the var advances" section.
#[test]
fn an_aborted_first_battle_still_consumes_the_route_101_trigger() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    // Prove beginning this attempt clears stale terminal state rather than
    // letting its later abort masquerade as a completed battle.
    phase.first_battle_outcome = Some(BattleOutcome::PlayerWon);
    // Drain slot 0 through the same accessor the turn engine spends PP with,
    // rather than reaching into the struct -- `crate::flow::first_battle`'s
    // own abort test does it this way too.
    let mut lead = new_game::provisional_starter();
    let starting_pp = lead.moves()[0].pp;
    assert!(starting_pp > 0, "a freshly built starter starts with PP");
    for _ in 0..starting_pp {
        lead.deduct_pp(0)
            .expect("draining a slot that still has PP");
    }
    phase.party_lead = Some(lead);

    walk_one_tile_east(&mut phase);
    assert!(
        phase.first_battle.is_some(),
        "setup: the trigger must fire and build a battle -- the abort happens a frame later"
    );
    assert_eq!(
        phase.first_battle_outcome, None,
        "starting a new fight clears the previous terminal outcome"
    );

    // One driver frame: the turn fails pre-draw, so the battle ends with no
    // outcome at all.
    phase.step(held(Buttons::RIGHT));
    assert!(
        phase.first_battle.is_none(),
        "setup: the aborted battle must have emptied the slot"
    );
    assert!(
        phase.party_lead.is_some(),
        "setup: the abort writes the lead back all the same"
    );
    assert_eq!(
        phase.first_battle_outcome, None,
        "an abort must not manufacture or retain a terminal outcome"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "an abort produces no outcome, and the trigger must be consumed anyway"
    );

    // Walk off the tile and back onto it -- the same fresh completed step
    // that fired the trigger the first time.
    phase.player = PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East);
    phase.pending_landing = None;
    let rng_before = phase.rng.state();
    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.first_battle.is_none(),
        "an aborted battle must not leave the rescue trigger live"
    );
    assert!(phase.wild_battle.is_none());
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "a spent trigger draws nothing -- not even start_first_battle's construction draws"
    );
}

/// Review regression (#231, finding 2a): the trigger **suppresses the
/// wild-encounter roll** on the frame it fires.
///
/// Upstream reaches `CheckStandardWildEncounter` (`field_control_avatar.c:162`)
/// only by falling through `TryStartStepBasedScript` (`:155-161`), which
/// returns `TRUE` the moment `TryStartCoordEventScript` (`:485-486`) fires —
/// so a coord-event frame never rolls. This is the one arm of that precedence
/// bundled Route 101 data can actually express: the rescue tile is paintable
/// as tall grass, and Route 101's real land table is fightable, so absent the
/// suppression the very same step would draw.
///
/// Pinned two ways, because a suppressed roll is the absence of an effect:
/// the stream must sit exactly where `start_first_battle` alone left it, and
/// `CheckStandardWildEncounter`'s `sPrevMetatileBehavior` side effect
/// (`:668-686`) must be untouched. The immunity counter cannot serve as a
/// third pin -- a fired roll zeroes it (saturation) and `begin_first_battle`'s
/// `battle_setup.c:941` restart zeroes it too, so its `0` here pins the
/// *restart*, not the suppression. The control run at the end proves the
/// fixture really would have rolled.
#[test]
fn the_route_101_trigger_suppresses_the_wild_encounter_roll_on_its_own_tile() {
    // `crate::flow::wild_encounter::tests`' own `ENCOUNTER_SEED`: its first
    // draws make the first *rolled* grass step produce a real encounter, so
    // the control below is unambiguous.
    const SEED: u32 = 17;
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let grass = [(trigger_tile_cell(), MB_TALL_GRASS)];

    let mut phase = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 5, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &grass,
    );
    phase.rng = Rng::new(SEED);
    phase.party_lead = Some(new_game::provisional_starter());
    // Four ordinary steps burn the whole post-transition immunity window
    // (`WILD_ENCOUNTER_IMMUNITY_STEPS`) without drawing, so the fifth -- the
    // one onto the trigger tile -- is a step the roll would really see.
    walk_east(&mut phase, 4);
    assert_eq!(
        phase.wild.immunity_steps(),
        WILD_ENCOUNTER_IMMUNITY_STEPS,
        "setup: the immunity window must be spent before the trigger step"
    );
    assert_eq!(
        phase.rng.state(),
        Rng::new(SEED).state(),
        "setup: immune steps draw nothing, so the stream is still untouched"
    );

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.first_battle.is_some(),
        "the coord event still fires on a tile that is also grass"
    );
    assert!(
        phase.wild_battle.is_none(),
        "and the grass roll it outranks must never have happened"
    );
    let mut reference = Rng::new(SEED);
    crate::flow::first_battle::start_first_battle(new_game::provisional_starter(), &mut reference)
        .expect("the same construction must succeed off a reference stream");
    assert_eq!(
        phase.rng.state(),
        reference.state(),
        "the frame's only draws are start_first_battle's -- an encounter roll would add its \
         own AllowWildCheckOnNewMetatile draw ahead of them"
    );
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "CB2_StartFirstBattle's RestartWildEncounterImmunitySteps (battle_setup.c:941) \
         must reset the spent window on the way into the fight (this value cannot \
         distinguish suppression -- the stream assertion above owns that)"
    );
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_NORMAL,
        "sPrevMetatileBehavior must still read the ordinary ground of the step before"
    );

    // Control: the identical walk with the trigger already spent
    // (`VAR_ROUTE101_STATE == 2`) really does roll on that same tile, so the
    // assertions above are about a suppressed roll rather than a fixture that
    // could never have rolled.
    let mut control = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 5, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &grass,
    );
    control
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    control.rng = Rng::new(SEED);
    control.party_lead = Some(new_game::provisional_starter());
    walk_east(&mut control, 5);
    assert!(
        control.first_battle.is_none(),
        "control: a spent trigger must not fire"
    );
    assert!(
        control.wild_battle.is_some(),
        "control: the same grass tile really does roll an encounter when nothing outranks it"
    );
    assert_eq!(
        control.wild.prev_metatile_behavior(),
        MB_TALL_GRASS,
        "control: and CheckStandardWildEncounter really does record the tile it saw"
    );
}

/// Review regression (#231, finding 2b): the trigger also outranks the
/// door-warp check (`field_control_avatar.c:155-161`, after
/// `TryStartCoordEventScript` inside the same `TryStartStepBasedScript`) and
/// the arrow poll (`:164-168`) — and *why that half cannot be driven to a
/// real warp on this map*.
///
/// Both warp paths need a `warp_events` entry at the tile
/// (`engine::overworld::trigger_door_warp`/`trigger_arrow_warp` each look one
/// up after the behavior check), and **Route 101 declares none at all** — a
/// route is entered by walking, not through a door. So no test over bundled
/// data can make a warp and this coord event contend for the same frame; the
/// precedence is encoded and documented rather than pinned, exactly as
/// `crate::flow::wild_encounter::arrow_poll_open` already documents for its
/// own equally-unreachable encounter arm.
///
/// What this test *does* pin is that reachability argument itself — the
/// emptiness of `warp_events` is an assertion, so a future Route 101 warp
/// fails here and forces the real precedence test — plus the behavior half
/// the fixture can express: a door-shaped and an arrow-shaped metatile
/// behavior painted onto the trigger tile change nothing about the frame.
#[test]
fn route_101_has_no_warp_events_so_the_trigger_can_never_race_one() {
    let route101 = MapId("MAP_ROUTE101");
    let events = assets::MapEventsTable::new()
        .resolve(route101)
        .expect("Route 101 resolves in the generated map-events table");
    assert!(
        events.warp_events.is_empty(),
        "Route 101 has no warp events, so the door/arrow arms of the trigger's precedence \
         are unreachable over bundled data -- if that ever changes, pin them for real"
    );

    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    for behavior in [MB_ANIMATED_DOOR, MB_EAST_ARROW_WARP] {
        let mut phase = route_101_trigger_phase_with_special_tiles(
            PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
            &[(trigger_tile_cell(), behavior)],
        );
        phase.rng = Rng::new(4242);
        phase.party_lead = Some(new_game::provisional_starter());

        walk_one_tile_east(&mut phase);

        assert_eq!(
            phase.map_id, route101,
            "behavior {behavior:#04x}: no warp may fire, so the map must not change"
        );
        assert_eq!(
            phase.player.position(),
            (tx, ty),
            "behavior {behavior:#04x}: the player stays on the trigger tile"
        );
        assert!(
            phase.first_battle.is_some(),
            "behavior {behavior:#04x}: the coord event fires regardless of the tile's behavior"
        );
    }
}

/// Review regression (#231, finding 3): `Route101_EventScript_PreventExitSouth`'s
/// own coord event at `(10, 18)` must never start a battle — and the only
/// thing standing between it and one is the **script-name** check in
/// `super::first_battle_trigger::OverworldPhase::first_battle_trigger_at`.
///
/// Everything else about that event matches: it is a `Trigger` on
/// `"VAR_ROUTE101_STATE"` at elevation 3, its `var_value` is `2` — the exact
/// value this cut leaves behind once the rescue trigger is consumed — and it
/// sits one tile north of the rescue tile, so the player really does stand on
/// it, with the var really reading `2`, the moment they walk back south.
/// Dropping the `script != TRIGGER_SCRIPT` half of that check restarts the
/// Zigzagoon fight there.
#[test]
fn the_prevent_exit_coord_events_never_start_a_battle() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 1, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.rng = Rng::new(4242);
    phase.party_lead = Some(new_game::provisional_starter());
    walk_one_tile_east(&mut phase);
    let mut frames = 0;
    while phase.first_battle.is_some() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 500, "setup: the driver must terminate");
    }
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(2),
        "setup: the consumed trigger leaves exactly the value PreventExit* gates on"
    );

    // North onto `(10, 18)` -- `Route101_EventScript_PreventExitSouth`'s own
    // coord-event tile.
    let rng_before = phase.rng.state();
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.player.position(),
        (tx, ty - 1),
        "setup: the step must land on the PreventExitSouth coord event"
    );
    assert!(
        phase.first_battle.is_none(),
        "a PreventExit* coord event is not the rescue trigger, whatever its var says"
    );
    assert!(phase.wild_battle.is_none());
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "and nothing was constructed off the shared stream either -- start_first_battle \
         draws before it can fail"
    );
}

/// The other half of the reachability argument
/// `route_101_has_no_warp_events_so_the_trigger_can_never_race_one` starts:
/// the trigger's "discards a same-frame interaction" arm
/// (`crate::flow::wild_encounter::field_input_consumed`, upstream `:172`)
/// cannot be driven either, because no Route 101 object event stands next to
/// a rescue tile for an A press to find.
///
/// Route 101's six object events sit at `(16, 8)`, `(9, 13)`, `(7, 14)`,
/// `(10, 13)`, `(5, 11)` and `(2, 13)` -- the nearest is five rows north of
/// the trigger row. Asserted rather than asserted-in-a-comment so a future
/// object-event addition next to the rescue tiles fails here and forces the
/// real precedence test to be written.
#[test]
fn no_route_101_object_event_stands_beside_the_rescue_trigger_tiles() {
    let events = assets::MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .expect("Route 101 resolves in the generated map-events table");
    for (tx, ty) in [ROUTE_101_TRIGGER_TILE, (11, 19)] {
        for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
            let (nx, ny) = (tx + dx, ty + dy);
            assert!(
                !events
                    .object_events
                    .iter()
                    .any(|o| i32::from(o.x) == nx && i32::from(o.y) == ny),
                "an object event at ({nx}, {ny}) would make the trigger's interaction-discard \
                 arm reachable -- pin it for real instead of relying on this assertion"
            );
        }
    }
}

/// The data premise `MapRuntime::coord_event_at`'s documented first-match
/// divergence rests on (its own doc comment): every Route 101 coord event
/// sits at a distinct position, so first-match and upstream's
/// keep-scanning loop cannot disagree here. Asserted like the slice's
/// other data premises (no warp events, no adjacent NPCs), so a future
/// map change that stacks two coord events on one tile fails this test
/// and forces the engine helper's semantics to be revisited.
#[test]
fn route_101_coord_events_all_sit_at_distinct_positions() {
    let events = assets::MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .expect("Route 101 always has a static events entry");
    let mut positions: Vec<(i16, i16, u8)> = events
        .coord_events
        .iter()
        .map(|e| (e.x, e.y, e.elevation))
        .collect();
    let total = positions.len();
    positions.sort_unstable();
    positions.dedup();
    assert_eq!(
        positions.len(),
        total,
        "two coord events share a tile -- coord_event_at's first-match \
         shortcut is no longer equivalent to upstream's scan"
    );
}
