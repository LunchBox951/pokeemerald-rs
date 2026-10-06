//! The tileset-animation tick: per-frame advance, wrap period, and reload reset.

use super::test_support::*;
use super::OverworldPhase;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

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
