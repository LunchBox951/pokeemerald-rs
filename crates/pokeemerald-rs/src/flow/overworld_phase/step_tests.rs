//! Tests for [`super::OverworldPhase::step`] and related stepping/collision
//! behaviour.

use super::step::InteractionOutcome;
use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// Regression for the new-game-to-overworld RNG handoff: trainer-id
/// initialization consumes exactly one `Random()` draw (the id's high half
/// -- the low half is the seed itself, not a second draw;
/// `new_game::init_save_blocks`'s module docs), and encounters must continue
/// from that advanced state rather than restarting at seed 0 or skipping an
/// extra draw that was never really spent.
#[test]
fn new_game_rng_stream_continues_after_the_trainer_id_draw() {
    let phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
    let mut expected = Rng::new(new_game::NEW_GAME_RNG_SEED);
    expected.next_u16();

    assert_eq!(
        phase.rng.state(),
        expected.state(),
        "the phase must retain the RNG state after the one trainer-id draw"
    );
    // Independently-derived ground truth (issue #313): `ISO_RANDOMIZE1(0) ==
    // 1_103_515_245 * 0 + 24_691 == 24_691 == 0x0000_6073`.
    assert_eq!(phase.rng.state(), 0x0000_6073);
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

/// Issue #435 regression: a same-frame A-plus-direction press must resolve
/// the interaction lookup against the PRE-movement facing, and a hit must
/// preempt this frame's movement outright -- see [`OverworldPhase::step`]'s
/// "NPC dialog routing" section for the upstream citations. Before this
/// fix, a perpendicular direction held alongside A would turn the player
/// before the interaction lookup ran, missing Mom.
///
/// `AssetPack::load_default` (needed to actually render a dialog box) is
/// unavailable headless, so this checks the interaction lookup's own
/// outcome directly (the same pattern
/// `a_pressed_mid_step_is_discarded_and_the_same_press_at_rest_interacts`
/// uses) rather than `phase.dialog` -- the real-pack acceptance test in
/// `frame_tests` already covers the box actually opening.
#[test]
fn a_pressed_with_a_perpendicular_direction_finds_mom_and_does_not_turn_the_player() {
    // Two tiles east of Mom is too far; one tile east, facing west, is
    // exactly adjacent (module docs' `ONE_F` fixture notes).
    let start = PlayerState::new((3, 6), 3, Direction::West);
    let mut phase = synthetic_phase(start, None);

    {
        let runtime = runtime_for(&phase);
        // North is perpendicular to the player's West facing -- a step in
        // that direction would turn the player away from Mom if movement
        // ran first.
        let outcome =
            phase.interaction_tokens_this_frame(pressed(Buttons::A | Buttons::UP), &runtime);
        assert!(
            matches!(outcome, Some(InteractionOutcome::Dialog(_))),
            "the pre-movement facing (still West) must find Mom and her recognized script, \
             even with a perpendicular direction also pressed this frame"
        );
    }

    // Drive the identical buttons through the real `step()` pipeline: the
    // interaction must claim the frame before `advance_or_skip_for_preempt`
    // can turn or step the player.
    phase.step(pressed(Buttons::A | Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (3, 6),
        "an interaction that fires this frame must preempt the step"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::West,
        "and the turn too -- PlayerStep never runs once the interaction claims the frame"
    );
    assert!(
        !phase.player.in_transit(),
        "no walk animation may have started either"
    );
}

/// The complement: an A press with a direction held, but facing nothing,
/// must still turn or step exactly as it did before this fix -- the
/// preempt-movement path introduced for issue #435 must not fire when
/// [`OverworldPhase::interaction_tokens_this_frame`] finds no object event.
#[test]
fn a_pressed_with_a_direction_while_facing_nothing_turns_normally() {
    // One tile further east than the fixture above: (3, 6) ahead is clear
    // of visible object events (module docs' `ONE_F` fixture notes).
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);

    phase.step(pressed(Buttons::A | Buttons::UP));

    assert!(
        phase.dialog.is_none(),
        "no object event stands ahead of this tile -- nothing to interact with"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "an A press with no interaction to preempt movement must still let the direction turn \
         the player, exactly as a direction alone would"
    );
}

/// An object event with a real, non-`"0x0"` script this port does not model
/// yet still consumes the frame: `TryStartInteractionScript`
/// (`field_control_avatar.c:172`) returns TRUE for any non-NULL script, so
/// `PlayerStep` never runs (`overworld.c:1444-1455`).
#[test]
fn a_pressed_with_a_perpendicular_direction_facing_an_unmodelled_script_does_not_turn() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::North), None);

    phase.step(pressed(Buttons::A | Buttons::LEFT));

    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "facing a visible object event with a real (unmodelled) script, an A press consumes \
         the frame upstream -- the perpendicular direction must not turn the player"
    );
    assert_eq!(phase.player.position(), (4, 6));
}

