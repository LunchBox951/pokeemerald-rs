//! Tests for [`super::OverworldPhase::step`] and related stepping/collision
//! behaviour.

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// Regression for the new-game-to-overworld RNG handoff.
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

/// Senior review regression, headless: upstream discards an A press made
/// *during* a tile crossing outright.
#[test]
fn a_pressed_mid_step_is_discarded_and_the_same_press_at_rest_interacts() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);

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
/// open, [`OverworldPhase::step`] must not feed movement to the player.
#[test]
fn an_open_dialog_freezes_movement_until_it_closes() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
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

    let mut closed = false;
    for _ in 0..40 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed, "confirming must close the synthetic dialog");

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (7, 5),
        "ordinary movement must resume once the box has closed"
    );
}

/// Mutation guard for [`OverworldPhase::step`]'s tileset-animation tick.
#[test]
fn step_advances_the_tileset_animation_tick_once_per_frame_even_behind_a_dialog() {
    use engine::text::Token;

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

/// The other half of the [`OverworldPhase`] tick wiring: every map (re)load
/// restarts the room's animation counter at 0.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn loading_a_room_and_warping_both_restart_the_tileset_animation_tick() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.tick, 0,
        "a freshly loaded room starts its animation counter at 0"
    );

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

/// Issue #207 review, round 2: the production starter *wiring*.
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

/// The finding-1 regression at the phase level, on real map data.
#[test]
fn holding_a_direction_into_a_visible_npc_stops_the_player_adjacent_to_it() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);

    phase.step(held(Buttons::LEFT));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::LEFT));
    }
    assert_eq!(phase.player.position(), (3, 6));
    assert!(!phase.player.in_transit());

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

    let runtime = runtime_for(&phase);
    assert!(
        phase
            .interaction_tokens_this_frame(pressed(Buttons::A), &runtime)
            .is_some(),
        "the tile the player was stopped on must be the tile Mom is \
         interactable from"
    );
}

/// The complement, same fixture shape: a *hidden* object event does not block.
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
/// to the player every frame.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn overworld_movement_input_turns_the_player() {
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

/// The issue #218 escape: the bed's center pillow tile at layout-local
/// `(1, 4)` carries `MB_IMPASSABLE_SOUTH_AND_NORTH`. Before the fix,
/// the avatar could walk lengthwise off the bed and out through its headboard.
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

    phase.player = PlayerState::new((0, 7), 3, Direction::North);
    for expected in [(0, 6), (0, 5), (0, 4)] {
        walk(&mut phase, Buttons::UP);
        assert_eq!(phase.player.position(), expected);
    }

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

/// The bed's side columns, which are walkable end to end and must stay
/// that way.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "fixture precondition: the spawn map is the bedroom carrying the bed"
    );

    phase.player = PlayerState::new((0, 7), 3, Direction::North);

    let walk_one_tile_north = |phase: &mut OverworldPhase| {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    };

    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 6));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 3),
        "transition wildcard adopted into elevation; previous_elevation \
         still reads the floor behind"
    );

    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 5));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (4, 4),
        "the raised edge is real, non-transition elevation: both fields \
         adopt it"
    );

    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 4));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 4),
        "previous_elevation must still read the raised edge's 4 while \
         standing on the wildcard, not reset by it"
    );

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
