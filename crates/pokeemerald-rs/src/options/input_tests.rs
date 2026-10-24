//! Input, wraparound, and task-chain behaviour of [`OptionsMenu`] against
//! `pokeemerald/src/option_menu.c:264-372`.

use super::chrome::OptionsChrome;
use super::*;

fn menu_with(settings: GameSettings) -> OptionsMenu {
    OptionsMenu::with_chrome(OptionsChrome::synthetic(), settings)
}

/// A menu past its fade in, so the next tick reads input.
fn ready(settings: GameSettings) -> (OptionsMenu, ButtonState) {
    let mut menu = menu_with(settings);
    let buttons = ButtonState::new();
    for _ in 0..40 {
        if menu.phase() == OptionsPhase::Input {
            return (menu, buttons);
        }
        menu.tick(buttons);
    }
    panic!("the fade in never finished");
}

/// One frame with exactly `held` down.
fn frame(menu: &mut OptionsMenu, buttons: &mut ButtonState, held: Buttons) -> OptionsEvent {
    buttons.update(held);
    menu.tick(*buttons)
}

/// Presses then releases `held`, so the next press is a fresh edge.
fn tap(menu: &mut OptionsMenu, buttons: &mut ButtonState, held: Buttons) {
    frame(menu, buttons, held);
    frame(menu, buttons, Buttons::NONE);
}

fn go_to(menu: &mut OptionsMenu, buttons: &mut ButtonState, row: OptionsRow) {
    while menu.selected() != row {
        tap(menu, buttons, Buttons::DOWN);
    }
}

#[test]
fn input_is_ignored_until_the_fade_in_finishes() {
    // `Task_OptionMenuFadeIn` only switches to the input task once
    // `gPaletteFade.active` clears (`option_menu.c:258-262`).
    let mut menu = menu_with(GameSettings::default());
    let mut buttons = ButtonState::new();
    let mut frames = 0;
    while menu.phase() != OptionsPhase::Input {
        let event = frame(
            &mut menu,
            &mut buttons,
            if frames % 2 == 0 {
                Buttons::DOWN
            } else {
                Buttons::NONE
            },
        );
        assert_eq!(event, OptionsEvent::None);
        frames += 1;
        assert!(frames < 40);
    }
    assert_eq!(menu.selected(), OptionsRow::TextSpeed);
    // States 0-10 of `CB2_InitOptionMenu` (10 frames; 8 and 9 share one,
    // `option_menu.c:218-225`), state 11's
    // `BeginNormalPaletteFade` (`option_menu.c:152-254`), then `begin_in`'s
    // own update plus 21 more through the fade and its four finishing polls
    // (`palette.c:411-416,807-822`; the count `palette_fade`'s tests pin) --
    // 22 in all -- and the frame whose task polls the finished fade.
    assert_eq!(frames, 10 + 1 + 23);
}

#[test]
fn up_and_down_wrap_through_all_seven_rows() {
    // `option_menu.c:275-290`: Up from the top lands on CANCEL, Down from
    // CANCEL on the top.
    let (mut menu, mut buttons) = ready(GameSettings::default());
    assert_eq!(menu.selected(), OptionsRow::TextSpeed);
    tap(&mut menu, &mut buttons, Buttons::UP);
    assert_eq!(menu.selected(), OptionsRow::Cancel);
    tap(&mut menu, &mut buttons, Buttons::DOWN);
    assert_eq!(menu.selected(), OptionsRow::TextSpeed);
    let mut seen = Vec::new();
    for _ in 0..7 {
        tap(&mut menu, &mut buttons, Buttons::DOWN);
        seen.push(menu.selected());
    }
    assert_eq!(
        seen,
        [
            OptionsRow::BattleScene,
            OptionsRow::BattleStyle,
            OptionsRow::Sound,
            OptionsRow::ButtonMode,
            OptionsRow::FrameType,
            OptionsRow::Cancel,
            OptionsRow::TextSpeed,
        ]
    );
}

#[test]
fn a_held_arrow_does_not_repeat() {
    // Every handler reads `JOY_NEW`, not `JOY_REPEAT` (`option_menu.c:398-418`).
    let (mut menu, mut buttons) = ready(GameSettings::default());
    for _ in 0..10 {
        frame(&mut menu, &mut buttons, Buttons::RIGHT);
    }
    assert_eq!(menu.settings().text_speed, 1);
    frame(&mut menu, &mut buttons, Buttons::NONE);
    frame(&mut menu, &mut buttons, Buttons::RIGHT);
    assert_eq!(menu.settings().text_speed, 2);
    // A held Down likewise moves one row.
    for _ in 0..10 {
        frame(&mut menu, &mut buttons, Buttons::DOWN);
    }
    assert_eq!(menu.selected(), OptionsRow::BattleScene);
}