/// The faced object really is a visible object event carrying a real script.
#[test]
fn the_faced_vigoroth_is_visible_with_a_real_script() {
    let phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::North), None);
    let runtime = runtime_for(&phase);
    let object =
        engine::overworld::facing_object_event(&phase.player, &runtime, &phase.save1.event_data)
            .expect("the Vigoroth at (4, 5) must be visible on a fresh save");
    assert_eq!(object.script, "PlayersHouse_1F_EventScript_Vigoroth1");
    assert!(crate::overworld::npc_scripts::script_text(object.script).is_none());
}

/// The `"0x0"` NULL-script sentinel and a real unmodelled script differ:
/// Fallarbor Town's Battle Tent corridor attendant at `(2, 6)` carries
/// `"0x0"` and no hide flag, so an A press on it must not consume the
/// frame and the perpendicular direction still turns the player.
#[test]
fn a_null_script_does_not_preempt_movement_unlike_an_unmodelled_script() {
    const CORRIDOR: assets::MapId = assets::MapId("MAP_FALLARBOR_TOWN_BATTLE_TENT_CORRIDOR");
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        CORRIDOR,
        PlayerState::new((3, 6), 3, Direction::West),
        None,
    );

    {
        let header = assets::MapHeaderTable::new().header(CORRIDOR).unwrap();
        let events = assets::MapEventsTable::new().resolve(CORRIDOR).unwrap();
        let runtime = phase.scene.runtime(CORRIDOR, header, events);
        let object = engine::overworld::facing_object_event(
            &phase.player,
            &runtime,
            &phase.save1.event_data,
        )
        .expect("the corridor attendant at (2, 6) must be visible and faced");
        assert_eq!(object.script, "0x0", "the NULL-script sentinel");
    }

    phase.step(pressed(Buttons::A | Buttons::UP));

    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a `\"0x0\"` script is upstream's NULL no-op: it must not consume the frame, so the \
         perpendicular direction still turns the player"
    );
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

/// Wrapping [`OverworldPhase::tick`] past `u32::MAX` lands on
/// `step::TILESET_ANIM_WRAP_PERIOD` (256), not 0: tick 0 reads to
/// `tileset_anims::latched_frame` as a fresh room with no region fired yet,
/// while 256 is a tick every configured cadence has already latched by
/// (`tileset_anims` module docs). `synthetic_phase` fabricates real
/// `general`-tileset animation frames, so `phase.tick` alone drives the
/// boundary.
#[test]
fn wrapping_the_tileset_animation_tick_lands_on_the_upstream_period_not_zero() {
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), None);
    phase.tick = u32::MAX;

    phase.step(ButtonState::new());
    assert_eq!(
        phase.tick, 256,
        "the tick that wraps past u32::MAX must land where tick 256 already is, \
         not back at the fresh-room tick 0"
    );

    for expected in 257..=261u32 {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.tick, expected,
            "counting must resume normally from the wrap target"
        );
    }
}

