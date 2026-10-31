//! Tests for the palette stage and BG mosaic hook on live frame composition:
//! the identity stage, fades blanking the field, per-bank staging, overlays
//! staged once, and the BG mosaic reaching the field and not the avatar.

use super::test_support::*;
use super::OverworldPhase;
use crate::palette_overlay::IdentityTransform;
use engine::overworld::{Direction, PlayerState};
use rendering::{
    Framebuffer, NormalPaletteFade, PaletteColorTransform, PaletteFadeTarget, PaletteStage,
};

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