#[test]
fn text_speed_steps_and_wraps_both_ways() {
    // `TextSpeed_ProcessInput` (`option_menu.c:398-418`).
    let (mut menu, mut buttons) = ready(GameSettings::default());
    let mut right = Vec::new();
    for _ in 0..4 {
        tap(&mut menu, &mut buttons, Buttons::RIGHT);
        right.push(menu.settings().text_speed);
    }
    assert_eq!(right, [1, 2, 0, 1]);
    let mut left = Vec::new();
    for _ in 0..4 {
        tap(&mut menu, &mut buttons, Buttons::LEFT);
        left.push(menu.settings().text_speed);
    }
    assert_eq!(left, [0, 2, 1, 0]);
}

#[test]
fn three_choice_rows_apply_right_then_left_when_both_arrive() {
    // The handlers test Right and Left independently (`:400-413`), so a
    // same-frame pair nets out for text speed and button mode.
    let (mut menu, mut buttons) = ready(GameSettings::default());
    frame(&mut menu, &mut buttons, Buttons::LEFT | Buttons::RIGHT);
    assert_eq!(menu.settings().text_speed, 0);
    go_to(&mut menu, &mut buttons, OptionsRow::ButtonMode);
    frame(&mut menu, &mut buttons, Buttons::NONE);
    frame(&mut menu, &mut buttons, Buttons::LEFT | Buttons::RIGHT);
    assert_eq!(menu.settings().button_mode, ButtonMode::Normal);
}

#[test]
fn binary_rows_toggle_once_for_either_or_both_arrows() {
    // `selection ^= 1` under `JOY_NEW(DPAD_LEFT | DPAD_RIGHT)`
    // (`option_menu.c:443-453,462-472,476-487`).
    let (mut menu, mut buttons) = ready(GameSettings::default());
    go_to(&mut menu, &mut buttons, OptionsRow::BattleScene);
    tap(&mut menu, &mut buttons, Buttons::RIGHT);
    assert!(menu.settings().battle_scene_off);
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    assert!(!menu.settings().battle_scene_off);
    frame(&mut menu, &mut buttons, Buttons::LEFT | Buttons::RIGHT);
    assert!(menu.settings().battle_scene_off, "both arrows toggle once");
    frame(&mut menu, &mut buttons, Buttons::NONE);

    go_to(&mut menu, &mut buttons, OptionsRow::BattleStyle);
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    assert_eq!(menu.settings().battle_style, BattleStyle::Set);
    tap(&mut menu, &mut buttons, Buttons::RIGHT);
    assert_eq!(menu.settings().battle_style, BattleStyle::Shift);

    go_to(&mut menu, &mut buttons, OptionsRow::Sound);
    tap(&mut menu, &mut buttons, Buttons::RIGHT);
    assert_eq!(menu.settings().sound, SoundMode::Stereo);
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    assert_eq!(menu.settings().sound, SoundMode::Mono);
}

#[test]
fn button_mode_steps_through_its_three_modes() {
    // `ButtonMode_ProcessInput` (`option_menu.c:572-592`).
    let (mut menu, mut buttons) = ready(GameSettings::default());
    go_to(&mut menu, &mut buttons, OptionsRow::ButtonMode);
    let mut modes = Vec::new();
    for _ in 0..3 {
        tap(&mut menu, &mut buttons, Buttons::RIGHT);
        modes.push(menu.settings().button_mode);
    }
    assert_eq!(
        modes,
        [ButtonMode::Lr, ButtonMode::LEqualsA, ButtonMode::Normal]
    );
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    assert_eq!(menu.settings().button_mode, ButtonMode::LEqualsA);
}

#[test]
fn frame_type_wraps_over_twenty_frames() {
    // `FrameType_ProcessInput` (`option_menu.c:514-538`).
    let (mut menu, mut buttons) = ready(GameSettings::default());
    go_to(&mut menu, &mut buttons, OptionsRow::FrameType);
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    assert_eq!(menu.settings().window_frame_type, 19);
    tap(&mut menu, &mut buttons, Buttons::RIGHT);
    assert_eq!(menu.settings().window_frame_type, 0);
    for expected in 1..=19 {
        tap(&mut menu, &mut buttons, Buttons::RIGHT);
        assert_eq!(menu.settings().window_frame_type, expected);
    }
}

#[test]
fn unnameable_raw_values_step_as_upstream_does_not_normalized() {
    // Right resets a raw value above the last choice to 0; Left decrements it
    // (`option_menu.c:400-413,514-537,574-587`).
    let raw = GameSettings {
        text_speed: 6,
        window_frame_type: 27,
        button_mode: ButtonMode::Other(9),
        ..GameSettings::default()
    };
    let (mut menu, mut buttons) = ready(raw);
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    assert_eq!(menu.settings().text_speed, 5);
    go_to(&mut menu, &mut buttons, OptionsRow::ButtonMode);
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    assert_eq!(menu.settings().button_mode, ButtonMode::Other(8));
    go_to(&mut menu, &mut buttons, OptionsRow::FrameType);
    tap(&mut menu, &mut buttons, Buttons::RIGHT);
    assert_eq!(menu.settings().window_frame_type, 0);
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    tap(&mut menu, &mut buttons, Buttons::LEFT);
    assert_eq!(menu.settings().window_frame_type, 18);
}

