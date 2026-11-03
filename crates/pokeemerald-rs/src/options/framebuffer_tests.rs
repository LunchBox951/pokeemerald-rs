//! Geometry, colour, darkening, and fade of the composed Options screen
//! against `pokeemerald/src/option_menu.c:90-109,374-395,421-441,541-569,618-665`.
//!
//! The fixture font draws one foreground pixel at the top-left of each glyph
//! cell, so a string's start is exactly one probe pixel.

use super::chrome::OptionsChrome;
use super::*;
use rendering::{darken, Bgr555, PaletteColorTransform, Rgb888};

const LIST_ORIGIN: (usize, usize) = (16, 40);
const BACKDROP: Rgb888 = Bgr555::from_channels(17, 18, 31).to_rgb888();
const WHITE: Rgb888 = Bgr555::from_channels(31, 31, 31).to_rgb888();
const LABEL_FG: Rgb888 = Bgr555::from_channels(31, 22, 10).to_rgb888();
const SELECTED_FG: Rgb888 = Bgr555::from_channels(31, 17, 16).to_rgb888();
const UNSELECTED_FG: Rgb888 = Bgr555::from_channels(9, 9, 9).to_rgb888();

fn compose(settings: GameSettings, selected: OptionsRow) -> Framebuffer {
    OptionsChrome::synthetic().compose(settings, selected, None)
}

fn pixel(fb: &Framebuffer, x: usize, y: usize) -> Rgb888 {
    fb.pixel(x, y).expect("probe inside the framebuffer")
}

/// The pixel where a string drawn at list-window `x` on `row` begins.
fn probe(fb: &Framebuffer, row: OptionsRow, x: i32) -> Rgb888 {
    let x = LIST_ORIGIN.0 + usize::try_from(x).unwrap();
    pixel(fb, x, LIST_ORIGIN.1 + row.index() * 16 + 1)
}

fn width(text: &str) -> i32 {
    OptionsChrome::synthetic().label(text).width
}

#[test]
fn the_backdrop_header_and_borders_are_never_darkened() {
    // Only BG0 is a `BLDCNT` target (`option_menu.c:186`), and the list is the
    // only BG0 window.
    let fb = compose(GameSettings::default(), OptionsRow::Cancel);
    assert_eq!(pixel(&fb, 0, 0), BACKDROP, "sOptionMenuBg_Pal (`:135`)");
    assert_eq!(pixel(&fb, 200, 12), WHITE, "header content is BG1");
    let border = OptionsChrome::synthetic_frame_color(0);
    assert_eq!(pixel(&fb, 10, 100), border, "list border is BG1");
    assert_eq!(pixel(&fb, 10, 12), border, "header border is BG1");
}

#[test]
fn only_the_selected_row_escapes_the_bg0_darken() {
    // `HighlightOptionMenuItem` WIN0 = x 16..224, y row*16+40..row*16+56
    // (`:374-379`); outside it BG0 darkens by `BLDY` 4 (`:187`).
    for row in OptionsRow::ALL {
        let fb = compose(GameSettings::default(), row);
        for other in OptionsRow::ALL {
            let y = LIST_ORIGIN.1 + other.index() * 16;
            let expected = if other == row {
                WHITE
            } else {
                darken(WHITE, 4)
            };
            assert_eq!(
                pixel(&fb, 215, y + 8),
                expected,
                "{other:?} while {row:?} is selected"
            );
            // The row's first and last pixel rows belong to its WIN0 range.
            assert_eq!(pixel(&fb, 215, y), expected);
            assert_eq!(pixel(&fb, 215, y + 15), expected);
        }
    }
}

#[test]
fn row_names_sit_at_x_8_one_pixel_below_each_row_top() {
    // `DrawOptionMenuTexts` (`:652-660`).
    let fb = compose(GameSettings::default(), OptionsRow::TextSpeed);
    assert_eq!(probe(&fb, OptionsRow::TextSpeed, 8), LABEL_FG);
    let dim = compose(GameSettings::default(), OptionsRow::Cancel);
    assert_eq!(probe(&dim, OptionsRow::TextSpeed, 8), darken(LABEL_FG, 4));
}

