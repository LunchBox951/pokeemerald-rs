//! Tests for dialog interaction and frame composition: A/B dialog input,
//! frame ticking, and NPC placement against the scrolling background.

use super::test_support::*;
use super::OverworldPhase;
use crate::palette_overlay::IdentityTransform;
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};
use rendering::{
    Framebuffer, NormalPaletteFade, PaletteColorTransform, PaletteFadeTarget, PaletteStage,
};

/// Background scroll at rest: the viewport's baseline offset, in pixels.
const RESTING_SCROLL_Y_PX: i32 = 8;
/// Pixels the scene moves over one tile step.
const TILE_STEP_PX: i32 = 16;

/// Walks from spawn down the stair warp to Brendan's House 1F, faces Mom,
/// and checks that A opens her real dialog, the text stays fully shown until
/// the confirm press, the confirm closes the dialog on that tick, and
/// movement input then works again.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog() {
    let mut phase = OverworldPhase::load(
        crate::pack_source::PackSource::Repo,
        crate::new_game::NewGameOptions::DEFAULT,
        crate::new_game::NewGameIdentity::DEFAULT,
    )
    .expect("run `cargo xtask extract` first");
    assert!(phase.dialog.is_none(), "no dialog is open at spawn");

    // The player spawns on the stair warp tile, so stepping off and back on
    // produces a fresh warp landing.
    let bedroom = phase.map_id;
    walk_one_tile(&mut phase, Buttons::DOWN);
    assert_eq!(phase.map_id, bedroom, "still upstairs after stepping south");
    turn_and_walk_one_tile(&mut phase, Buttons::UP);
    let one_f = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(
        phase.map_id, one_f,
        "stepping back onto the stair warp must land on 1F"
    );

    // Mom stands at (2, 6). The test places the player on the tile south of
    // her, facing north, instead of walking there; this test covers only the
    // interaction and dialog wiring against the real Mom object.
    phase.player = PlayerState::new((2, 7), 3, Direction::North);

    // A facing Mom opens her dialog.
    phase.step(pressed(Buttons::A));
    assert!(
        phase.dialog.is_some(),
        "a fresh A-press facing Mom must open her dialog"
    );
    assert_eq!(
        phase.map_id, one_f,
        "opening a dialog must not itself change the room"
    );

    // A held direction must not move the player while the dialog is open.
    let position_before_printing = phase.player.position();
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        position_before_printing,
        "movement must be frozen while a dialog is open"
    );

    // Print every glyph of the real text without any confirm held, since
    // only the script-level `waitbuttonpress` wait needs one. The frame
    // budgets are generous because per-glyph timing belongs to
    // `engine::text::render::Printer`'s tests.
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

    // After the full print, the box keeps every glyph on screen and waits for
    // the `waitbuttonpress` confirm (`NpcDialog::with_waitbuttonpress`)
    // instead of closing on its own.
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

    // Press A every frame across a short window, because the frame on which
    // the prompt first accepts a press is timing-dependent. The loop stops
    // when the dialog closes, so no later press can reopen it with Mom still
    // in front of the player.
    let mut closed = false;
    for _ in 0..5 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed, "confirming waitbuttonpress must close the dialog");

    // Facing north from Mom, the first held Down only turns. The turn keeps
    // the player busy through frame 8, and the step commits on frame 9.
    assert_eq!(phase.player.facing(), Direction::North);
    phase.step(held(Buttons::DOWN));
    assert_eq!(phase.player.facing(), Direction::South, "must turn first");
    assert_eq!(
        phase.player.position(),
        (2, 7),
        "a turn must not move the tile"
    );
    for _ in 2..=8 {
        phase.step(held(Buttons::DOWN));
        assert_eq!(phase.player.position(), (2, 7), "still busy from the turn");
    }
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (2, 8),
        "resumes once the turn drains"
    );
}