#[test]
fn a_press_only_exits_from_cancel() {
    // `option_menu.c:266-269`: A on any other row is consumed.
    let (mut menu, mut buttons) = ready(GameSettings::default());
    tap(&mut menu, &mut buttons, Buttons::A);
    assert_eq!(menu.phase(), OptionsPhase::Input);
    tap(&mut menu, &mut buttons, Buttons::UP);
    assert_eq!(menu.selected(), OptionsRow::Cancel);
    frame(&mut menu, &mut buttons, Buttons::A);
    assert_eq!(menu.phase(), OptionsPhase::Save);
}

#[test]
fn input_precedence_is_a_then_b_then_up_then_down_then_choices() {
    // The else-if chain `option_menu.c:266-340`.
    let (mut menu, mut buttons) = ready(GameSettings::default());
    frame(
        &mut menu,
        &mut buttons,
        Buttons::A | Buttons::DOWN | Buttons::RIGHT,
    );
    assert_eq!(menu.selected(), OptionsRow::TextSpeed, "A won over Down");
    assert_eq!(menu.settings().text_speed, 0, "A won over Right");
    frame(&mut menu, &mut buttons, Buttons::NONE);
    frame(
        &mut menu,
        &mut buttons,
        Buttons::UP | Buttons::DOWN | Buttons::RIGHT,
    );
    assert_eq!(menu.selected(), OptionsRow::Cancel, "Up won over Down");
    assert_eq!(menu.settings().text_speed, 0, "a move consumed the arrows");
}

#[test]
fn b_commits_the_staged_settings_like_cancel() {
    // `Task_OptionMenuSave` runs for B too and writes every staged value
    // (`option_menu.c:270-273,351-359`).
    let (mut menu, mut buttons) = ready(GameSettings::default());
    tap(&mut menu, &mut buttons, Buttons::RIGHT);
    go_to(&mut menu, &mut buttons, OptionsRow::FrameType);
    tap(&mut menu, &mut buttons, Buttons::RIGHT);
    go_to(&mut menu, &mut buttons, OptionsRow::Sound);
    tap(&mut menu, &mut buttons, Buttons::RIGHT);
    frame(&mut menu, &mut buttons, Buttons::B);
    assert_eq!(menu.phase(), OptionsPhase::Save);
    let event = frame(&mut menu, &mut buttons, Buttons::NONE);
    assert_eq!(
        event,
        OptionsEvent::Committed(GameSettings {
            text_speed: 1,
            window_frame_type: 1,
            sound: SoundMode::Stereo,
            ..GameSettings::default()
        })
    );
}

#[test]
fn the_commit_lands_a_frame_after_the_exit_then_a_fade_out_returns() {
    // `Task_OptionMenuSave` runs the frame after the input task switches to
    // it, then waits on the fade (`option_menu.c:351-372`).
    let (mut menu, mut buttons) = ready(GameSettings::default());
    assert_eq!(
        frame(&mut menu, &mut buttons, Buttons::B),
        OptionsEvent::None
    );
    assert!(matches!(
        frame(&mut menu, &mut buttons, Buttons::NONE),
        OptionsEvent::Committed(_)
    ));
    assert_eq!(menu.phase(), OptionsPhase::FadeOut);
    let mut frames = 0;
    loop {
        // Input during the fade is ignored: A is pressed afresh every other frame.
        let held = if frames % 2 == 0 {
            Buttons::A
        } else {
            Buttons::NONE
        };
        let event = frame(&mut menu, &mut buttons, held);
        frames += 1;
        assert!(frames < 60, "the fade out never finished");
        if event == OptionsEvent::Returned {
            break;
        }
        assert_eq!(event, OptionsEvent::None);
    }
    assert_eq!(menu.phase(), OptionsPhase::Done);
    assert_eq!(
        frame(&mut menu, &mut buttons, Buttons::NONE),
        OptionsEvent::None
    );
}

#[test]
fn unnameable_raw_values_survive_open_and_close_untouched() {
    let raw = GameSettings {
        text_speed: 7,
        window_frame_type: 31,
        button_mode: ButtonMode::Other(200),
        ..GameSettings::default()
    };
    let (mut menu, mut buttons) = ready(raw);
    frame(&mut menu, &mut buttons, Buttons::B);
    assert_eq!(
        frame(&mut menu, &mut buttons, Buttons::NONE),
        OptionsEvent::Committed(raw)
    );
}

#[test]
fn setup_states_0_through_10_take_ten_frames() {
    // States 8 and 9 run in one frame (no `break`, `option_menu.c:218-225`),
    // so the fade begins on the eleventh tick.
    let mut menu = menu_with(GameSettings::default());
    for _ in 0..10 {
        menu.tick(ButtonState::new());
        assert_eq!(menu.phase(), OptionsPhase::Setup);
    }
    menu.tick(ButtonState::new());
    assert_eq!(menu.phase(), OptionsPhase::FadeIn);
}
