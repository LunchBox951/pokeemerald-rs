//! Pins the cross-frame vertical enable latch against a literal restatement of
//! mGBA's `StepWindow` and `FinishFrame` (`video-software.c:758-771,881-891`).

use super::{WindowConfig, WindowLayerEnable, WindowPass, WindowRange, WindowRect, WindowRegion};

const SAMPLE: [u8; 18] = [
    0, 1, 39, 40, 50, 70, 99, 100, 120, 159, 160, 161, 200, 227, 228, 229, 240, 255,
];

/// Steps one frame the way the C code does and returns the per-line states
/// and the latch left after `FinishFrame`.
fn reference_frame(start: u8, end: u8, incoming: bool) -> ([bool; 160], bool) {
    let mut on = incoming;
    let mut lines = [false; 160];
    for (y, line) in lines.iter_mut().enumerate() {
        if usize::from(start) == y {
            on = true;
        }
        if usize::from(end) == y {
            on = false;
        }
        *line = on;
    }
    if (160..228).contains(&end) {
        on = false;
    }
    if (160..228).contains(&start) && start > end {
        on = true;
    }
    (lines, on)
}

#[test]
fn explicit_latch_matches_the_reference_over_three_frames() {
    for &start in &SAMPLE {
        for &end in &SAMPLE {
            for first in [false, true] {
                let mut incoming = first;
                for frame in 0..3 {
                    let (lines, outgoing) = reference_frame(start, end, incoming);
                    let range = WindowRange::new(start, end);
                    let carried = range.with_vertical_latch(incoming);
                    for (y, expected) in lines.iter().enumerate() {
                        let y = u8::try_from(y).unwrap();
                        let context = format!("({start},{end}) in={first} frame={frame} y={y}");
                        assert_eq!(
                            range.contains_vertical_with_latch(y, incoming),
                            *expected,
                            "{context}"
                        );
                        assert_eq!(carried.contains_vertical(y), *expected, "{context}");
                    }
                    assert_eq!(
                        range.next_frame_latch(incoming),
                        outgoing,
                        "({start},{end}) in={first} frame={frame} outgoing"
                    );
                    incoming = outgoing;
                }
            }
        }
    }
}

#[test]
fn every_endpoint_pair_matches_the_reference() {
    for start in 0..=255u8 {
        for end in 0..=255u8 {
            for incoming in [false, true] {
                let (lines, outgoing) = reference_frame(start, end, incoming);
                let range = WindowRange::new(start, end);
                assert_eq!(range.next_frame_latch(incoming), outgoing);
                for (y, expected) in lines.iter().enumerate() {
                    let y = u8::try_from(y).unwrap();
                    assert_eq!(
                        range.contains_vertical_with_latch(y, incoming),
                        *expected,
                        "({start},{end}) in={incoming} y={y}"
                    );
                }
            }
        }
    }
}

#[test]
fn new_seeds_the_steady_state_of_unchanged_registers() {
    for &start in &SAMPLE {
        for &end in &SAMPLE {
            let (_, second_incoming) = reference_frame(start, end, false);
            let (second, _) = reference_frame(start, end, second_incoming);
            let range = WindowRange::new(start, end);
            for (y, expected) in second.iter().enumerate() {
                let y = u8::try_from(y).unwrap();
                assert_eq!(
                    range.contains_vertical(y),
                    *expected,
                    "({start},{end}) y={y}"
                );
            }
        }
    }
}

#[test]
fn end_past_vblank_holds_the_window_open_across_two_frames() {
    let range = WindowRange::new(100, 240);

    let first = range.with_vertical_latch(false);
    assert!(!first.contains_vertical(0));
    assert!(!first.contains_vertical(99));
    assert!(first.contains_vertical(100));
    assert!(first.contains_vertical(159));
    let carried = range.next_frame_latch(false);
    assert!(
        carried,
        "end 240 is never reached, so FinishFrame keeps it on"
    );

    let second = range.with_vertical_latch(carried);
    assert!(second.contains_vertical(0));
    assert!(second.contains_vertical(99));
    assert!(second.contains_vertical(100));
    assert!(second.contains_vertical(159));
    assert!(range.next_frame_latch(carried));
}

