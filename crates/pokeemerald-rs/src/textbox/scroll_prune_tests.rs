//! A window scroll drops the retained ops that left the window for good,
//! without changing a pixel of what stays on screen.

use super::tests::opaque_glyph_at;
use super::*;

const CONTENT_SIZE: (i32, i32) = (208, 32);
const LINE_HEIGHT: i32 = 16;
const ORIGIN: (i32, i32) = (20, 30);
/// Scroll steps summing well past several window heights; the odd amounts
/// leave ops straddling the top edge between whole-line scrolls.
const SCROLL_STEPS: [i32; 8] = [5, 11, 16, 16, 3, 16, 16, 16];
const LONG_MESSAGE_SCROLLS: usize = 64;
const MAX_RETAINED_OPS: usize = 6;
const SCENE_COLOR: Rgb888 = Rgb888 {
    r: 90,
    g: 91,
    b: 92,
};
const BACKDROP_COLOR: Rgb888 = Rgb888 { r: 7, g: 8, b: 9 };

fn colors(background: u8) -> TextColors {
    TextColors {
        foreground: 1,
        background,
        shadow: 3,
    }
}

fn palette() -> Vec<Rgb888> {
    (0..16u8)
        .map(|i| Rgb888 {
            r: 10 + i,
            g: 100 + i,
            b: 200 + i,
        })
        .collect()
}

fn glyph_op(x: i32, y: i32) -> WindowOp {
    WindowOp::Glyph(Box::new(opaque_glyph_at(x, y)))
}

fn clear_span(x: i32, y: i32, background: u8) -> WindowOp {
    WindowOp::ClearSpan(ClearedSpan {
        x,
        y,
        width: 16,
        height: LINE_HEIGHT,
        colors: colors(background),
    })
}

fn keypad_icon(x: i32, y: i32) -> WindowOp {
    WindowOp::KeypadIcon(KeypadIconPlacement {
        x,
        y,
        id: 0,
        width: 8,
        height: 12,
    })
}

fn down_arrow(x: i32, y: i32, background: u8) -> WindowOp {
    WindowOp::DownArrow(test_down_arrow(2).draw(x, y, 1, colors(background)))
}

/// A reference scroll that shifts every op and fills the exposed rows but
/// removes nothing, so its replay is the full-history baseline.
fn scroll_keeping_everything(ops: &mut Vec<WindowOp>, dy: i32, background: u8) {
    for op in ops.iter_mut() {
        op.shift_y(-dy);
    }
    ops.push(WindowOp::ScrollFill(ClearedSpan {
        x: 0,
        y: CONTENT_SIZE.1 - dy,
        width: CONTENT_SIZE.0,
        height: dy,
        colors: colors(background),
    }));
}

fn compose(ops: &[WindowOp]) -> Framebuffer {
    let scene = {
        let mut scene = Framebuffer::new();
        scene.fill(SCENE_COLOR);
        scene
    };
    let mut fb = Framebuffer::new();
    fb.fill(BACKDROP_COLOR);
    compose_window_ops(
        &mut fb,
        ops,
        ORIGIN,
        CONTENT_SIZE,
        &test_keypad_icons(&[(0, 0, 2), (3, 3, 3), (7, 11, 2)]),
        &palette(),
        WindowBackdrop::message_box(None, Some(&scene)),
    );
    fb
}

fn is_wholly_above_window(op: &WindowOp) -> bool {
    match op {
        WindowOp::Glyph(g) => g.y + LINE_HEIGHT <= 0,
        WindowOp::ClearSpan(s) | WindowOp::ScrollFill(s) => s.y + s.height <= 0,
        WindowOp::KeypadIcon(i) => i.y + i32::from(i.height) <= 0,
        WindowOp::DownArrow(a) => a.y + a.height <= 0,
    }
}

#[test]
fn scrolling_drops_ops_wholly_above_the_window() {
    let mut ops = vec![glyph_op(0, 1)];
    for _ in 0..3 {
        scroll_window_ops(&mut ops, LINE_HEIGHT, colors(1), CONTENT_SIZE);
    }
    let retained_above = ops.iter().filter(|op| is_wholly_above_window(op)).count();
    assert_eq!(retained_above, 0, "retained off-screen ops: {ops:?}");
}

