//! The drawn battle message window: its bounds, colors, and what stays on
//! screen between messages.
//!
//! `B_WIN_MSG` is a 26x4-tile window at tilemap (2, 15) whose text starts at
//! window pixel (0, 1) with palette-bank-0 colors 1 (body), 6 (shadow), and 15
//! (background) (`src/battle_bg.c:165`-`:172`, `src/battle_message.c:1480`-`:1488`).

use engine::text::render::TextSpeed;
use rendering::{Bgr555, BitDepth, Framebuffer, Palette, Rgb888, ScreenEntry, Tilemap, Tileset};

use super::fixtures::Fixture;
use super::presentation_support::{
    layer_over, message, run, run_same, submit, text_fixture, TestSheet, A_HELD, A_PRESS, NONE,
};
use crate::battle_narration::message_box::BattleTextbox;
use crate::battle_narration::BattleMessageLayer;

const FRAME_INDEX: u8 = 3;
const BODY_INDEX: u8 = 1;
const SHADOW_INDEX: u8 = 6;
const BACKGROUND_INDEX: u8 = 15;
/// Window content bounds in screen pixels.
const LEFT: usize = 16;
const TOP: usize = 120;
const WIDTH: usize = 208;
const HEIGHT: usize = 32;

fn color(r: u8, g: u8, b: u8) -> Bgr555 {
    Bgr555::from_channels(r, g, b)
}

/// A textbox whose every tile is the frame color, so anything the window
/// draws shows against it.
fn layer() -> BattleMessageLayer<TestSheet> {
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[usize::from(FRAME_INDEX)] = color(1, 2, 3);
    colors[usize::from(BODY_INDEX)] = color(31, 0, 0);
    colors[usize::from(SHADOW_INDEX)] = color(0, 31, 0);
    colors[usize::from(BACKGROUND_INDEX)] = color(0, 0, 31);
    let frame_tile = vec![FRAME_INDEX | (FRAME_INDEX << 4); BitDepth::Bpp4.tile_byte_len()];
    let textbox = BattleTextbox::from_parts(
        Tileset::decode(BitDepth::Bpp4, &frame_tile).unwrap(),
        Palette::new(colors),
        Tilemap::new(32, 32, vec![ScreenEntry::new(0, false, false, 0); 32 * 32]).unwrap(),
    );
    layer_over(textbox, TextSpeed::Fast)
}

fn rgb(color: Bgr555) -> Rgb888 {
    color.to_rgb888()
}

fn drawn(layer: &BattleMessageLayer<TestSheet>) -> Framebuffer {
    let mut framebuffer = Framebuffer::new();
    layer.draw(&mut framebuffer);
    framebuffer
}

fn pixel(framebuffer: &Framebuffer, x: usize, y: usize) -> Rgb888 {
    framebuffer.pixel(x, y).unwrap()
}

#[test]
fn an_idle_layer_draws_the_textbox_around_a_blank_window() {
    let framebuffer = drawn(&layer());

    let frame = rgb(color(1, 2, 3));
    let background = rgb(color(0, 0, 31));
    assert_eq!(pixel(&framebuffer, LEFT - 1, TOP), frame);
    assert_eq!(pixel(&framebuffer, LEFT, TOP - 1), frame);
    assert_eq!(pixel(&framebuffer, LEFT + WIDTH, TOP), frame);
    assert_eq!(pixel(&framebuffer, LEFT, TOP + HEIGHT), frame);
    assert_eq!(pixel(&framebuffer, LEFT, TOP), background);
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 1),
        background
    );
}

#[test]
fn a_glyph_lands_one_pixel_into_the_window_in_the_windows_text_colors() {
    let fixture = text_fixture();
    let mut layer = layer();
    submit(&mut layer, &fixture, &[message::AB]);
    run(&mut layer, &[NONE]);

    let framebuffer = drawn(&layer);

    assert_eq!(pixel(&framebuffer, LEFT, TOP), rgb(color(0, 0, 31)));
    assert_eq!(pixel(&framebuffer, LEFT, TOP + 1), rgb(color(31, 0, 0)));
    assert_eq!(
        pixel(&framebuffer, LEFT + 15, TOP + 1),
        rgb(color(31, 0, 0))
    );
    assert_eq!(pixel(&framebuffer, LEFT, TOP + 2), rgb(color(0, 31, 0)));
    assert_eq!(pixel(&framebuffer, LEFT - 1, TOP + 1), rgb(color(1, 2, 3)));
}

