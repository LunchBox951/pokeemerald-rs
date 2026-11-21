//! The dark page-prompt arrow in the battle message window.
//!
//! `B_WIN_MSG` sets `useAlternateDownArrow` (`src/battle_message.c:2997`-`:3000`),
//! so `TextPrinterDrawDownArrow` blits the dark tiles at the printer cursor
//! while a `\p` or `\l` waits (`src/text.c:787`-`:882`). The tick that reaches
//! the prompt only arms the wait; the arrow first draws one frame later, then
//! redraws every ninth frame with `downArrowDelay = 8`, bobbing through the
//! sheet's frames.

use engine::text::render::TextSpeed;
use rendering::{Bgr555, BitDepth, Framebuffer, Palette, Rgb888, ScreenEntry, Tilemap, Tileset};

use super::fixtures::Fixture;
use super::presentation_support::{
    layer_with_arrow, message, run, run_same, submit, text_fixture, TestSheet, A_PRESS, NONE,
};
use crate::battle_narration::message_box::BattleTextbox;
use crate::battle_narration::BattleMessageLayer;
use crate::textbox::{self, WindowOp};

const FRAME_INDEX: u8 = 3;
const BACKGROUND_INDEX: u8 = 15;
/// The palette slot the test arrow's one marker pixel per frame uses.
const MARKER_INDEX: u8 = 5;
const LEFT: usize = 16;
const TOP: usize = 120;
/// The prompt's cursor after one glyph: x 8, y 1 (`PRINTER_ORIGIN`).
const ARROW_X: usize = LEFT + 8;
const ARROW_Y: usize = TOP + 1;
const ARROW_HEIGHT: usize = 16;
const ARROW_WIDTH: usize = 8;
/// A waiting prompt redraws on every ninth frame (`downArrowDelay = 8`).
const REDRAW_INTERVAL: usize = 9;
const ARROW_FRAMES: usize = 4;

fn color(r: u8, g: u8, b: u8) -> Bgr555 {
    Bgr555::from_channels(r, g, b)
}

/// A battle message layer whose dark arrow is [`textbox::test_down_arrow`]:
/// frame `n` marks only row `n` of column 0.
fn layer() -> BattleMessageLayer<TestSheet> {
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[usize::from(FRAME_INDEX)] = color(1, 2, 3);
    colors[usize::from(MARKER_INDEX)] = color(31, 31, 0);
    colors[usize::from(BACKGROUND_INDEX)] = color(0, 0, 31);
    let frame_tile = vec![FRAME_INDEX | (FRAME_INDEX << 4); BitDepth::Bpp4.tile_byte_len()];
    let textbox = BattleTextbox::from_parts(
        Tileset::decode(BitDepth::Bpp4, &frame_tile).unwrap(),
        Palette::new(colors),
        Tilemap::new(32, 32, vec![ScreenEntry::new(0, false, false, 0); 32 * 32]).unwrap(),
    );
    layer_with_arrow(
        textbox,
        textbox::test_down_arrow(MARKER_INDEX),
        TextSpeed::Fast,
    )
}

fn arrow_ops(layer: &BattleMessageLayer<TestSheet>) -> usize {
    layer
        .window_ops()
        .iter()
        .filter(|op| matches!(op, WindowOp::DownArrow(_)))
        .count()
}

/// The arrow rectangle's marker rows. Every other pixel of the rectangle must
/// be the window background, so an undrawn arrow and a leftover glyph pixel
/// both fail.
fn marker_rows(layer: &BattleMessageLayer<TestSheet>) -> Vec<usize> {
    let mut framebuffer = Framebuffer::new();
    layer.draw(&mut framebuffer);
    let marker = color(31, 31, 0).to_rgb888();
    let background = color(0, 0, 31).to_rgb888();
    let mut rows = Vec::new();
    for dy in 0..ARROW_HEIGHT {
        for dx in 0..ARROW_WIDTH {
            let pixel: Rgb888 = framebuffer.pixel(ARROW_X + dx, ARROW_Y + dy).unwrap();
            if pixel == marker && dx == 0 {
                rows.push(dy);
            } else {
                assert_eq!(
                    pixel, background,
                    "offset ({dx}, {dy}) of the arrow rectangle"
                );
            }
        }
    }
    rows
}