#[test]
fn a_long_message_retains_only_the_work_inside_the_window() {
    let mut ops = Vec::new();
    for _ in 0..LONG_MESSAGE_SCROLLS {
        ops.push(glyph_op(0, LINE_HEIGHT + 1));
        ops.push(clear_span(20, LINE_HEIGHT + 1, 1));
        scroll_window_ops(&mut ops, LINE_HEIGHT, colors(1), CONTENT_SIZE);
        assert!(
            ops.len() <= MAX_RETAINED_OPS,
            "the list must stay bounded by the window, not the scroll history: {}",
            ops.len()
        );
    }
    // The last two lines' glyphs and spans, and the last two fills: all that
    // still overlap the 32-row window.
    assert_eq!(ops.len(), MAX_RETAINED_OPS, "{ops:?}");
    assert!(ops.iter().all(|op| !is_wholly_above_window(op)));
}

#[test]
fn an_op_is_dropped_only_once_its_last_row_has_left_the_window() {
    let mut ops = vec![glyph_op(0, 1)];
    let glyph_survives = |ops: &[WindowOp]| ops.iter().any(|op| matches!(op, WindowOp::Glyph(_)));

    scroll_window_ops(&mut ops, 8, colors(1), CONTENT_SIZE);
    scroll_window_ops(&mut ops, 8, colors(1), CONTENT_SIZE);
    // y == -15: one row of the glyph is still in the window.
    assert!(glyph_survives(&ops));

    scroll_window_ops(&mut ops, 1, colors(1), CONTENT_SIZE);
    // y == -16: the cell lies wholly above row 0.
    assert!(!glyph_survives(&ops));
}

#[test]
fn every_kind_of_op_is_dropped_once_it_is_above_the_window() {
    let mut ops = vec![
        glyph_op(0, 0),
        clear_span(0, 0, 1),
        keypad_icon(0, 0),
        down_arrow(0, 0, 1),
        WindowOp::ScrollFill(ClearedSpan {
            x: 0,
            y: 0,
            width: 8,
            height: LINE_HEIGHT,
            colors: colors(1),
        }),
    ];
    scroll_window_ops(&mut ops, LINE_HEIGHT, colors(1), CONTENT_SIZE);
    assert_eq!(ops.len(), 1, "only the new fill remains: {ops:?}");
    assert!(matches!(ops[0], WindowOp::ScrollFill(_)));
}

#[test]
fn pruning_leaves_every_composed_pixel_unchanged_across_many_scrolls() {
    // Backgrounds: transparent (shows the scene), the interior index, and an
    // opaque palette index.
    for background in [0, 1, 5] {
        let mut pruned = vec![
            glyph_op(0, 1),
            glyph_op(20, 1),
            clear_span(40, 1, background),
            keypad_icon(60, 2),
            down_arrow(100, 16, background),
            glyph_op(0, LINE_HEIGHT + 1),
        ];
        let mut unpruned = pruned.clone();
        assert_eq!(compose(&pruned).pixels(), compose(&unpruned).pixels());

        for (step, dy) in SCROLL_STEPS.into_iter().enumerate() {
            scroll_window_ops(&mut pruned, dy, colors(background), CONTENT_SIZE);
            scroll_keeping_everything(&mut unpruned, dy, background);
            // Text revealed on the freshly exposed line, as the printer does
            // after each scroll.
            let row = CONTENT_SIZE.1 - LINE_HEIGHT + 1;
            let x = i32::try_from(step).unwrap() * 12;
            for ops in [&mut pruned, &mut unpruned] {
                ops.push(glyph_op(x, row));
                ops.push(clear_span(x + 100, row, background));
            }

            assert_eq!(
                compose(&pruned).pixels(),
                compose(&unpruned).pixels(),
                "background {background}, step {step}: the visible pixels must not change"
            );
        }
        assert!(pruned.iter().all(|op| !is_wholly_above_window(op)));
        assert!(pruned.len() < unpruned.len(), "pruning must shed ops");
    }
}