#[test]
fn a_finished_message_stays_up_until_the_next_one_starts_and_replaces_it() {
    let fixture = text_fixture();
    let mut layer = layer();
    submit(&mut layer, &fixture, &[message::AB]);
    run_same(&mut layer, 3, NONE);
    let second_glyph_x = LEFT + 8;
    assert_eq!(
        pixel(&drawn(&layer), second_glyph_x + 15, TOP + 1),
        rgb(color(31, 0, 0))
    );

    submit(&mut layer, &fixture, &[message::AB_AGAIN]);
    run(&mut layer, &[NONE]);

    // Only the new message's first glyph (x 16..32) remains.
    let framebuffer = drawn(&layer);
    assert_eq!(
        pixel(&framebuffer, LEFT + 15, TOP + 1),
        rgb(color(31, 0, 0))
    );
    assert_eq!(
        pixel(&framebuffer, second_glyph_x + 15, TOP + 1),
        rgb(color(0, 0, 31))
    );
}

#[test]
fn a_page_clear_blanks_the_window_before_the_next_page_prints() {
    let fixture = text_fixture();
    let mut layer = layer();
    submit(&mut layer, &fixture, &[message::PAGE_CLEAR]);
    run(&mut layer, &[NONE, NONE, A_HELD]);
    assert_eq!(pixel(&drawn(&layer), LEFT, TOP + 1), rgb(color(31, 0, 0)));

    run(&mut layer, &[A_PRESS]);

    assert_eq!(pixel(&drawn(&layer), LEFT, TOP + 1), rgb(color(0, 0, 31)));
}

#[test]
fn a_scroll_carries_the_printed_line_out_of_the_window() {
    let fixture = text_fixture();
    let mut layer = layer();
    submit(&mut layer, &fixture, &[message::SCROLL]);
    run(&mut layer, &[NONE, NONE, A_PRESS]);
    assert_eq!(pixel(&drawn(&layer), LEFT, TOP + 1), rgb(color(31, 0, 0)));

    // The first scroll frame moves the glyph up 4 pixels, past its two
    // painted rows, so the window already reads blank.
    run_same(&mut layer, 1, NONE);

    let framebuffer = drawn(&layer);
    assert_eq!(pixel(&framebuffer, LEFT, TOP + 1), rgb(color(0, 0, 31)));
    assert_eq!(pixel(&framebuffer, LEFT, TOP + 2), rgb(color(0, 0, 31)));
}

#[test]
fn a_glyph_clipped_below_the_window_stays_hidden_after_a_scroll() {
    let fixture = Fixture::new(&[(110, "{SHIFT_DOWN 31}A\\l")], &[]);
    let mut layer = layer();
    submit(&mut layer, &fixture, &[110]);
    // A lands at window y = 32, wholly below the 32-pixel window.
    run(&mut layer, &[NONE]);
    let background = rgb(color(0, 0, 31));
    for y in 0..HEIGHT {
        assert_eq!(
            pixel(&drawn(&layer), LEFT, TOP + y),
            background,
            "before scroll y={y}"
        );
    }
    // Answer the prompt and let the 16-pixel scroll run out.
    let mut ticks = run(&mut layer, &[NONE, A_PRESS]);
    ticks.extend(run_same(&mut layer, 12, NONE));
    let framebuffer = drawn(&layer);
    for y in 0..HEIGHT {
        assert_eq!(
            pixel(&framebuffer, LEFT, TOP + y),
            background,
            "after scroll y={y}"
        );
    }
}

#[test]
fn a_glyph_straddling_the_bottom_edge_keeps_only_its_revealed_rows_after_a_scroll() {
    let fixture = Fixture::new(&[(110, "{SHIFT_DOWN 30}A\\l")], &[]);
    let mut layer = layer();
    submit(&mut layer, &fixture, &[110]);
    // A lands at window y = 31: its body row is the last window row and its
    // shadow row falls below the window.
    run(&mut layer, &[NONE]);
    let background = rgb(color(0, 0, 31));
    let body = rgb(color(31, 0, 0));
    let shadow = rgb(color(0, 31, 0));
    assert_eq!(pixel(&drawn(&layer), LEFT, TOP + 31), body);

    run(&mut layer, &[NONE, A_PRESS]);
    run_same(&mut layer, 12, NONE);

    // The scroll moved the body row up 16 pixels; the shadow row was never
    // drawn, so it does not appear at y = 16.
    let framebuffer = drawn(&layer);
    for y in 0..HEIGHT {
        let expected = if y == 15 { body } else { background };
        assert_eq!(pixel(&framebuffer, LEFT, TOP + y), expected, "y={y}");
        assert_ne!(pixel(&framebuffer, LEFT, TOP + y), shadow, "y={y}");
    }
}

/// A colour no textbox palette entry uses, standing for the layers beneath
/// BG0.
const SCENE: Rgb888 = Rgb888 {
    r: 10,
    g: 200,
    b: 30,
};

fn drawn_over_scene(layer: &BattleMessageLayer<TestSheet>) -> Framebuffer {
    let mut framebuffer = Framebuffer::new();
    framebuffer.fill(SCENE);
    layer.draw(&mut framebuffer);
    framebuffer
}