/// Mom's OAM must stay locked to the scrolling background on every frame of
/// a real player step through [`OverworldPhase::step`], using the real pack,
/// map data and button input.
///
/// `PlayerState::step` commits the destination tile on the first frame, so
/// without the shared camera-lag term Mom's OAM would jump a full metatile
/// on that frame and then sit still while the background scrolled. The test
/// therefore checks per-frame deltas: Mom's OAM `y` against the BG
/// `scroll_y` shared by every layer, at rest, on the first and last transit
/// frames, at the midpoint (`step_progress() == 8`), and on the first resting
/// frame. Progress 0 in all four directions is covered by
/// `crate::overworld::npc::placement_tests`, and the BG scroll's intermediate
/// value by `crate::overworld::viewport::tests`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_past_mom_keeps_her_oam_glued_to_the_scrolling_background() {
    /// Mom's OAM `y` and the shared BG `scroll_y` of the scene's current
    /// frame. OAM entry 0 is the player and entry 1 is Mom; nobody else draws
    /// on 1F on a fresh save
    /// (`crate::overworld::tests::real_pack_1f_oam_entries_cover_every_drawn_fresh_save_npc`).
    fn mom_and_scroll(phase: &OverworldPhase) -> (i32, i32) {
        let (entries, (_, scroll_y)) = phase
            .scene
            .oam_entries_and_bg_scroll(&phase.player, &phase.save1().event_data);
        assert_eq!(entries.len(), 2, "1F draws the player and Mom, nobody else");
        (i32::from(entries[1].y()), i32::from(scroll_y))
    }

    let mut phase = OverworldPhase::load(
        crate::pack_source::PackSource::Repo,
        crate::new_game::NewGameOptions::DEFAULT,
        crate::new_game::NewGameIdentity::DEFAULT,
    )
    .expect("run `cargo xtask extract` first");

    // Reach 1F through the stair warp, as in the dialog test above.
    walk_one_tile(&mut phase, Buttons::DOWN);
    turn_and_walk_one_tile(&mut phase, Buttons::UP);
    let one_f = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(phase.map_id, one_f, "the stair warp must land on 1F");

    // One tile south of Mom and already facing south, so the first held Down
    // commits a step instead of turning.
    phase.player = PlayerState::new((2, 7), 3, Direction::South);
    let (rest_y, rest_scroll) = mom_and_scroll(&phase);
    assert_eq!(
        rest_scroll, RESTING_SCROLL_Y_PX,
        "at rest the BG scroll equals the resting baseline"
    );

    // Frame 1 of a south step moves Mom one pixel, not one metatile.
    phase.step(held(Buttons::DOWN));
    assert_eq!(phase.player.position(), (2, 8), "the tile commits at once");
    assert_eq!(phase.player.step_progress(), 1);
    let (frame1_y, frame1_scroll) = mom_and_scroll(&phase);
    assert_eq!(
        frame1_y - rest_y,
        -1,
        "walking south slides Mom one pixel up the screen on the first frame"
    );

    // Advance to the midpoint, then to the last transit frame.
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

    // Between transit frames, Mom's OAM moves up by as many pixels as the
    // background scrolls down. Frames at rest are compared on OAM alone:
    // `build_tilemaps` adds direction-dependent tilemap padding during a
    // transit, which shifts `scroll_y`'s origin by a constant that cancels
    // only between two frames on the same side of that boundary.
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
        (RESTING_SCROLL_Y_PX + 8, RESTING_SCROLL_Y_PX + 15),
        "the scroll is the resting baseline plus the elapsed frame count"
    );

    // Boundary frames and the whole-step total.
    assert_eq!(
        last_y - settled_y,
        1,
        "frame 15 owes exactly one last pixel"
    );
    assert_eq!(
        settled_scroll, RESTING_SCROLL_Y_PX,
        "back at rest, the BG scroll returns to the resting baseline"
    );
    assert_eq!(
        rest_y - settled_y,
        TILE_STEP_PX,
        "one whole metatile of travel across the step, and no more"
    );

    // The settled frame equals the resting placement of a fresh player
    // standing on the destination tile.
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

// -- Message-box confirm input: A or B ------------------------------------

/// A fresh B press advances and closes a dialog, as A does: upstream's
/// down-arrow wait, mid-page wait and hold-to-speed-up path all accept
/// `A_BUTTON | B_BUTTON` (`TextPrinterWaitWithDownArrow` at
/// `pokeemerald/src/text.c:865-882`, `TextPrinterWait` at `:884-900`,
/// `RunTextPrinter` at `:944` and `:950`).
#[test]
fn b_alone_advances_and_closes_a_dialog() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));

    // Press only B on every frame of a bounded window, because the frame on
    // which the prompt first accepts a press is timing-dependent.
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

/// B closes an open dialog without also moving the player, and with no
/// dialog open B does not start an interaction.
///
/// [`OverworldPhase::step`] returns after the dialog branch, before movement
/// and interaction, and interaction reads only A
/// (`FieldInput::pressedAButton`, `field_control_avatar.c:172`).
#[test]
fn b_does_not_move_the_player_or_open_a_dialog() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    // The tile to the south is clear, so a step that was not frozen would move the player.
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));
    // The bounded loop fails, rather than hangs, if B stops closing the box.
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

    // Mom stands at (2, 6); the player stands east of her facing west.
    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.step(pressed(Buttons::B));
    assert!(phase.dialog.is_none(), "B is not an interaction button");
    // The same press with A resolves an interaction, so the check above
    // depends on the button and not the position. The test calls
    // `interaction_tokens_this_frame` directly because `step` would open the
    // dialog through `NpcDialog::open_at_speed`, which loads an asset pack
    // this synthetic fixture lacks.
    let runtime = runtime_for(&phase);
    assert!(
        phase
            .interaction_tokens_this_frame(pressed(Buttons::B), &runtime)
            .is_none(),
        "no B edge may resolve an interaction"
    );
    assert!(
        phase
            .interaction_tokens_this_frame(pressed(Buttons::A), &runtime)
            .is_some(),
        "A still interacts"
    );
}