/// Issue #852: [`OverworldPhase::advance_start_menu_frame`] (`start_menu.rs`)
/// keeps the animation running while the field start menu owns the frame,
/// exactly as [`OverworldPhase::step`] does while a dialog does, and both
/// call sites must wrap `tick` to the same already-latched tick. A synthetic
/// already-open menu ([`crate::start_menu::synthetic_start_menu`]) needs no
/// local pack, so this reaches the real wrap without one.
#[test]
fn wrapping_the_tick_through_the_start_menu_frame_also_lands_on_the_upstream_period() {
    let temp = crate::flow::tests::TempSave::new("start-menu-tick-wrap-852");
    let mut save_slot = temp.slot();
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), None);
    phase.start_menu = Some(crate::start_menu::synthetic_start_menu());
    phase.tick = u32::MAX;

    assert!(
        phase.advance_start_menu_frame(ButtonState::new(), &mut save_slot),
        "an already-open menu must keep owning the frame"
    );
    assert_eq!(
        phase.tick, 256,
        "the start-menu frame path must wrap the same way step() does, not back to 0"
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

/// Issue #908 regression: a same-frame `A`-plus-`START` press must resolve
/// this port's counterpart to `TryStartInteractionScript`
/// (`field_control_avatar.c:172`) before `pressedStartButton`
/// (`:182-187`) ever gets a look, because upstream's own
/// `ProcessPlayerFieldInput` checks the interaction branch first and
/// returns `TRUE` out of it before the `START` branch is even reached.
///
/// Stands the player where
/// [`a_pressed_mid_step_is_discarded_and_the_same_press_at_rest_interacts`]
/// settles -- one tile east of Mom, at rest, already facing her -- so the
/// same real, recognized interaction backs `field_input_claimed` here.
/// [`OverworldPhase::step`] is not driven end to end: both outcomes this
/// port would take (opening [`crate::overworld::NpcDialog`] or
/// [`crate::start_menu::open`]) need a local asset pack this checkout does
/// not have, so an end-to-end assertion would pass on a merge-base bug for
/// the wrong reason -- the menu failing to open on a missing pack, not on
/// the gate. Asserted at [`OverworldPhase::start_menu_may_open`] instead,
/// the same decision [`OverworldPhase::step`]'s own "Field start menu
/// ordering" section feeds from a real
/// [`OverworldPhase::interaction_tokens_this_frame`] lookup every frame.
#[test]
fn start_does_not_preempt_a_same_frame_npc_interaction() {
    let phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    let buttons = pressed(Buttons::A | Buttons::START);

    let interaction_found = {
        let runtime = runtime_for(&phase);
        phase
            .interaction_tokens_this_frame(buttons, &runtime)
            .is_some()
    };
    assert!(
        interaction_found,
        "the fixture must face an NPC whose script this port recognizes"
    );

    assert!(
        !phase.start_menu_may_open(buttons, interaction_found),
        "a same-frame interaction must refuse a fresh START the same frame \
         (field_control_avatar.c:172 returns TRUE before :182)"
    );
    // Positive control: the same fixture, told nothing else claimed the
    // frame, is where a fresh START normally works -- so the refusal above
    // is really the interaction claim, not some other gate this fixture
    // happens to fail.
    assert!(
        phase.start_menu_may_open(buttons, false),
        "the fixture must otherwise be a frame START can open"
    );
}

/// Review-round regression: a same-frame `START` press that fails to build
/// a menu (no local asset pack, [`crate::start_menu::open`]'s own real
/// failure mode) must not also cost that frame's movement --
/// [`OverworldPhase::build_start_menu`]'s own doc comment on why the menu
/// is *built*, not merely decided, ahead of movement, specifically so a
/// failed build is known before [`OverworldPhase::step`] chooses whether to
/// preempt movement for it. This crate's own `cargo test` environment never
/// has a local pack (`crate::flow::tests`' own guard pattern), so the same
/// held-direction-plus-fresh-`START` frame here reliably exercises the
/// failure path, not the pack-dependent success one.
#[test]
fn a_failed_pack_load_on_start_does_not_cost_the_frames_movement() {
    if assets::pack::AssetPack::default_path().is_file() {
        return;
    }
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);

    // Already facing west (module docs' `ONE_F` fixture notes): holding
    // Left begins a step immediately, no separate turn frame first.
    let mut buttons = ButtonState::new();
    buttons.update(Buttons::LEFT);
    buttons.update(Buttons::LEFT | Buttons::START);
    phase.step(buttons);

    assert!(
        phase.start_menu().is_none(),
        "no local pack exists in this test environment, so the menu must \
         not have opened"
    );
    assert!(
        phase.player.in_transit(),
        "a failed pack load must leave START exactly as inert as a refused \
         gate -- the held-direction step must still have started"
    );
}