#[test]
fn register_change_preserves_an_already_open_latch() {
    let open = WindowRange::new(100, 240).next_frame_latch(false);
    assert!(open);

    let moved = WindowRange::new(120, 240).with_vertical_latch(open);
    assert!(moved.contains_vertical(0), "write did not reset the latch");
    assert!(moved.contains_vertical(119));
    assert!(moved.contains_vertical(159));

    let closing = WindowRange::new(50, 70);
    let kept = closing.with_vertical_latch(open);
    for y in [0, 49, 50, 69] {
        assert!(kept.contains_vertical(y), "y={y}");
    }
    for y in [70, 159] {
        assert!(!kept.contains_vertical(y), "y={y}");
    }
    assert!(!closing.next_frame_latch(open), "end 70 closes it");

    let fresh = closing.with_vertical_latch(false);
    assert!(!fresh.contains_vertical(0));
    assert!(!fresh.contains_vertical(49));
    assert!(fresh.contains_vertical(50));
}

#[test]
fn equal_visible_endpoints_close_an_open_latch_at_that_line() {
    let range = WindowRange::new(50, 50);
    assert!(range.contains_vertical_with_latch(49, true));
    assert!(!range.contains_vertical_with_latch(50, true));
    assert!(!range.contains_vertical_with_latch(159, true));
    assert!(!range.next_frame_latch(true));
}

#[test]
fn unreachable_start_does_not_force_an_open_latch_off() {
    let range = WindowRange::new(240, 40);
    assert!(range.contains_vertical_with_latch(39, true));
    assert!(!range.contains_vertical_with_latch(40, true));
    assert!(!range.contains_vertical_with_latch(0, false));
}

#[test]
fn finish_frame_clears_at_end_then_reopens_at_start() {
    let range = WindowRange::new(220, 200);
    assert!(!range.contains_vertical_with_latch(159, false));
    assert!(range.next_frame_latch(false));
}

fn rect_with_latch(x: (u8, u8), y: (u8, u8), incoming: bool) -> WindowRect {
    WindowRect::new(
        WindowRange::new(x.0, x.1),
        WindowRange::new(y.0, y.1).with_vertical_latch(incoming),
    )
}

fn bg_only(index: usize) -> WindowLayerEnable {
    let mut enable = WindowLayerEnable::NONE;
    enable.bg[index] = true;
    enable
}

#[test]
fn classification_follows_the_carried_latch_for_both_windows() {
    for incoming in [false, true] {
        let config = WindowConfig {
            win0: Some((rect_with_latch((0, 10), (100, 240), incoming), bg_only(0))),
            win1: Some((rect_with_latch((20, 30), (100, 240), incoming), bg_only(1))),
            obj_window: None,
            winout: bg_only(3),
        };
        let (win0_layers, win0_region) = config.classify_with_region(5, 0, false);
        let (win1_layers, win1_region) = config.classify_with_region(25, 0, false);
        if incoming {
            assert_eq!(win0_region, WindowRegion::Win0);
            assert_eq!(win0_layers, bg_only(0));
            assert_eq!(win1_region, WindowRegion::Win1);
            assert_eq!(win1_layers, bg_only(1));
        } else {
            assert_eq!(win0_region, WindowRegion::WinOut);
            assert_eq!(win1_region, WindowRegion::WinOut);
        }
        assert_eq!(config.classify(5, 100, false), bg_only(0));
    }
}

#[test]
fn scanline_passes_use_the_carried_latch_for_an_empty_horizontal_window() {
    let enable = bg_only(0);
    let build = |incoming| WindowConfig {
        win0: Some((rect_with_latch((5, 5), (100, 240), incoming), enable)),
        win1: None,
        obj_window: None,
        winout: WindowLayerEnable::NONE,
    };

    let closed = build(false).scanline_passes(0);
    assert_eq!(closed.len(), 1, "no window on this line: {closed:?}");

    let open = build(true).scanline_passes(0);
    let expected_window_pass = WindowPass::new(5, enable, WindowRegion::Win0);
    assert_eq!(open.len(), 3, "{open:?}");
    assert_eq!(open[1], expected_window_pass);
}
