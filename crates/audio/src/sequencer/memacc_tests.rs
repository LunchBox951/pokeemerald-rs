//! MEMACC mutations and branches preserve upstream opcode semantics and runtime paths.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::sequence::decode_track;
use crate::song::ToneData;

// --- MEMACC --------------------------------------------------------

const MEMACC_BRANCH_TARGET: usize = 37;

fn apply_memacc(mem_acc: &mut MemAccArea, op: u8, address: u8, operand: u8) {
    Sequencer::exec_memacc(mem_acc, &mut TrackState::new(), op, address, operand, None);
}

fn memacc_branch_taken(mem_acc: &mut MemAccArea, op: u8, address: u8, operand: u8) -> bool {
    let mut track = TrackState::new();
    Sequencer::exec_memacc(
        mem_acc,
        &mut track,
        op,
        address,
        operand,
        Some(MEMACC_BRANCH_TARGET),
    );
    track.cursor == MEMACC_BRANCH_TARGET
}

// `decode_track` hands `ply_memacc`'s operation byte through unchanged
// (`sequence.rs:343`..`:346`), so the two tests below drive raw `0..=17`
// literals: they pin the wire numbering that the `MEMACC_*` constants
// only name.

#[test]
fn raw_memacc_opcodes_select_the_upstream_mutations() {
    let cases = [
        (0_u8, 3_u8, 3_u8, "mem_set cell0 = 3"),
        (1, 3, 13, "mem_add cell0 += 3"),
        (2, 3, 7, "mem_sub cell0 -= 3"),
        (3, 1, 3, "mem_mem_set cell0 = cell1"),
        (4, 1, 13, "mem_mem_add cell0 += cell1"),
        (5, 1, 7, "mem_mem_sub cell0 -= cell1"),
    ];

    for (raw_op, operand, expected_cell_0, operation) in cases {
        let mut mem_acc = MemAccArea::default();
        mem_acc.write(0, 10);
        mem_acc.write(1, 3);

        apply_memacc(&mut mem_acc, raw_op, 0, operand);

        assert_eq!(
            mem_acc.read(0),
            Some(expected_cell_0),
            "raw opcode {raw_op}: {operation}"
        );
    }
}

#[test]
fn raw_memacc_opcodes_select_the_upstream_comparisons() {
    const CELL_0: u8 = 10;
    const OPERANDS: [u8; 3] = [5, 10, 11];

    // Upstream numbers the six orderings `6..=11` against a literal and
    // repeats them at `12..=17` against another cell. Each row lists
    // whether the branch is taken for each of `OPERANDS`.
    let cases = [
        (6_u8, 12_u8, [false, true, false], "=="),
        (7, 13, [true, false, true], "!="),
        (8, 14, [true, false, false], ">"),
        (9, 15, [true, true, false], ">="),
        (10, 16, [false, true, true], "<="),
        (11, 17, [false, false, true], "<"),
    ];

    for (literal_op, cell_op, taken_per_operand, ordering) in cases {
        for (operand, expected) in OPERANDS.into_iter().zip(taken_per_operand) {
            let mut mem_acc = MemAccArea::default();
            mem_acc.write(0, CELL_0);
            mem_acc.write(1, operand);

            assert_eq!(
                memacc_branch_taken(&mut mem_acc, literal_op, 0, operand),
                expected,
                "raw opcode {literal_op}: {CELL_0} {ordering} {operand}"
            );
            assert_eq!(
                memacc_branch_taken(&mut mem_acc, cell_op, 0, 1),
                expected,
                "raw opcode {cell_op}: {CELL_0} {ordering} cell holding {operand}"
            );
        }
    }
}