/// The phase's tick counter must reach
/// [`crate::overworld::OverworldScene::compose`] through
/// [`OverworldPhase::compose_frame`]. Other tests cover the counter's
/// increment, its reset on load and warp, and `compose(tick)` changing
/// pixels, but not the join between the counter and composition. Littleroot
/// Town's flower view (the real-pack fixture position in
/// `crate::overworld::tests`) composes different pixels at tick 60 than at
/// tick 0.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn idle_frames_animate_the_composed_tileset_pixels() {
    let town = assets::MapId("MAP_LITTLEROOT_TOWN");
    let mut phase = OverworldPhase::for_test_with_source(
        crate::overworld::load_room_from_source(
            &crate::pack_source::PackSource::Repo,
            town,
            crate::overworld::PlayerCharacter::Brendan,
            &engine::event_data::EventData::new(),
        )
        .expect("run `cargo xtask extract` first"),
        town,
        PlayerState::new((10, 17), 3, Direction::South),
        None,
        crate::pack_source::PackSource::Repo,
    );

    let base = phase.compose_frame();
    for _ in 0..60 {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.tick, 60, "each idle frame advances the tick by one");
    let animated = phase.compose_frame();
    assert_ne!(
        &*base, &*animated,
        "60 idle frames must animate the flower tiles through the phase's tick"
    );
}

/// The synthetic scene's two colours: red ground and a blue avatar.
const RED: u32 = 0x00FF_0000;
const BLUE: u32 = 0x0000_00FF;
const BLACK: u32 = 0;

fn field_phase() -> OverworldPhase {
    synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), None)
}

fn count_of(frame: &platform::Frame, color: u32) -> usize {
    frame.iter().filter(|&&pixel| pixel == color).count()
}

fn pack(color: rendering::Rgb888) -> u32 {
    u32::from_be_bytes([0, color.r, color.g, color.b])
}

/// A fade one update past `begin`: the BG blend is at coefficient 2 while the
/// OBJ blend is still at 0 (`pokeemerald/src/palette.c:432-473`).
fn half_stepped_fade() -> NormalPaletteFade {
    let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::Black);
    fade.update();
    fade.update();
    fade
}

#[test]
fn the_identity_stage_leaves_the_field_frame_as_it_was() {
    let phase = field_phase();
    let frame = phase.compose_frame();
    assert!(
        frame.iter().all(|&pixel| pixel == RED || pixel == BLUE),
        "the fixture frame is red ground under a blue avatar"
    );
    assert!(count_of(&frame, RED) > 0 && count_of(&frame, BLUE) > 0);

    assert_eq!(
        *phase.compose_frame_staged(PaletteStage::default(), None),
        *frame
    );
    let identity = IdentityTransform;
    let forced = PaletteStage {
        bg: Some(&identity),
        obj: Some(&identity),
    };
    assert_eq!(
        *phase.compose_frame_staged(forced, None),
        *frame,
        "an identity transform on both banks must not change a pixel"
    );

    let scene_frame = phase.scene.compose_staged(
        &phase.player,
        &phase.save1.event_data,
        phase.tick,
        forced,
        None,
    );
    assert_eq!(
        scene_frame.pixels(),
        phase
            .scene
            .compose(&phase.player, &phase.save1.event_data, phase.tick)
            .pixels()
    );
}

#[test]
fn a_fully_black_stage_blanks_the_field() {
    let phase = field_phase();
    let fade = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
    let black = phase.compose_frame_staged(fade.palette_stage(), None);
    assert_eq!(count_of(&black, BLACK), black.len());

    let scene_frame = phase.scene.compose_staged(
        &phase.player,
        &phase.save1.event_data,
        phase.tick,
        fade.palette_stage(),
        None,
    );
    assert!(scene_frame
        .pixels()
        .iter()
        .all(|&pixel| pixel == rendering::Rgb888::BLACK));
}

