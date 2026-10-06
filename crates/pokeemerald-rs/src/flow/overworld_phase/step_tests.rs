//! Stepping, collision, and turn locks.

use super::step::InteractionOutcome;
use super::test_support::*;
use super::{OverworldPhase, SyntheticStartMenu};
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

/// `START` holds the field lock too (`start_menu.c:581-591`), so it clears
/// a pending turn's busy window like a dialog does.
#[test]
fn a_start_menu_opened_inside_a_turns_busy_window_must_not_swallow_input_after_it_closes() {
    let temp = crate::flow::tests::TempSave::new("start-menu-turn-lock-976");
    let mut save_slot = temp.slot();
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "the held direction must turn the player in place, starting the busy window"
    );

    phase.start_menu = Some(crate::start_menu::synthetic_start_menu());
    for _ in 0..20 {
        assert!(phase.advance_start_menu_frame(ButtonState::new(), &mut save_slot));
    }
    assert!(
        phase.advance_start_menu_frame(pressed(Buttons::B), &mut save_slot),
        "B still owns the closing frame"
    );
    assert!(
        phase.start_menu().is_none(),
        "B must have closed the synthetic menu"
    );

    // The first field frame after the menu closes must act on the held
    // direction, not sit in a window frozen before the menu opened.
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.facing(),
        Direction::East,
        "the turn's busy window must not survive the start menu that opened inside it"
    );
}

/// `ShowStartMenu` freezes the player before its own frame is drawn
/// (`start_menu.c:581-591`), so the turn ends on the frame START lands.
#[test]
fn a_fresh_start_ends_a_turns_busy_window_on_the_frame_the_menu_opens() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
    phase.synthetic_start_menu = SyntheticStartMenu::Builds;

    phase.step(held(Buttons::UP));
    assert!(
        phase.player.turn_frames_remaining() > 0,
        "setup: the held direction turns in place and starts the busy window"
    );

    phase.step(pressed(Buttons::START));
    assert!(
        phase.start_menu().is_some(),
        "setup: the injected build must really have opened a menu"
    );
    assert_eq!(
        phase.player.turn_frames_remaining(),
        0,
        "the menu's own opening frame is composed after this step returns, so the \
         turn must already be over by then -- not one frame later"
    );
}

/// A turn the field lock cancels never reaches its tile-centre observation,
/// so nothing replays once the menu closes.
#[test]
fn a_turn_cancelled_by_start_is_never_observed_by_the_wild_check() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
    phase.synthetic_start_menu = SyntheticStartMenu::Builds;

    phase.step(held(Buttons::UP));
    phase.step(pressed(Buttons::START));
    phase.start_menu = None;
    for _ in 0..(2 * engine::overworld::TURN_IN_PLACE_FRAMES) {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.wild.immunity_steps(), 0);
}

/// The interaction claiming the frame is upstream's lock
/// (`field_control_avatar.c:172`); the box itself needs a pack this suite lacks.
#[test]
fn an_a_press_interaction_ends_a_turns_busy_window_on_the_frame_it_claims() {
    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::South), None);

    phase.step(held(Buttons::LEFT));
    assert!(
        phase.player.turn_frames_remaining() > 0,
        "setup: the held direction turns the player toward Mom, starting the busy window"
    );
    {
        let runtime = runtime_for(&phase);
        assert!(
            matches!(
                phase.interaction_tokens_this_frame(pressed(Buttons::A), &runtime),
                Some(InteractionOutcome::Dialog(_))
            ),
            "setup: the next frame's A press really does claim the frame"
        );
    }

    phase.step(pressed(Buttons::A));
    assert_eq!(
        phase.player.turn_frames_remaining(),
        0,
        "the claimed frame is composed after this step returns, so the turn must \
         already be over by then -- not one frame later"
    );
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