/// Prints `A\pB` to its page prompt, which the first tick's glyph reaches.
fn at_page_prompt() -> BattleMessageLayer<TestSheet> {
    let mut layer = layer();
    submit(&mut layer, &text_fixture(), &[message::PAGE_CLEAR]);
    run(&mut layer, &[NONE]);
    layer
}

#[test]
fn a_page_prompt_draws_the_dark_arrow_a_frame_after_it_starts_waiting() {
    let mut layer = at_page_prompt();
    // The tick that reaches the `\p` only arms the wait (`text.c:1102`-`:1109`).
    run(&mut layer, &[NONE]);
    assert_eq!(arrow_ops(&layer), 0);
    assert_eq!(marker_rows(&layer), []);

    run(&mut layer, &[NONE]);

    assert_eq!(arrow_ops(&layer), 1);
    assert_eq!(marker_rows(&layer), [0]);
}

#[test]
fn the_arrow_bobs_every_ninth_waiting_frame_and_keeps_one_op() {
    let mut layer = at_page_prompt();
    run_same(&mut layer, 2, NONE);

    let mut seen = Vec::new();
    for waiting_frame in 0..REDRAW_INTERVAL * (ARROW_FRAMES + 1) {
        let rows = marker_rows(&layer);
        assert_eq!(rows.len(), 1, "waiting frame {waiting_frame}");
        if waiting_frame % REDRAW_INTERVAL == 0 {
            seen.push(rows[0]);
        } else {
            assert_eq!(Some(&rows[0]), seen.last(), "holds at {waiting_frame}");
        }
        assert_eq!(arrow_ops(&layer), 1, "waiting frame {waiting_frame}");
        run_same(&mut layer, 1, NONE);
    }

    assert_eq!(seen, [0, 1, 2, 3, 0]);
}

#[test]
fn confirming_a_page_prompt_erases_the_arrow() {
    let mut layer = at_page_prompt();
    run_same(&mut layer, 3, NONE);
    assert_eq!(marker_rows(&layer), [0]);

    run(&mut layer, &[A_PRESS]);

    assert_eq!(arrow_ops(&layer), 0);
    assert_eq!(marker_rows(&layer), []);
}

#[test]
fn confirming_a_scroll_prompt_erases_the_arrow_and_the_glyph_under_it() {
    let mut layer = layer();
    let fixture = Fixture::new(&[(110, "A\\lB")], &[]);
    submit(&mut layer, &fixture, &[110]);
    run_same(&mut layer, 4, NONE);
    assert_eq!(marker_rows(&layer), [0]);

    // The confirm tick starts the scroll; it erases the rectangle at the
    // cursor the wait drew at (`TextPrinterClearDownArrow`, `text.c:838`-`:849`).
    run(&mut layer, &[A_PRESS]);

    assert_eq!(arrow_ops(&layer), 0);
    assert_eq!(marker_rows(&layer), []);
}

#[test]
fn a_new_message_starts_without_the_previous_prompts_arrow() {
    let mut layer = at_page_prompt();
    run_same(&mut layer, 3, NONE);
    assert_eq!(arrow_ops(&layer), 1);
    // Answer the prompt, finish the message, and print another.
    run(&mut layer, &[A_PRESS]);
    run_same(&mut layer, 4, NONE);
    submit(&mut layer, &text_fixture(), &[message::AB]);
    run_same(&mut layer, 6, NONE);

    assert_eq!(arrow_ops(&layer), 0);
}

#[test]
fn a_message_without_a_prompt_draws_no_arrow() {
    for id in [message::AB, message::PAUSED, message::WAIT_FOR_PRESS] {
        let mut layer = layer();
        submit(&mut layer, &text_fixture(), &[id]);
        for frame in 0..40 {
            run_same(&mut layer, 1, NONE);
            assert_eq!(arrow_ops(&layer), 0, "template {id}, frame {frame}");
        }
    }
}
