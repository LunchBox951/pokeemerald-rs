use super::summary::{badge_count, time_tokens, LineX};
use super::tests::load_synthetic_scene_with_summary;
use super::{MainMenuType, SaveSummary};
use engine::save::{PlayTime, PlayerGender, SaveBlock1, SaveBlock2};
use engine::text::Token;
use rendering::{Bgr555, Framebuffer, Rgb888};

/// An opaque foreground font index: every glyph cell paints its full advance.
const OPAQUE_FOREGROUND_FONT_INDEX: u8 = 1;
/// The CONTINUE window's content origin: tile column 2, tile row 1.
const CONTINUE_LEFT_PX: usize = 16;
const CONTINUE_TOP_PX: usize = 8;
/// The CONTINUE window's content bottom: six tile rows below its top.
const CONTINUE_BOTTOM_PX: usize = 56;

const MALE_BLUE: Rgb888 = Bgr555::from_channels(4, 16, 31).to_rgb888();
const FEMALE_PINK: Rgb888 = Bgr555::from_channels(31, 3, 21).to_rgb888();

fn text_of(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| match token {
            Token::Char(c) => *c,
            other => panic!("unexpected token {other:?}"),
        })
        .collect()
}

fn play_time(hours: u16, minutes: u8) -> PlayTime {
    PlayTime {
        hours,
        minutes,
        ..PlayTime::default()
    }
}

fn block2(name: &str, gender: PlayerGender, time: PlayTime) -> SaveBlock2 {
    let mut player_name = [engine::text::EOS; 8];
    let encoded = engine::text::encode_str(name).unwrap();
    player_name[..encoded.len()].copy_from_slice(&encoded);
    SaveBlock2 {
        player_name,
        player_gender: gender,
        play_time: time,
        ..SaveBlock2::default()
    }
}

fn block1_with_badges(flags: &[u16]) -> SaveBlock1 {
    let mut block1 = SaveBlock1::default();
    for &flag in flags {
        block1.event_data.flag_set(flag).unwrap();
    }
    block1
}

fn summary(name: &str, gender: PlayerGender, time: PlayTime, badges: &[u16]) -> SaveSummary {
    SaveSummary::from_save(&block1_with_badges(badges), &block2(name, gender, time))
}

/// `FLAG_BADGE01_GET` through `FLAG_BADGE08_GET` (`flags.h:1359-1366`).
const ALL_BADGE_FLAGS: [u16; 8] = [0x867, 0x868, 0x869, 0x86A, 0x86B, 0x86C, 0x86D, 0x86E];

#[test]
fn the_time_reads_hours_left_aligned_a_colon_and_two_digit_minutes() {
    for (hours, minutes, expected) in [
        (0, 0, "0:00"),
        (9, 5, "9:05"),
        (12, 34, "12:34"),
        (999, 59, "999:59"),
    ] {
        assert_eq!(
            text_of(&time_tokens(play_time(hours, minutes))),
            expected,
            "{hours}h {minutes}m"
        );
    }
}

#[test]
fn the_badge_count_reads_the_eight_badge_flags() {
    assert_eq!(badge_count(&SaveBlock1::default()), 0);
    assert_eq!(badge_count(&block1_with_badges(&ALL_BADGE_FLAGS)), 8);
    assert_eq!(badge_count(&block1_with_badges(&[0x867, 0x86B, 0x86E])), 3);
}

#[test]
fn the_badge_count_ignores_the_flags_either_side_of_the_badges() {
    // `FLAG_SYS_POKEDEX_GET` (0x861) is far away; 0x866 and 0x86F border the run.
    assert_eq!(badge_count(&block1_with_badges(&[0x861, 0x866, 0x86F])), 0);
}