#[test]
fn each_bank_stages_only_its_own_field_pixels() {
    let phase = field_phase();
    let fade = half_stepped_fade();
    let bg = fade.bg_blend();
    let faded_ground = pack(bg.transform(rendering::Rgb888 { r: 255, g: 0, b: 0 }));
    assert_ne!(faded_ground, RED, "the BG blend must have begun");

    let bg_only = phase.compose_frame_staged(
        PaletteStage {
            bg: Some(&bg),
            obj: None,
        },
        None,
    );
    assert!(count_of(&bg_only, faded_ground) > 0);
    assert_eq!(
        count_of(&bg_only, BLUE),
        count_of(&phase.compose_frame(), BLUE),
        "the BG stage leaves the avatar alone"
    );

    let obj_black = NormalPaletteFade::begin_in(PaletteFadeTarget::Black).obj_blend();
    let obj_only = phase.compose_frame_staged(
        PaletteStage {
            bg: None,
            obj: Some(&obj_black),
        },
        None,
    );
    assert_eq!(
        count_of(&obj_only, BLUE),
        0,
        "the OBJ stage blanks the avatar"
    );
    assert_eq!(
        count_of(&obj_only, RED),
        count_of(&phase.compose_frame(), RED)
    );
}

#[test]
fn the_start_menu_is_staged_once_over_a_staged_field() {
    let mut phase = field_phase();
    let fade = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
    let unstaged_field = phase.compose_frame();
    phase.open_synthetic_start_menu();
    let unstaged = phase.compose_frame();
    assert!(
        unstaged.iter().any(|&pixel| pixel != RED && pixel != BLUE),
        "the open menu paints its own colours"
    );

    let black = phase.compose_frame_staged(fade.palette_stage(), None);
    assert_eq!(count_of(&black, BLACK), black.len());

    let stepped = half_stepped_fade();
    let bg = stepped.bg_blend();
    let stage = PaletteStage {
        bg: Some(&bg),
        obj: None,
    };
    let faded = phase.compose_frame_staged(stage, None);
    let ground = unstaged_field[(Framebuffer::HEIGHT - 1) * Framebuffer::WIDTH];
    let once = pack(bg.transform(rendering::Rgb888 { r: 255, g: 0, b: 0 }));
    let twice = pack(bg.transform(bg.transform(rendering::Rgb888 { r: 255, g: 0, b: 0 })));
    assert_eq!(ground, RED);
    assert_ne!(once, twice, "the fixture must tell one staging from two");
    assert_eq!(faded[(Framebuffer::HEIGHT - 1) * Framebuffer::WIDTH], once);
}

#[test]
fn the_field_dialog_is_staged_with_the_field() {
    use engine::text::Token;
    use rendering::Rgb888;

    const BOX: Rgb888 = Rgb888 {
        r: 40,
        g: 80,
        b: 120,
    };
    let dialog = crate::overworld::dialog::synthetic_dialog_with_frame_color(
        vec![Token::Char('A'), Token::End],
        BOX,
    );
    let phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));
    assert!(count_of(&phase.compose_frame(), pack(BOX)) > 0);

    let fade = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
    let black = phase.compose_frame_staged(fade.palette_stage(), None);
    assert_eq!(count_of(&black, BLACK), black.len());

    let bg_black = fade.bg_blend();
    let bg_only = phase.compose_frame_staged(
        PaletteStage {
            bg: Some(&bg_black),
            obj: None,
        },
        None,
    );
    assert_eq!(count_of(&bg_only, pack(BOX)), 0);
    assert!(count_of(&bg_only, BLUE) > 0, "the avatar is not BG");
}

#[test]
fn a_bg_mosaic_reaches_the_field_frame_and_not_the_avatar() {
    let phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_patterned_scene(10, 10),
        ONE_F,
        PlayerState::new((7, 4), 3, Direction::South),
        None,
    );
    let plain = phase.compose_frame();
    // `Task_FieldPoisonEffect` peaks at `tMosaic` 10, writing `REG_MOSAIC` 0xAA
    // (`pokeemerald/src/fldeff_misc.c:1212-1226`).
    let size = rendering::MosaicSize::from_raw(0xA, 0xA);
    let mosaicked = phase.compose_frame_staged(PaletteStage::default(), Some(size));

    assert_ne!(*mosaicked, *plain, "the BG mosaic must change the field");
    assert_eq!(
        count_of(&mosaicked, BLUE),
        count_of(&plain, BLUE),
        "the avatar is not mosaicked"
    );
    assert_eq!(
        *phase.compose_frame_staged(PaletteStage::default(), Some(rendering::MosaicSize::NONE)),
        *plain
    );

    let scene_frame = phase.scene.compose_live(
        &phase.player,
        &phase.save1.event_data,
        phase.tick,
        &phase.object_events,
        PaletteStage::default(),
        Some(size),
    );
    assert_eq!(*mosaicked, *crate::frame::to_platform_frame(&scene_frame));
}