#[test]
fn memacc_literal_mutations_set_and_wrap_at_u8_bounds() {
    let mut mem_acc = MemAccArea::default();

    apply_memacc(&mut mem_acc, MEMACC_SET, 0, 250);
    assert_eq!(mem_acc.read(0), Some(250));

    apply_memacc(&mut mem_acc, MEMACC_ADD, 0, 10);
    assert_eq!(mem_acc.read(0), Some(4));

    apply_memacc(&mut mem_acc, MEMACC_SUB, 0, 10);
    assert_eq!(mem_acc.read(0), Some(250));
}

#[test]
fn memacc_cell_mutations_copy_and_wrap_at_u8_bounds() {
    let mut mem_acc = MemAccArea::default();
    apply_memacc(&mut mem_acc, MEMACC_SET, 1, 250);

    apply_memacc(&mut mem_acc, MEMACC_COPY, 0, 1);
    assert_eq!(mem_acc.read(0), Some(250));

    apply_memacc(&mut mem_acc, MEMACC_SET, 0, 10);
    apply_memacc(&mut mem_acc, MEMACC_ADD_CELL, 0, 1);
    assert_eq!(mem_acc.read(0), Some(4));

    apply_memacc(&mut mem_acc, MEMACC_SET, 0, 3);
    apply_memacc(&mut mem_acc, MEMACC_SUB_CELL, 0, 1);
    assert_eq!(mem_acc.read(0), Some(9));
}

#[test]
fn memacc_literal_comparisons_cover_every_ordering() {
    let cases = [
        (MEMACC_EQ, 11, false, "10 == 11"),
        (MEMACC_EQ, 10, true, "10 == 10"),
        (MEMACC_EQ, 5, false, "10 == 5"),
        (MEMACC_NE, 11, true, "10 != 11"),
        (MEMACC_NE, 10, false, "10 != 10"),
        (MEMACC_NE, 5, true, "10 != 5"),
        (MEMACC_GT, 11, false, "10 > 11"),
        (MEMACC_GT, 10, false, "10 > 10"),
        (MEMACC_GT, 5, true, "10 > 5"),
        (MEMACC_GE, 11, false, "10 >= 11"),
        (MEMACC_GE, 10, true, "10 >= 10"),
        (MEMACC_GE, 5, true, "10 >= 5"),
        (MEMACC_LE, 11, true, "10 <= 11"),
        (MEMACC_LE, 10, true, "10 <= 10"),
        (MEMACC_LE, 5, false, "10 <= 5"),
        (MEMACC_LT, 11, true, "10 < 11"),
        (MEMACC_LT, 10, false, "10 < 10"),
        (MEMACC_LT, 5, false, "10 < 5"),
    ];
    let mut mem_acc = MemAccArea::default();
    apply_memacc(&mut mem_acc, MEMACC_SET, 0, 10);

    for (op, operand, expected, expression) in cases {
        assert_eq!(
            memacc_branch_taken(&mut mem_acc, op, 0, operand),
            expected,
            "{expression}"
        );
    }
}

#[test]
fn memacc_cell_comparisons_cover_every_ordering() {
    let cases = [
        (MEMACC_CELL_EQ, 11, false, "10 == 11"),
        (MEMACC_CELL_EQ, 10, true, "10 == 10"),
        (MEMACC_CELL_EQ, 5, false, "10 == 5"),
        (MEMACC_CELL_NE, 11, true, "10 != 11"),
        (MEMACC_CELL_NE, 10, false, "10 != 10"),
        (MEMACC_CELL_NE, 5, true, "10 != 5"),
        (MEMACC_CELL_GT, 11, false, "10 > 11"),
        (MEMACC_CELL_GT, 10, false, "10 > 10"),
        (MEMACC_CELL_GT, 5, true, "10 > 5"),
        (MEMACC_CELL_GE, 11, false, "10 >= 11"),
        (MEMACC_CELL_GE, 10, true, "10 >= 10"),
        (MEMACC_CELL_GE, 5, true, "10 >= 5"),
        (MEMACC_CELL_LE, 11, true, "10 <= 11"),
        (MEMACC_CELL_LE, 10, true, "10 <= 10"),
        (MEMACC_CELL_LE, 5, false, "10 <= 5"),
        (MEMACC_CELL_LT, 11, true, "10 < 11"),
        (MEMACC_CELL_LT, 10, false, "10 < 10"),
        (MEMACC_CELL_LT, 5, false, "10 < 5"),
    ];

    for (op, other, expected, expression) in cases {
        let mut mem_acc = MemAccArea::default();
        apply_memacc(&mut mem_acc, MEMACC_SET, 0, 10);
        apply_memacc(&mut mem_acc, MEMACC_SET, 1, other);
        assert_eq!(
            memacc_branch_taken(&mut mem_acc, op, 0, 1),
            expected,
            "{expression}"
        );
    }
}