/// Prints `text` as a battle message and runs `frames` idle frames.
fn printed(text: &str, frames: usize) -> BattleMessageLayer<TestSheet> {
    let fixture = Fixture::new(&[(110, text)], &[]);
    let mut layer = layer();
    submit(&mut layer, &fixture, &[110]);
    run_same(&mut layer, frames, NONE);
    layer
}

// `HIGHLIGHT 1` selects palette index 1, the window's body colour, as the
// background (`src/text.c:985-990`); the window bitmap then holds index 1
// wherever a clear writes, not the `B_WIN_MSG` fill index 15.

#[test]
fn a_clear_under_highlight_one_paints_palette_index_one_not_the_window_fill() {
    let layer = printed("{HIGHLIGHT 1}{CLEAR 8}", 3);

    let framebuffer = drawn(&layer);
    // `ClearTextSpan` fills from the cursor at window (0, 1).
    assert_eq!(pixel(&framebuffer, LEFT, TOP + 1), rgb(color(31, 0, 0)));
    assert_eq!(pixel(&framebuffer, LEFT + 7, TOP + 1), rgb(color(31, 0, 0)));
    assert_eq!(pixel(&framebuffer, LEFT + 8, TOP + 1), rgb(color(0, 0, 31)));
    assert_eq!(pixel(&framebuffer, LEFT, TOP), rgb(color(0, 0, 31)));
}

#[test]
fn a_fill_window_under_highlight_one_paints_the_whole_window_with_index_one() {
    let layer = printed("{HIGHLIGHT 1}{FILL_WINDOW}", 3);

    let framebuffer = drawn(&layer);
    let body = rgb(color(31, 0, 0));
    assert_eq!(pixel(&framebuffer, LEFT, TOP), body);
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 1),
        body
    );
    assert_eq!(pixel(&framebuffer, LEFT - 1, TOP), rgb(color(1, 2, 3)));
}

#[test]
fn a_scroll_under_highlight_one_fills_the_exposed_rows_with_index_one() {
    let fixture = Fixture::new(&[(110, "{HIGHLIGHT 1}A\\lB")], &[]);
    let mut layer = layer();
    submit(&mut layer, &fixture, &[110]);
    run(&mut layer, &[NONE, NONE, A_PRESS]);
    assert_eq!(
        pixel(&drawn(&layer), LEFT + WIDTH - 1, TOP + HEIGHT - 1),
        rgb(color(0, 0, 31))
    );

    // The first scroll frame exposes the bottom 4 rows (`ScrollWindow`'s
    // `PIXEL_FILL(bgColor)`, `src/text.c:1196-1201`).
    run_same(&mut layer, 1, NONE);

    let framebuffer = drawn(&layer);
    let body = rgb(color(31, 0, 0));
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 1),
        body
    );
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 4),
        body
    );
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 5),
        rgb(color(0, 0, 31))
    );
}

// Index 0 is transparent on BG0, and the window's tiles replace the textbox
// tilemap inside its rectangle (`PutWindowTilemap`), so a transparent fill
// shows what lies beneath BG0, not the textbox or the index-15 fill.

#[test]
fn a_fill_window_under_highlight_zero_shows_the_scene_beneath_the_window() {
    let layer = printed("{HIGHLIGHT 0}{FILL_WINDOW}", 3);

    let framebuffer = drawn_over_scene(&layer);
    assert_eq!(pixel(&framebuffer, LEFT, TOP), SCENE);
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 1),
        SCENE
    );
    assert_eq!(pixel(&framebuffer, LEFT - 1, TOP), rgb(color(1, 2, 3)));
    assert_eq!(pixel(&framebuffer, LEFT, TOP + HEIGHT), rgb(color(1, 2, 3)));
}

#[test]
fn a_scroll_under_highlight_zero_shows_the_scene_in_the_exposed_rows() {
    let fixture = Fixture::new(&[(110, "{HIGHLIGHT 0}A\\lB")], &[]);
    let mut layer = layer();
    submit(&mut layer, &fixture, &[110]);
    run(&mut layer, &[NONE, NONE, A_PRESS]);
    run_same(&mut layer, 1, NONE);

    let framebuffer = drawn_over_scene(&layer);
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 1),
        SCENE
    );
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 4),
        SCENE
    );
    assert_eq!(
        pixel(&framebuffer, LEFT + WIDTH - 1, TOP + HEIGHT - 5),
        rgb(color(0, 0, 31))
    );
}

#[test]
fn a_clear_under_highlight_zero_leaves_the_window_fill() {
    // `ClearTextSpan` skips painting when the background is transparent
    // (`src/text.c:649-674`).
    let layer = printed("{HIGHLIGHT 0}{CLEAR 8}", 3);

    assert_eq!(
        pixel(&drawn_over_scene(&layer), LEFT, TOP + 1),
        rgb(color(0, 0, 31))
    );
}
