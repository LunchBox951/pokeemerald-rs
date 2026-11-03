//! Frame timing and sprite entry contract: blink and cloud-scroll cadence are pure
//! functions of the frame, and sprite entries match upstream placement and tiles.

use super::{
    cloud_scroll_y, press_start_visible, sprite_entries, NUM_COPYRIGHT_FRAMES,
    NUM_PRESS_START_FRAMES,
};
use rendering::BitDepth;

#[test]
fn press_start_blinks_every_16_ticks() {
    // Sprite timer reads `frame + 2` (title_screen.c:409-417, :675-681, :759-762), so the
    // banner turns on at frame 14, not 15, and off at frame 30, not 31 (issue #873).
    assert!(!press_start_visible(0));
    assert!(press_start_visible(14));
    assert!(press_start_visible(15));
    assert!(!press_start_visible(30));
    assert!(!press_start_visible(31));
    assert!(press_start_visible(46));
    assert!(press_start_visible(47));
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the simulated scroll stays within u16 for 20 frames"
)]
fn cloud_scroll_advances_roughly_one_pixel_every_4_ticks() {
    let mut counter: u32 = 0;
    let mut cloud_accumulator: u32 = 0;
    let mut expected = Vec::new();
    for _ in 0..20 {
        counter += 1;
        if counter & 1 != 0 {
            cloud_accumulator += 1;
        }
        expected.push((cloud_accumulator / 2) as u16);
    }
    let actual: Vec<u16> = (0..20).map(cloud_scroll_y).collect();
    assert_eq!(actual, expected);
    assert!(actual[19] > actual[0]);
}

#[test]
fn cloud_scroll_and_press_start_share_the_first_phase3_tick_as_frame_zero() {
    // `banner_timer` starts at 1: the sprite is animated once on the creation tick before
    // frame 0 (title_screen.c:409-417, :675-681, :759-763, :806-814). Issue #873.
    let mut phase3_counter: u16 = 0;
    let mut cloud_accumulator: u16 = 0;
    let mut banner_timer: u16 = 1;

    for frame in 0..32_u32 {
        phase3_counter += 1;
        if phase3_counter & 1 != 0 {
            cloud_accumulator += 1;
        }
        banner_timer += 1;

        assert_eq!(
            (cloud_scroll_y(frame), press_start_visible(frame)),
            (cloud_accumulator / 2, banner_timer & 16 != 0),
            "frame {frame}"
        );
    }
}

#[test]
fn cloud_scroll_is_a_pure_function_of_frame() {
    assert_eq!(cloud_scroll_y(37), cloud_scroll_y(37));
}

#[test]
fn cloud_scroll_wraps_like_upstreams_signed_16_bit_accumulator() {
    const LAST_POSITIVE_FRAME: u32 = 65_532;
    const FIRST_NEGATIVE_FRAME: u32 = 65_534;
    const SECOND_NEGATIVE_FRAME: u32 = 65_536;
    const ACCUMULATOR_PERIOD_FRAMES: u32 = 131_070;

    assert_eq!(
        cloud_scroll_y(LAST_POSITIVE_FRAME),
        (i16::MAX / 2).cast_unsigned()
    );
    assert_eq!(
        cloud_scroll_y(FIRST_NEGATIVE_FRAME),
        (i16::MIN / 2).cast_unsigned()
    );
    assert_eq!(
        cloud_scroll_y(SECOND_NEGATIVE_FRAME),
        ((i16::MIN + 1) / 2).cast_unsigned()
    );
    assert_eq!(cloud_scroll_y(ACCUMULATOR_PERIOD_FRAMES), cloud_scroll_y(0));
}