#[test]
fn the_summary_lists_each_string_at_upstreams_column_and_row() {
    let lines = summary(
        "RUSTY",
        PlayerGender::Male,
        play_time(9, 5),
        &ALL_BADGE_FLAGS[..2],
    )
    .lines();
    let placed: Vec<_> = lines
        .iter()
        .map(|line| (line.x, line.y, text_of(&line.tokens)))
        .collect();
    assert_eq!(
        placed,
        [
            (LineX::Left(0), 17, "PLAYER".to_string()),
            (LineX::RightEdge(100), 17, "RUSTY".to_string()),
            (LineX::Left(0x6C), 17, "TIME".to_string()),
            (LineX::RightEdge(0xD0), 17, "9:05".to_string()),
            (LineX::Left(0x6C), 33, "BADGES".to_string()),
            (LineX::RightEdge(0xD0), 33, "2".to_string()),
        ]
    );
}

/// The columns of `row` painted in `color`, as inclusive `(first, last)` runs.
fn runs(fb: &Framebuffer, row: usize, color: Rgb888) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for x in 0..Framebuffer::WIDTH {
        if fb.pixel(x, row) != Some(color) {
            continue;
        }
        match runs.last_mut() {
            Some((_, last)) if *last + 1 == x => *last = x,
            _ => runs.push((x, x)),
        }
    }
    runs
}

fn compose_continue(summary: &SaveSummary) -> Framebuffer {
    load_synthetic_scene_with_summary(
        OPAQUE_FOREGROUND_FONT_INDEX,
        MainMenuType::SavedGame,
        Some(summary),
    )
    .compose()
}

#[test]
fn the_three_columns_and_two_rows_land_at_upstreams_pixels() {
    let fb = compose_continue(&summary(
        "RUSTY",
        PlayerGender::Male,
        play_time(12, 34),
        &ALL_BADGE_FLAGS[..5],
    ));

    // Row y=17: PLAYER at x=0 and the name ending at x=100, TIME at 0x6C and
    // the time ending at 0xD0, all inside the window whose content starts at
    // pixel (16, 8).
    let first_row = CONTINUE_TOP_PX + 17;
    let runs_17 = runs(&fb, first_row, MALE_BLUE);
    assert_eq!(runs_17.len(), 4, "PLAYER, name, TIME, time: {runs_17:?}");
    assert_eq!(runs_17[0].0, CONTINUE_LEFT_PX, "PLAYER starts at x=0");
    assert_eq!(runs_17[1].1, CONTINUE_LEFT_PX + 100 - 1, "name ends at 100");
    assert_eq!(runs_17[2].0, CONTINUE_LEFT_PX + 0x6C, "TIME starts at 0x6C");
    assert_eq!(
        runs_17[3].1,
        CONTINUE_LEFT_PX + 0xD0 - 1,
        "time ends at 0xD0"
    );

    // Row y=33: BADGES at 0x6C and the count ending at 0xD0.
    let second_row = CONTINUE_TOP_PX + 33;
    let runs_33 = runs(&fb, second_row, MALE_BLUE);
    assert_eq!(runs_33.len(), 2, "BADGES, count: {runs_33:?}");
    assert_eq!(
        runs_33[0].0,
        CONTINUE_LEFT_PX + 0x6C,
        "BADGES starts at 0x6C"
    );
    assert_eq!(
        runs_33[1].1,
        CONTINUE_LEFT_PX + 0xD0 - 1,
        "count ends at 0xD0"
    );

    // Nothing is printed one row above either line.
    assert!(runs(&fb, first_row - 1, MALE_BLUE).is_empty());
    assert!(runs(&fb, second_row - 1, MALE_BLUE).is_empty());
    // The left column has no second-row string.
    assert!(
        (0..CONTINUE_LEFT_PX + 0x6C).all(|x| fb.pixel(x, second_row) != Some(MALE_BLUE)),
        "no POKéDEX line is drawn on the left of the second row"
    );
}

