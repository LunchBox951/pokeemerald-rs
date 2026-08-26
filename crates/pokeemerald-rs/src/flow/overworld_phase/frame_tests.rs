//! Tests for dialog interaction, frame composition, and OAM positioning
//! ([`super::OverworldPhase::compose_frame`], NPC dialog routing).

use super::test_support::*;
use super::OverworldPhase;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// The issue #161 acceptance test: spawn (post-#158 intro handoff) ->
/// walk down the stairs to Brendan's House 1F (the real #163 warp path,
/// already pinned by [`stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map`])
/// -> face Mom (the real, pack-loaded `OBJ_EVENT_GFX_MOM` object event at
/// `(2, 6)`, script `PlayersHouse_1F_EventScript_Mom`) -> A opens her
/// dialog with the real upstream text (`crate::overworld::npc_scripts::script_text`'s
/// own transcription of `PlayersHouse_1F_Text_IsntItNiceInHere`) -> A
/// confirms her script-level `waitbuttonpress` (issue #410) -> the dialog
/// closes, with its text still up until that exact tick, and control
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
    // script-level waitbuttonpress wait needs one), then confirm it, then
    // let it close. Generous frame budgets throughout: the exact per-glyph
    // cadence is `engine::text::render::Printer`'s own, already pinned by
    // that module's tests -- this test only cares that the *dialog* (not
    // the printer internals) reaches each milestone.
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
            panic!("the dialog must not close on its own before a confirm reaches waitbuttonpress");
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

    // Still idling past the full print: the box must keep every glyph on
    // screen, waiting on the script-level `waitbuttonpress`
    // (`NpcDialog::with_waitbuttonpress`, issue #410) rather than auto-
    // closing or clearing on its own.
    for _ in 0..8 {
        phase.step(ButtonState::new());
        let dialog = phase
            .dialog
            .as_ref()
            .expect("must still be open, awaiting the confirm press");
        assert_eq!(
            dialog.revealed_glyph_count(),
            expected_glyph_count,
            "text must stay fully on screen while awaiting waitbuttonpress"
        );
    }

    // Confirm: issue #410 means this closes on the very next tick the
    // press lands on, with the text still fully shown right up to that
    // tick -- no intervening `Cleared`/blank-box frames the old synthetic
    // trailing `{P}` used to force. Pressing A fresh every frame (rather
    // than once) is deliberate and still exactly matches a single real
    // button press's effect: the exact frame a press first lands on is
    // otherwise timing-sensitive to get right in this test, so this holds
    // the "button" down across a small window instead; the loop stops the
    // instant the dialog closes, so no press after that could re-open a
    // new one against Mom, still facing.
    let mut closed = false;
    for _ in 0..5 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed, "confirming waitbuttonpress must close the dialog");

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
