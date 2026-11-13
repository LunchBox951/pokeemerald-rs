//! The message layer's frame contract: text speed, A/B speed-up, pauses,
//! prompts, and the order queued messages print in.
//!
//! Expected frames follow `AddTextPrinter` and `RenderText`
//! (`src/text.c:271`-`:313`, `:936`-`:975`, `:1119`-`:1120`): a printer given
//! delay `d` waits `d - 1` frames after each token, so "AB" ends on the frame
//! after the last glyph's delay, not on the last glyph.

use engine::text::render::TextSpeed;

use super::presentation_support::{
    completion_frames, layer, message, run, run_same, submit, text_fixture, trace, A_HELD, A_PRESS,
    B_PRESS, NONE,
};
use crate::battle_narration::{GateStatus, LayerStatus};

#[test]
fn a_message_ends_a_full_reveal_delay_after_its_last_glyph() {
    for (speed, expected) in [
        (TextSpeed::Slow, "G.......G.......F"),
        (TextSpeed::Mid, "G...G...F"),
        (TextSpeed::Fast, "GGF"),
    ] {
        let fixture = text_fixture();
        let mut layer = layer(speed);
        let gate = submit(&mut layer, &fixture, &[message::AB]);

        let ticks = run_same(&mut layer, expected.len(), NONE);

        assert_eq!(trace(&ticks), expected, "{speed:?}");
        assert_eq!(completion_frames(&ticks), [(expected.len(), gate)]);
    }
}

#[test]
fn the_gate_stays_pending_until_the_frame_the_text_finishes() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Mid);
    let gate = submit(&mut layer, &fixture, &[message::AB]);
    assert_eq!(layer.gate_status(&gate), GateStatus::Pending);

    for _ in 0..8 {
        layer.tick(NONE);
        assert_eq!(layer.gate_status(&gate), GateStatus::Pending);
    }
    layer.tick(NONE);

    assert_eq!(layer.gate_status(&gate), GateStatus::Complete);
}

#[test]
fn a_press_during_a_reveal_delay_speeds_the_print_up_and_a_held_button_keeps_it_up() {
    for press in [A_PRESS, B_PRESS] {
        for speed in [TextSpeed::Slow, TextSpeed::Mid] {
            let fixture = text_fixture();
            let mut layer = layer(speed);
            submit(&mut layer, &fixture, &[message::AB]);

            let held = engine::text::render::PrinterInput {
                a_held: press.a_held,
                b_held: press.b_held,
                ..NONE
            };
            let ticks = run(&mut layer, &[NONE, press, held, held]);

            assert_eq!(trace(&ticks), "G.GF", "{speed:?}");
        }
    }
}

#[test]
fn a_button_pressed_only_before_the_first_delay_does_not_speed_the_print_up() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Slow);
    submit(&mut layer, &fixture, &[message::AB]);

    let mut ticks = run(&mut layer, &[A_PRESS]);
    ticks.extend(run_same(&mut layer, 16, A_HELD));

    assert_eq!(trace(&ticks), "G.......G.......F");
}

#[test]
fn speed_up_does_not_carry_into_the_next_message() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Slow);
    submit(&mut layer, &fixture, &[message::AB, message::AB_AGAIN]);
    let first = run(&mut layer, &[NONE, A_PRESS, A_HELD, A_HELD]);
    assert_eq!(trace(&first), "G.GF");

    let second = run_same(&mut layer, 17, NONE);

    assert_eq!(trace(&second), "G.......G.......F");
}

#[test]
fn a_timed_pause_holds_the_gate_through_held_buttons() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let gate = submit(&mut layer, &fixture, &[message::PAUSED]);

    let ticks = run_same(&mut layer, 7, A_HELD);

    assert_eq!(trace(&ticks), "GpppPGF");
    assert_eq!(completion_frames(&ticks), [(7, gate)]);
}

#[test]
fn a_press_wait_needs_a_fresh_press_after_the_wait_begins() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let gate = submit(&mut layer, &fixture, &[message::WAIT_FOR_PRESS]);

    // The press on frame 2 arrives as the wait begins and held-only frames
    // never count.
    let waiting = run(&mut layer, &[NONE, A_PRESS, A_HELD, NONE, A_HELD, A_HELD]);
    assert_eq!(trace(&waiting), "G?????");
    assert_eq!(layer.gate_status(&gate), GateStatus::Pending);

    let released = run(&mut layer, &[B_PRESS, NONE, NONE]);

    assert_eq!(trace(&released), "!GF");
    assert_eq!(completion_frames(&released), [(3, gate)]);
}

#[test]
fn a_page_prompt_waits_for_a_press_then_clears_the_window() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let gate = submit(&mut layer, &fixture, &[message::PAGE_CLEAR]);

    let ticks = run(&mut layer, &[NONE, NONE, A_HELD, NONE, A_PRESS, NONE, NONE]);

    assert_eq!(trace(&ticks), "Gccc".to_owned() + "CGF");
    assert_eq!(completion_frames(&ticks), [(7, gate)]);
}

#[test]
fn a_scroll_prompt_waits_for_a_press_then_scrolls_one_line_before_resuming() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let gate = submit(&mut layer, &fixture, &[message::SCROLL]);

    let mut ticks = run(&mut layer, &[NONE, NONE, A_HELD, NONE, A_PRESS]);
    // A 16-pixel line scrolls 4 pixels a frame at Fast, then resumes.
    ticks.extend(run_same(&mut layer, 7, NONE));

    assert_eq!(trace(&ticks), "Gsss>vvvv^GF");
    assert_eq!(completion_frames(&ticks), [(12, gate)]);
}

#[test]
fn queued_messages_print_one_after_the_other_never_as_one_stream() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let gate = submit(&mut layer, &fixture, &[message::AB, message::AB_AGAIN]);

    let ticks = run_same(&mut layer, 7, NONE);

    // The first message's end consumes its frame; the second starts on the
    // next one.
    assert_eq!(trace(&ticks), "GGFGGF-");
    assert_eq!(completion_frames(&ticks), [(6, gate)]);
    assert_eq!(ticks[2].completed, None);
    assert_eq!(ticks[6].status, LayerStatus::Idle);
}

#[test]
fn batches_complete_in_submission_order_each_on_its_own_last_frame() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let first = submit(&mut layer, &fixture, &[message::AB]);
    let second = submit(&mut layer, &fixture, &[message::AB_AGAIN]);

    let ticks = run_same(&mut layer, 6, NONE);

    assert_eq!(
        completion_frames(&ticks),
        [(3, first.clone()), (6, second.clone())]
    );
    assert_ne!(first, second);
}

#[test]
fn the_next_message_starts_with_its_own_frames_button_edges() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    submit(
        &mut layer,
        &fixture,
        &[message::WAIT_FOR_PRESS, message::WAIT_FOR_PRESS],
    );
    run(&mut layer, &[NONE, NONE, NONE]);
    // The press that releases the first wait is frame 4's; the first message
    // ends on frame 6, and the press is not reused for the second prompt.
    let ticks = run(
        &mut layer,
        &[B_PRESS, NONE, NONE, NONE, NONE, NONE, B_PRESS, NONE],
    );

    assert_eq!(trace(&ticks), "!GFG??!G");
}