#[test]
fn memacc_out_of_range_destination_is_a_no_op() {
    let mut mem_acc = MemAccArea::default();

    for op in [MEMACC_SET, MEMACC_ADD, MEMACC_SUB] {
        apply_memacc(&mut mem_acc, op, 200, 42);
    }

    assert_eq!(mem_acc, MemAccArea::default());
    assert!(!memacc_branch_taken(&mut mem_acc, MEMACC_EQ, 200, 0));
}

#[test]
fn memacc_out_of_range_source_is_a_no_op_or_false_comparison() {
    let mut mem_acc = MemAccArea::default();
    apply_memacc(&mut mem_acc, MEMACC_SET, 0, 42);
    let original = mem_acc.clone();

    for op in [MEMACC_COPY, MEMACC_ADD_CELL, MEMACC_SUB_CELL] {
        apply_memacc(&mut mem_acc, op, 0, 200);
        assert_eq!(mem_acc, original);
    }

    for op in [
        MEMACC_CELL_EQ,
        MEMACC_CELL_NE,
        MEMACC_CELL_GT,
        MEMACC_CELL_GE,
        MEMACC_CELL_LE,
        MEMACC_CELL_LT,
    ] {
        assert!(!memacc_branch_taken(&mut mem_acc, op, 0, 200));
    }
}

#[test]
fn memacc_unknown_operation_is_a_no_op() {
    for op in [18, u8::MAX] {
        let mut mem_acc = MemAccArea::default();
        apply_memacc(&mut mem_acc, MEMACC_SET, 0, 42);
        let original = mem_acc.clone();

        apply_memacc(&mut mem_acc, op, 0, 7);

        assert_eq!(mem_acc, original, "operation {op}");
    }
}

#[test]
fn memacc_taken_branch_without_a_target_falls_through() {
    let mut mem_acc = MemAccArea::default();
    apply_memacc(&mut mem_acc, MEMACC_SET, 0, 42);
    let mut track = TrackState::new();

    Sequencer::exec_memacc(&mut mem_acc, &mut track, MEMACC_EQ, 0, 42, None);

    assert_eq!(track.cursor, 0);
}

#[test]
fn memacc_cells_are_private_to_each_sequencer() {
    let song = || test_song(vec![vec![Event::Fine]], 150);
    let mut first = Sequencer::new(song());
    apply_test_event(
        &mut first,
        0,
        &Event::MemAcc {
            op: MEMACC_SET,
            addr: 0,
            value: 42,
            target: None,
        },
    );
    let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
    first.render_frame(&mut output);
    assert!(first.is_finished());
    assert_eq!(first.mem_acc.read(0), Some(42));

    // Built after `first` already wrote a cell, so a shared area would leak here.
    let second = Sequencer::new(song());

    assert_eq!(second.mem_acc.read(0), Some(0));
}