#[test]
fn text_speed_choices_use_the_three_way_layout_and_mark_the_selection() {
    // `TextSpeed_DrawChoices` (`:421-441`): SLOW at 104, FAST right-aligned
    // to 198, MID at `(wSlow - (wMid - 94) - wFast) / 2 + 104`.
    let slow = width("SLOW");
    let mid_x = (slow - (width("MID") - 94) - width("FAST")) / 2 + 104;
    let fast_x = 198 - width("FAST");
    for (speed, expected) in [
        (0u8, [true, false, false]),
        (1, [false, true, false]),
        (2, [false, false, true]),
    ] {
        let fb = compose(
            GameSettings {
                text_speed: speed,
                ..GameSettings::default()
            },
            OptionsRow::TextSpeed,
        );
        for ((x, selected), name) in [
            (104, expected[0]),
            (mid_x, expected[1]),
            (fast_x, expected[2]),
        ]
        .into_iter()
        .zip(["SLOW", "MID", "FAST"])
        {
            let color = if selected { SELECTED_FG } else { UNSELECTED_FG };
            assert_eq!(
                probe(&fb, OptionsRow::TextSpeed, x),
                color,
                "{name} at speed {speed}"
            );
        }
    }
}

#[test]
fn an_unnameable_text_speed_marks_no_choice() {
    let fb = compose(
        GameSettings {
            text_speed: 5,
            ..GameSettings::default()
        },
        OptionsRow::TextSpeed,
    );
    for x in [104, 198 - width("FAST")] {
        assert_eq!(probe(&fb, OptionsRow::TextSpeed, x), UNSELECTED_FG);
    }
}

#[test]
fn two_way_rows_left_align_the_first_and_right_align_the_second() {
    // `BattleScene_DrawChoices` etc. (`:443-460,462-474,476-509`). The scene
    // flag is "off", so ON is selected while it is clear.
    let right = |text: &str| 198 - width(text);
    let fb = compose(GameSettings::default(), OptionsRow::BattleScene);
    assert_eq!(probe(&fb, OptionsRow::BattleScene, 104), SELECTED_FG);
    assert_eq!(
        probe(&fb, OptionsRow::BattleScene, right("OFF")),
        UNSELECTED_FG
    );

    let fb = compose(
        GameSettings {
            battle_style: BattleStyle::Set,
            sound: SoundMode::Stereo,
            battle_scene_off: true,
            ..GameSettings::default()
        },
        OptionsRow::BattleScene,
    );
    assert_eq!(
        probe(&fb, OptionsRow::BattleScene, right("OFF")),
        SELECTED_FG
    );
    let dark = |c| darken(c, 4);
    assert_eq!(
        probe(&fb, OptionsRow::BattleStyle, 104),
        dark(UNSELECTED_FG)
    );
    assert_eq!(
        probe(&fb, OptionsRow::BattleStyle, right("SET")),
        dark(SELECTED_FG)
    );
    assert_eq!(probe(&fb, OptionsRow::Sound, 104), dark(UNSELECTED_FG));
    assert_eq!(
        probe(&fb, OptionsRow::Sound, right("STEREO")),
        dark(SELECTED_FG)
    );
}

#[test]
fn button_mode_choices_use_the_three_way_layout() {
    // `ButtonMode_DrawChoices` (`:594-615`).
    let mid_x = (width("NORMAL") - (width("LR") - 94) - width("L=A")) / 2 + 104;
    for (mode, selected) in [
        (ButtonMode::Normal, Some(0)),
        (ButtonMode::Lr, Some(1)),
        (ButtonMode::LEqualsA, Some(2)),
        (ButtonMode::Other(7), None),
    ] {
        let fb = compose(
            GameSettings {
                button_mode: mode,
                ..GameSettings::default()
            },
            OptionsRow::ButtonMode,
        );
        for (index, x) in [104, mid_x, 198 - width("L=A")].into_iter().enumerate() {
            let color = if selected == Some(index) {
                SELECTED_FG
            } else {
                UNSELECTED_FG
            };
            assert_eq!(
                probe(&fb, OptionsRow::ButtonMode, x),
                color,
                "{mode:?} choice {index}"
            );
        }
    }
}