#[test]
fn a_shorter_name_still_ends_at_the_same_right_edge() {
    let name_end = |name: &str| {
        let fb = compose_continue(&summary(name, PlayerGender::Male, play_time(0, 0), &[]));
        let row = runs(&fb, CONTINUE_TOP_PX + 17, MALE_BLUE);
        (row[1].0, row[1].1)
    };
    let (long_start, long_end) = name_end("ABCDEFG");
    let (short_start, short_end) = name_end("AB");
    assert_eq!(long_end, CONTINUE_LEFT_PX + 99);
    assert_eq!(short_end, long_end);
    assert!(
        short_start > long_start,
        "a shorter name starts further right"
    );
}

#[test]
fn the_text_is_the_male_blue_or_the_female_pink_never_the_header_grey() {
    for (gender, color) in [
        (PlayerGender::Male, MALE_BLUE),
        (PlayerGender::Female, FEMALE_PINK),
        (PlayerGender::Other(7), FEMALE_PINK),
    ] {
        let fb = compose_continue(&summary("AB", gender, play_time(0, 0), &[]));
        assert!(
            !runs(&fb, CONTINUE_TOP_PX + 17, color).is_empty(),
            "{gender:?}"
        );
    }
}

#[test]
fn the_summary_stays_inside_the_continue_window() {
    let with_summary = compose_continue(&summary(
        "ABCDEFG",
        PlayerGender::Male,
        play_time(999, 59),
        &ALL_BADGE_FLAGS,
    ));
    let without = load_synthetic_scene_with_summary(
        OPAQUE_FOREGROUND_FONT_INDEX,
        MainMenuType::SavedGame,
        None,
    )
    .compose();
    for y in 0..Framebuffer::HEIGHT {
        for x in 0..Framebuffer::WIDTH {
            let inside = (CONTINUE_TOP_PX..CONTINUE_BOTTOM_PX).contains(&y)
                && (CONTINUE_LEFT_PX..CONTINUE_LEFT_PX + 208).contains(&x);
            if !inside {
                assert_eq!(
                    with_summary.pixel(x, y),
                    without.pixel(x, y),
                    "pixel ({x}, {y}) outside the CONTINUE content changed"
                );
            }
        }
    }
    assert_ne!(with_summary.pixels(), without.pixels());
}

#[test]
fn an_unselected_continue_window_darkens_its_summary() {
    let mut scene = load_synthetic_scene_with_summary(
        OPAQUE_FOREGROUND_FONT_INDEX,
        MainMenuType::SavedGame,
        Some(&summary("AB", PlayerGender::Male, play_time(0, 0), &[])),
    );
    scene.move_down();
    let fb = scene.compose();
    let row = CONTINUE_TOP_PX + 17;
    assert!(runs(&fb, row, MALE_BLUE).is_empty());
    assert!(!runs(&fb, row, rendering::darken(MALE_BLUE, 7)).is_empty());
}

#[test]
fn the_no_saved_game_menu_ignores_a_summary() {
    let summary = summary("RUSTY", PlayerGender::Male, play_time(1, 2), &[0x867]);
    let load = |summary| {
        load_synthetic_scene_with_summary(
            OPAQUE_FOREGROUND_FONT_INDEX,
            MainMenuType::NoSavedGame,
            summary,
        )
        .compose()
    };
    assert_eq!(load(Some(&summary)).pixels(), load(None).pixels());
}

#[test]
fn an_undecodable_name_prints_no_name_but_keeps_the_other_lines() {
    // A name ending in a truncated control code (`0xFC` needs a subcode).
    let block2 = SaveBlock2 {
        player_name: [0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xC1, 0xFC],
        play_time: play_time(3, 7),
        ..SaveBlock2::default()
    };
    let lines = SaveSummary::from_save(&SaveBlock1::default(), &block2).lines();
    assert!(lines[1].tokens.is_empty(), "no name is shown");
    assert_eq!(text_of(&lines[3].tokens), "3:07");
}