/// Builds `prelude`, then `[loop:] Voice, Note, Wait, MemAcc -> loop`
/// followed by `Fine`, and renders it: a taken branch loops the track
/// forever, a fall-through reaches `Fine`.
fn memacc_track_loops_forever(prelude: Vec<Event>, op: u8, address: u8, operand: u8) -> bool {
    let loop_target = prelude.len() + 1;
    let mut track = prelude;
    track.push(Event::Voice(0));
    track.push(Event::Note {
        key: 60,
        velocity: 127,
        gate: 1,
    });
    track.push(Event::Wait(24));
    track.push(Event::MemAcc {
        op,
        addr: address,
        value: operand,
        target: Some(loop_target),
    });
    track.push(Event::Fine);

    let mut sequencer = Sequencer::new(test_song(vec![track], 150));
    let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
    for _ in 0..200 {
        sequencer.render_frame(&mut output);
    }
    !sequencer.is_finished()
}

#[test]
fn memacc_mutates_and_branches_through_the_rendered_track() {
    let set_cell_0_to_5 = || {
        vec![Event::MemAcc {
            op: MEMACC_SET,
            addr: 0,
            value: 5,
            target: None,
        }]
    };

    assert!(
        memacc_track_loops_forever(set_cell_0_to_5(), MEMACC_EQ, 0, 5),
        "cell 0 holds 5, so the equal branch loops the rendered track forever"
    );
    assert!(
        !memacc_track_loops_forever(set_cell_0_to_5(), MEMACC_EQ, 0, 6),
        "cell 0 holds 5, so the unequal branch falls through to Fine"
    );
}

/// An unconditional `MEMACC` writing `addr`, for use as a prelude.
fn memacc_set(addr: u8, value: u8) -> Event {
    Event::MemAcc {
        op: MEMACC_SET,
        addr,
        value,
        target: None,
    }
}

#[test]
fn memacc_mutations_take_effect_through_the_rendered_track() {
    let mutate = |op, addr, value| Event::MemAcc {
        op,
        addr,
        value,
        target: None,
    };
    let cases = [
        (
            vec![memacc_set(0, 250), mutate(MEMACC_ADD, 0, 10)],
            4_u8,
            "mem_add wraps past 255",
        ),
        (
            vec![memacc_set(0, 3), mutate(MEMACC_SUB, 0, 10)],
            249,
            "mem_sub wraps below 0",
        ),
        (
            vec![memacc_set(1, 9), mutate(MEMACC_COPY, 0, 1)],
            9,
            "mem_mem_set copies cell 1",
        ),
        (
            vec![
                memacc_set(0, 10),
                memacc_set(1, 250),
                mutate(MEMACC_ADD_CELL, 0, 1),
            ],
            4,
            "mem_mem_add wraps past 255",
        ),
        (
            vec![
                memacc_set(0, 3),
                memacc_set(1, 250),
                mutate(MEMACC_SUB_CELL, 0, 1),
            ],
            9,
            "mem_mem_sub wraps below 0",
        ),
    ];

    for (prelude, expected_cell_0, operation) in cases {
        let neighbour = expected_cell_0.wrapping_add(1);
        assert!(
            memacc_track_loops_forever(prelude.clone(), MEMACC_EQ, 0, expected_cell_0),
            "{operation}: cell 0 should hold {expected_cell_0}"
        );
        assert!(
            !memacc_track_loops_forever(prelude, MEMACC_EQ, 0, neighbour),
            "{operation}: cell 0 should not hold {neighbour}"
        );
    }
}