#[test]
fn sprite_entries_always_includes_the_settled_version_banner() {
    for frame in [0, 37, 10_000] {
        let entries = sprite_entries(frame);
        let version_banner_count = entries
            .iter()
            .filter(|e| e.bit_depth() == BitDepth::Bpp8)
            .count();
        assert_eq!(version_banner_count, 2, "frame {frame}");
        assert!(
            entries
                .iter()
                .filter(|e| e.bit_depth() == BitDepth::Bpp8)
                .all(|e| e.enabled()),
            "frame {frame}: both version banner halves must be visible"
        );
    }
}

#[test]
fn sprite_entries_convert_upstream_centers_to_oam_origins() {
    let entries = sprite_entries(0);

    assert_eq!((entries[0].x(), entries[0].y()), (66, 50));
    assert_eq!((entries[1].x(), entries[1].y()), (130, 50));
    assert_eq!(entries[0].dimensions(), (64, 32));
    assert_eq!(entries[1].dimensions(), (64, 32));

    let press_start = &entries[2..2 + NUM_PRESS_START_FRAMES];
    let press_start_origins: Vec<_> = press_start.iter().map(|entry| entry.x()).collect();
    assert_eq!(press_start_origins, [48, 80, 112, 144, 176]);
    assert!(press_start.iter().all(|entry| entry.y() == 104));
    assert!(press_start
        .iter()
        .all(|entry| entry.dimensions() == (32, 8)));

    let copyright =
        &entries[2 + NUM_PRESS_START_FRAMES..2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES];
    let copyright_origins: Vec<_> = copyright.iter().map(|entry| entry.x()).collect();
    assert_eq!(copyright_origins, [48, 80, 112, 144, 176]);
    assert!(copyright.iter().all(|entry| entry.y() == 144));
}

#[test]
fn press_start_and_copyright_entries_use_upstream_anim_frame_tiles() {
    // `sOamAnimCmds`'s frames select these exact tiles (title_screen.c:214-262,
    // sprite.c:936); bases 0 and 20 shifted both banners 8px right (#1156).
    let entries = sprite_entries(0);
    let press_start: Vec<_> = entries[2..2 + NUM_PRESS_START_FRAMES]
        .iter()
        .map(|entry| entry.tile_index())
        .collect();
    let copyright: Vec<_> = entries
        [2 + NUM_PRESS_START_FRAMES..2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES]
        .iter()
        .map(|entry| entry.tile_index())
        .collect();

    assert_eq!(press_start, [1, 5, 9, 13, 17]);
    assert_eq!(copyright, [21, 25, 29, 33, 37]);
}

#[test]
fn sprite_entries_always_includes_5_press_start_and_5_copyright_segments() {
    for frame in [0, 15, 16, 37] {
        let entries = sprite_entries(frame);
        let four_bpp_count = entries
            .iter()
            .filter(|e| e.bit_depth() == BitDepth::Bpp4)
            .count();
        assert_eq!(
            four_bpp_count,
            NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES,
            "frame {frame}"
        );
    }
}

#[test]
fn sprite_entries_never_includes_the_logo_shine() {
    let expected = 2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES;
    for frame in [0, 1, 36, 67, 1000] {
        assert_eq!(sprite_entries(frame).len(), expected, "frame {frame}");
    }
}

#[test]
fn sprite_entries_press_start_visibility_tracks_the_blink_cadence_but_copyright_never_blinks() {
    let hidden_frame_entries = sprite_entries(0);
    let press_start: Vec<_> = hidden_frame_entries[2..2 + NUM_PRESS_START_FRAMES].to_vec();
    let copyright: Vec<_> = hidden_frame_entries
        [2 + NUM_PRESS_START_FRAMES..2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES]
        .to_vec();
    assert!(press_start.iter().all(|e| !e.enabled()));
    assert!(copyright.iter().all(|e| e.enabled()));

    let visible_frame_entries = sprite_entries(15);
    let press_start: Vec<_> = visible_frame_entries[2..2 + NUM_PRESS_START_FRAMES].to_vec();
    assert!(press_start.iter().all(|e| e.enabled()));
}