#[test]
fn frame_row_shows_type_then_the_one_based_number_selected() {
    // `FrameType_DrawChoices` (`:541-570`): TYPE at 104 plain, the number at
    // 128 in the selected colour.
    let fb = compose(
        GameSettings {
            window_frame_type: 19,
            ..GameSettings::default()
        },
        OptionsRow::FrameType,
    );
    assert_eq!(probe(&fb, OptionsRow::FrameType, 104), UNSELECTED_FG);
    assert_eq!(probe(&fb, OptionsRow::FrameType, 128), SELECTED_FG);
    // "20" is two glyphs: the second starts one advance after the first.
    assert_eq!(
        probe(&fb, OptionsRow::FrameType, 128 + width("2")),
        SELECTED_FG
    );
    let single = compose(GameSettings::default(), OptionsRow::FrameType);
    assert_eq!(
        probe(&single, OptionsRow::FrameType, 128 + width("1")),
        WHITE
    );
}

#[test]
fn the_border_previews_every_frame_and_falls_back_to_frame_zero() {
    // `LoadBgTiles`/`LoadPalette` of `GetWindowFrameTilesPal(selection)`
    // (`:520-523`); an unnameable id draws frame 0 and is not written back.
    for id in 0..WINDOW_FRAMES_COUNT {
        let fb = compose(
            GameSettings {
                window_frame_type: id,
                ..GameSettings::default()
            },
            OptionsRow::Cancel,
        );
        assert_eq!(
            pixel(&fb, 10, 100),
            OptionsChrome::synthetic_frame_color(id)
        );
    }
    for id in [20, 31] {
        let fb = compose(
            GameSettings {
                window_frame_type: id,
                ..GameSettings::default()
            },
            OptionsRow::Cancel,
        );
        assert_eq!(pixel(&fb, 10, 100), OptionsChrome::synthetic_frame_color(0));
    }
}

#[test]
fn the_fade_in_starts_black_and_the_input_phase_is_unfaded() {
    // `BeginNormalPaletteFade(PALETTES_ALL, 0, 16, 0, RGB_BLACK)`
    // (`option_menu.c:250`).
    let mut menu = OptionsMenu::with_chrome(OptionsChrome::synthetic(), GameSettings::default());
    // Setup is built behind black (`option_menu.c:152-249`).
    let first = menu.compose();
    assert!((0..Framebuffer::HEIGHT)
        .all(|y| { (0..Framebuffer::WIDTH).all(|x| first.pixel(x, y) == Some(Rgb888::BLACK)) }));
    let buttons = ButtonState::new();
    while menu.phase() != OptionsPhase::Input {
        menu.tick(buttons);
    }
    assert_eq!(pixel(&menu.compose(), 0, 0), BACKDROP);
}

#[test]
fn the_palette_fade_runs_before_the_bg0_darken() {
    // The CPU fade rewrites palette entries (`palette.c:442-449`) and the
    // BLDY darken is the later hardware effect (`option_menu.c:180-184`), so
    // an unselected white list pixel is `darken(fade(white))`, not the
    // reverse -- the two quantize to BGR555 differently.
    let mut fade = rendering::NormalPaletteFade::begin_in(rendering::PaletteFadeTarget::Black);
    for _ in 0..8 {
        fade.update();
    }
    let blend = fade.bg_blend();
    assert!(
        (1..16).contains(&blend.coefficient()),
        "an intermediate fade"
    );
    let fb = OptionsChrome::synthetic().compose(
        GameSettings::default(),
        OptionsRow::TextSpeed,
        Some(blend),
    );
    let unselected = pixel(&fb, 215, LIST_ORIGIN.1 + 16 + 8);
    assert_eq!(unselected, darken(blend.transform(WHITE), 4));
    assert_ne!(unselected, blend.transform(darken(WHITE, 4)));
    assert_eq!(pixel(&fb, 215, LIST_ORIGIN.1 + 8), blend.transform(WHITE));
}
