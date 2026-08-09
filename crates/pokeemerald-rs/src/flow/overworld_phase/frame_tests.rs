//! Tests for dialog interaction, frame composition, and OAM positioning
//! ([`super::OverworldPhase::compose_frame`], NPC dialog routing).

use super::test_support::*;
use super::OverworldPhase;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// The issue #161 acceptance test: walking down the stairs and talking to Mom.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert!(phase.dialog.is_none(), "no dialog is open at spawn");

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

    phase.player = PlayerState::new((2, 7), 3, Direction::North);

    phase.step(pressed(Buttons::A));
    assert!(
        phase.dialog.is_some(),
        "a fresh A-press facing Mom must open her dialog"
    );
    assert_eq!(
        phase.map_id, one_f,
        "opening a dialog must not itself change the room"
    );

    let position_before_printing = phase.player.position();
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        position_before_printing,
        "movement must be frozen while a dialog is open"
    );

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

/// The issue #217 acceptance test: walking past Mom keeps her OAM glued
/// to the scrolling background.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_past_mom_keeps_her_oam_glued_to_the_scrolling_background() {
    fn mom_and_scroll(phase: &OverworldPhase) -> (i32, i32) {
        let (entries, (_, scroll_y)) = phase
            .scene
            .oam_entries_and_bg_scroll(&phase.player, &phase.save1().event_data);
        assert_eq!(entries.len(), 2, "1F draws the player and Mom, nobody else");
        (i32::from(entries[1].y()), i32::from(scroll_y))
    }

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

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

    phase.player = PlayerState::new((2, 7), 3, Direction::South);
    let (rest_y, rest_scroll) = mom_and_scroll(&phase);
    assert_eq!(rest_scroll, 0, "at rest the BG scroll is 0");

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

/// B alone advances and closes a dialog.
#[test]
fn b_alone_advances_and_closes_a_dialog() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));

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

/// B does not move the player or open a dialog.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn b_does_not_move_the_player_or_open_a_dialog() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));
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

    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.step(pressed(Buttons::B));
    assert!(
        phase.dialog.is_none(),
        "B is not an interaction button -- upstream gates \
         TryStartInteractionScript on pressedAButton alone"
    );
    phase.step(pressed(Buttons::A));
    assert!(phase.dialog.is_some(), "A still interacts");
}

/// Review regression (#192): idle frames animate the composed tileset pixels.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn idle_frames_animate_the_composed_tileset_pixels() {
    let town = assets::MapId("MAP_LITTLEROOT_TOWN");
    let mut phase = OverworldPhase::for_test(
        crate::overworld::load_room(town).expect("run `cargo xtask extract` first"),
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