#[test]
fn memacc_comparisons_branch_through_the_rendered_track() {
    const CELL_0: u8 = 10;
    const OPERANDS: [u8; 3] = [5, 10, 11];

    let cases = [
        (MEMACC_EQ, MEMACC_CELL_EQ, [false, true, false], "=="),
        (MEMACC_NE, MEMACC_CELL_NE, [true, false, true], "!="),
        (MEMACC_GT, MEMACC_CELL_GT, [true, false, false], ">"),
        (MEMACC_GE, MEMACC_CELL_GE, [true, true, false], ">="),
        (MEMACC_LE, MEMACC_CELL_LE, [false, true, true], "<="),
        (MEMACC_LT, MEMACC_CELL_LT, [false, false, true], "<"),
    ];

    for (literal_op, cell_op, taken_per_operand, ordering) in cases {
        for (operand, expected) in OPERANDS.into_iter().zip(taken_per_operand) {
            assert_eq!(
                memacc_track_loops_forever(vec![memacc_set(0, CELL_0)], literal_op, 0, operand),
                expected,
                "{CELL_0} {ordering} {operand}"
            );
            assert_eq!(
                memacc_track_loops_forever(
                    vec![memacc_set(0, CELL_0), memacc_set(1, operand)],
                    cell_op,
                    0,
                    1
                ),
                expected,
                "{CELL_0} {ordering} cell holding {operand}"
            );
        }
    }
}

#[test]
fn memacc_out_of_range_cells_stay_safe_through_the_rendered_track() {
    const PAST_THE_AREA: u8 = 200;

    assert!(
        memacc_track_loops_forever(vec![memacc_set(PAST_THE_AREA, 42)], MEMACC_EQ, 0, 0),
        "a write past the area must corrupt no real cell"
    );
    assert!(
        !memacc_track_loops_forever(Vec::new(), MEMACC_EQ, PAST_THE_AREA, 0),
        "a comparison addressed past the area must fall through to Fine"
    );
    assert!(
        memacc_track_loops_forever(
            vec![Event::MemAcc {
                op: MEMACC_COPY,
                addr: 0,
                value: PAST_THE_AREA,
                target: None,
            }],
            MEMACC_EQ,
            0,
            0
        ),
        "a copy sourced past the area must leave cell 0 zeroed"
    );

    for op in [
        MEMACC_CELL_EQ,
        MEMACC_CELL_NE,
        MEMACC_CELL_GT,
        MEMACC_CELL_GE,
        MEMACC_CELL_LE,
        MEMACC_CELL_LT,
    ] {
        assert!(
            !memacc_track_loops_forever(vec![memacc_set(0, 10)], op, 0, PAST_THE_AREA),
            "operation {op} sourced past the area must fall through to Fine"
        );
    }
}

#[test]
fn memacc_unknown_operation_is_a_no_op_through_the_rendered_track() {
    for op in [18, u8::MAX] {
        let prelude = vec![Event::MemAcc {
            op,
            addr: 0,
            value: 42,
            target: None,
        }];
        assert!(
            memacc_track_loops_forever(prelude, MEMACC_EQ, 0, 0),
            "operation {op} must leave cell 0 zeroed"
        );
    }
}

#[test]
fn memacc_event_dispatches_a_taken_conditional_jump() {
    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));

    apply_test_event(
        &mut sequencer,
        0,
        &Event::MemAcc {
            op: MEMACC_EQ,
            addr: 0,
            value: 0,
            target: Some(MEMACC_BRANCH_TARGET),
        },
    );

    assert_eq!(sequencer.tracks[0].cursor, MEMACC_BRANCH_TARGET);
}

#[test]
fn decoded_memory_branch_preserves_each_runtime_paths_note() {
    let bytes = [
        0xBD, 0, 0xCF, 60, 127, 0xB9, 6, 0, 1, 16, 0, 0, 0, 0xCF, 72, 127, 0xCF, 0xB0, 0xB1,
    ];
    let events = decode_track(&bytes).unwrap();
    for (memory, expected) in [(0, vec![60, 72, 72]), (1, vec![60, 60])] {
        let wave = Arc::new(WaveData::looping(0, 0, vec![100]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let mut sequencer = Sequencer::new(Song::new(voices, vec![events.clone()], 150));
        sequencer.mem_acc.write(0, memory);
        let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
        sequencer.render_frame(&mut output);

        let mut keys: Vec<_> = sequencer
            .mixer
            .voices()
            .iter()
            .map(|voice| voice.midi_key())
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, expected, "memory cell = {memory}");
    }
}
