use super::fixture::*;
use super::*;
use crate::pitch::SAMPLES_PER_FRAME;
use crate::psg::{FrameClocks, FrameSequencer};

/// Drives one voice through the mixer's per-frame sequence on an explicitly
/// seeded frame sequencer.
struct Rig {
    sequencer: FrameSequencer,
    clocks: FrameClocks,
}

impl Rig {
    fn at_step(step: u8) -> Self {
        Self {
            sequencer: FrameSequencer::at_step(step),
            clocks: FrameClocks::with_capacity(4, 8),
        }
    }

    fn frame(&mut self, voice: &mut CgbVoice) -> Vec<StereoAcc> {
        self.sequencer
            .advance_clocks_into(SAMPLES_PER_FRAME, &mut self.clocks);
        voice.set_sequencer_step(self.clocks.start_step);
        voice.begin_frame(false);
        let mut acc = vec![(0, 0); SAMPLES_PER_FRAME];
        voice.render_clocked(&mut acc, &self.clocks.sweep, &self.clocks.length);
        acc
    }
}

fn run(rig: &mut Rig, voice: &mut CgbVoice, frames: usize) -> Vec<StereoAcc> {
    (0..frames).flat_map(|_| rig.frame(voice)).collect()
}

/// A hardware-written oracle. The reference (the same note with its length
/// disabled) supplies the waveform and its envelope gain at each sample; a
/// stopped channel holds the last sample it emitted while its duty position
/// keeps its place in time for the next register write to catch up
/// (`mgba/src/gb/audio.c:162-171,493-510,781-789`). `playing` lists the
/// half-open sample ranges between the hand-scheduled stop and trigger writes.
fn written_oracle(
    reference: &[StereoAcc],
    playing: &[(usize, usize)],
    total: usize,
) -> Vec<StereoAcc> {
    let mut held = (0, 0);
    (0..total)
        .map(|sample| {
            if playing
                .iter()
                .any(|&(from, to)| (from..to).contains(&sample))
            {
                held = reference[sample];
            }
            held
        })
        .collect()
}

fn square(channel: CgbChannelNumber, adsr: CgbAdsr, length: u8) -> CgbVoice {
    let sweep = (channel == CgbChannelNumber::Square1).then_some(0x08);
    square_voice_with_adsr(channel, sweep, adsr, TestNote::default()).with_length(length)
}

fn noise(width_selector: u8, length: u8) -> CgbVoice {
    noise_voice(CgbAdsr::flat(), width_selector, TestNote::default()).with_length(length)
}

const SQUARES: [CgbChannelNumber; 2] = [CgbChannelNumber::Square1, CgbChannelNumber::Square2];
/// Length 48 loads 16; the note's first pitch write enables the counter on an
/// even step (15); the 15th 256 Hz clock from the seeded step-6 epoch lands
/// on global sample 783 (clocks at 52, 104, 156, 209, ... on steps 0, 2, 4, 6).
const CUT_LENGTH: u8 = 48;
const CUT_SAMPLE: usize = 783;
const CUT_FRAMES: usize = 8;
/// Length 60 loads 4, 3 after the enable edge, and the 3rd clock is sample 156.
const TIED_LENGTH: u8 = 60;
const FIRST_EXPIRY: usize = 156;
const SECOND_FRAME_START: usize = SAMPLES_PER_FRAME;
/// The reloaded 63 counts out on the 63rd clock from sample 224.
const RELOADED_EXPIRY: usize = 3501;
const TIED_FRAMES: usize = 17;

fn tied_adsr() -> CgbAdsr {
    CgbAdsr {
        attack: 0,
        decay: 1,
        sustain: 15,
        release: 0,
    }
}

/// `CgbSound` stores `(duty << 6) + length` into `NRx1` (`m4a.c:1001`), and
/// that one write sets both the duty and the length (`mgba/src/gb/audio.c:885-888`).
/// An extracted nonzero length carries its `0x80` bit (`0x80 | 0x30 = 0xB0`),
/// so the helper's duty 2 plus 0xB0 stores 0x30 in the 8-bit register: duty 0,
/// 16 counts. Same register as a duty-0 voice with length 0x30.
#[test]
fn an_encoded_square_length_rewrites_the_duty_bits_of_its_nrx1_store() {
    for channel in SQUARES {
        let mut encoded = square(channel, CgbAdsr::flat(), 0xB0);
        let note = TestNote::default();
        let sweep = (channel == CgbChannelNumber::Square1).then_some(0x08);
        let mut register_equivalent = CgbVoice::square(
            channel,
            0,
            sweep,
            CgbAdsr::flat(),
            note.note_key,
            note.fine_pitch,
            note.track_right,
            note.track_left,
            note.velocity,
            note.gate_time,
            note.played_key,
            note.track,
            note.rhythm_pan,
            note.echo_volume,
            note.echo_length,
        )
        .with_length(0x30);
        let a = run(&mut Rig::at_step(6), &mut encoded, 2);
        let b = run(&mut Rig::at_step(6), &mut register_equivalent, 2);
        assert_eq!(
            a, b,
            "{channel:?}: NRx1 = 0x80 + 0xB0 = 0x30 must play duty 0, as the duty-0/length-0x30 voice does"
        );
    }
}

#[test]
fn a_square_note_cut_by_its_length_holds_its_last_output() {
    for channel in SQUARES {
        let reference = run(
            &mut Rig::at_step(6),
            &mut square(channel, CgbAdsr::flat(), 0),
            CUT_FRAMES,
        );
        let mut voice = square(channel, CgbAdsr::flat(), CUT_LENGTH);
        let rendered = run(&mut Rig::at_step(6), &mut voice, CUT_FRAMES);

        assert_eq!(
            rendered,
            written_oracle(
                &reference,
                &[(0, CUT_SAMPLE)],
                CUT_FRAMES * SAMPLES_PER_FRAME
            ),
            "{channel:?}"
        );
        assert!(
            reference[CUT_SAMPLE..]
                .windows(2)
                .any(|pair| pair[0] != pair[1]),
            "sanity: the uncut note keeps oscillating after the cut"
        );
        assert!(voice.is_length_stopped());
        assert!(voice.is_active(), "the counter does not retire the note");
        assert!(voice.is_end_tie_eligible());
    }
}

#[test]
fn a_noise_note_cut_by_its_length_holds_its_latch() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let reference = run(&mut Rig::at_step(6), &mut noise(width, 0), CUT_FRAMES);
        let mut voice = noise(width, CUT_LENGTH);
        let rendered = run(&mut Rig::at_step(6), &mut voice, CUT_FRAMES);

        assert_eq!(
            rendered,
            written_oracle(
                &reference,
                &[(0, CUT_SAMPLE)],
                CUT_FRAMES * SAMPLES_PER_FRAME
            ),
            "width selector {width}"
        );
        assert!(voice.is_length_stopped());
        assert!(voice.is_active());
    }
}

#[test]
fn a_note_without_a_length_is_never_cut() {
    // Sixty-four counts would have expired long before 40 frames (209 clocks).
    for channel in SQUARES {
        let mut voice = square(channel, CgbAdsr::flat(), 0);
        run(&mut Rig::at_step(6), &mut voice, 40);
        assert!(!voice.is_length_stopped(), "{channel:?}");
    }
    let mut voice = noise(WIDE_NOISE, 0);
    run(&mut Rig::at_step(6), &mut voice, 40);
    assert!(!voice.is_length_stopped());
}

#[test]
fn a_tied_square_outlives_its_first_count_until_a_volume_write_reloads_it() {
    for channel in SQUARES {
        let reference = run(
            &mut Rig::at_step(6),
            &mut square(channel, tied_adsr(), 0),
            TIED_FRAMES,
        );
        let mut voice = square(channel, tied_adsr(), TIED_LENGTH);
        let rendered = run(&mut Rig::at_step(6), &mut voice, TIED_FRAMES);

        // The decay-to-sustain store at frame 1's pass triggers the zero
        // counter back to 63, which counts out again at sample 3501; the
        // sustain refresh writes nothing, so nothing revives it after.
        let expected = written_oracle(
            &reference,
            &[(0, FIRST_EXPIRY), (SECOND_FRAME_START, RELOADED_EXPIRY)],
            TIED_FRAMES * SAMPLES_PER_FRAME,
        );
        let mismatch = rendered.iter().zip(&expected).position(|(a, b)| a != b);
        assert_eq!(mismatch, None, "{channel:?}");
        assert!(voice.is_length_stopped());
        assert!(voice.is_end_tie_eligible(), "the tie stays held throughout");
    }
}

#[test]
fn a_same_value_live_volume_write_reloads_the_stopped_counter_from_zero() {
    for channel in SQUARES {
        let flat_reference = run(
            &mut Rig::at_step(6),
            &mut square(channel, CgbAdsr::flat(), 0),
            TIED_FRAMES,
        );
        let mut voice = square(channel, CgbAdsr::flat(), TIED_LENGTH);
        let mut rig = Rig::at_step(6);
        let mut rendered = rig.frame(&mut voice);
        assert!(voice.is_length_stopped(), "expired inside frame 0");
        // A pitch-only write stores NRx4 without a trigger.
        voice.set_track_pitch(0, 0);
        rendered.extend(rig.frame(&mut voice));
        assert!(voice.is_length_stopped(), "a pitch write does not revive");
        // The live write at frame 2's start (step 7, odd) reloads the full 64.
        voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
        rendered.extend(run(&mut rig, &mut voice, TIED_FRAMES - 2));

        assert_eq!(
            rendered,
            written_oracle(
                &flat_reference,
                // The pitch write at frame 1's start re-latches the held sample
                // at that moment's duty position (one refresh sample).
                &[
                    (0, FIRST_EXPIRY),
                    (SECOND_FRAME_START, SECOND_FRAME_START + 1),
                    (2 * SAMPLES_PER_FRAME, 3762),
                ],
                TIED_FRAMES * SAMPLES_PER_FRAME,
            ),
            "{channel:?}"
        );
    }
}

#[test]
fn the_wave_channel_ignores_a_length() {
    let mut voice = wave_voice(false, TestNote::default()).with_length(CUT_LENGTH);
    run(&mut Rig::at_step(6), &mut voice, 40);
    assert!(!voice.is_length_stopped());
}

fn first_pass(voice: &mut CgbVoice, step: u8) {
    voice.set_sequencer_step(step);
    voice.begin_frame(false);
}

#[test]
fn a_first_pass_loads_the_length_and_the_enable_edge_clocks_it_on_an_even_step() {
    for (step, remaining) in [(6, 3), (5, 4)] {
        let mut voice = square(CgbChannelNumber::Square2, CgbAdsr::flat(), TIED_LENGTH);
        first_pass(&mut voice, step);
        assert_eq!(voice.length_counter().remaining(), remaining, "step {step}");
        assert!(voice.length_counter().is_enabled());
    }
}

#[test]
fn the_pitch_write_can_stop_a_length_one_note_before_its_trigger_reloads_it() {
    // NRx1 loads 1; the pitch block's NRx4 enable edge clocks it to zero on an
    // even step; the following trigger reloads 63 (`m4a.c:1001,1197-1226`).
    let mut even = square(CgbChannelNumber::Square2, CgbAdsr::flat(), 63);
    first_pass(&mut even, 6);
    assert_eq!(even.length_counter().remaining(), 63);
    assert!(!even.is_length_stopped(), "the trigger plays it again");

    let mut odd = square(CgbChannelNumber::Square2, CgbAdsr::flat(), 63);
    first_pass(&mut odd, 5);
    assert_eq!(odd.length_counter().remaining(), 1);
}

#[test]
fn a_replacement_inherits_the_enable_bit_so_its_load_is_not_clocked_again() {
    let mut predecessor = square(CgbChannelNumber::Square2, CgbAdsr::flat(), TIED_LENGTH);
    let mut rig = Rig::at_step(6);
    run(&mut rig, &mut predecessor, 1);
    assert!(predecessor.length_counter().is_enabled());

    let mut replacement = square(CgbChannelNumber::Square2, CgbAdsr::flat(), TIED_LENGTH);
    replacement.carry_hardware_state_from(&predecessor);
    assert_eq!(
        replacement.length_counter(),
        predecessor.length_counter(),
        "the hardware count survives until the new NRx1 store"
    );
    first_pass(&mut replacement, 6);
    assert_eq!(replacement.length_counter().remaining(), 4);

    let mut fresh = square(CgbChannelNumber::Square2, CgbAdsr::flat(), TIED_LENGTH);
    first_pass(&mut fresh, 6);
    assert_eq!(fresh.length_counter().remaining(), 3);
}

#[test]
fn a_cancelled_replacement_never_stores_its_length() {
    let mut predecessor = square(CgbChannelNumber::Square2, CgbAdsr::flat(), TIED_LENGTH);
    run(&mut Rig::at_step(6), &mut predecessor, 1);
    let mut replacement = square(CgbChannelNumber::Square2, CgbAdsr::flat(), 1);
    replacement.carry_hardware_state_from(&predecessor);
    replacement.note_off();
    first_pass(&mut replacement, 6);
    assert_eq!(replacement.length_counter(), predecessor.length_counter());
}

#[test]
fn the_idle_off_write_disables_the_counter_and_reloads_only_a_zero_count() {
    let mut running = square(CgbChannelNumber::Square2, CgbAdsr::flat(), TIED_LENGTH);
    first_pass(&mut running, 6);
    running.apply_hardware_off_write();
    assert!(!running.length_counter().is_enabled());
    assert_eq!(running.length_counter().remaining(), 3);

    let mut expired = square(CgbChannelNumber::Square2, CgbAdsr::flat(), TIED_LENGTH);
    run(&mut Rig::at_step(6), &mut expired, 1);
    assert!(expired.is_length_stopped());
    expired.apply_hardware_off_write();
    assert_eq!(expired.length_counter().remaining(), 64);
    assert!(!expired.is_length_stopped());
}

#[test]
fn a_tied_noise_note_restarts_at_the_reloading_write_and_holds_between_counts() {
    // A trigger restarts the LFSR, so the revived stream is not a continuation
    // of the pre-stop one; the held latch is what the stopped gaps show.
    let mut voice =
        noise_voice(tied_adsr(), WIDE_NOISE, TestNote::default()).with_length(TIED_LENGTH);
    let rendered = run(&mut Rig::at_step(6), &mut voice, TIED_FRAMES);
    let constant = |range: std::ops::Range<usize>| rendered[range].windows(2).all(|p| p[0] == p[1]);

    assert!(!constant(0..FIRST_EXPIRY), "plays until the first expiry");
    assert!(
        constant(FIRST_EXPIRY..SECOND_FRAME_START),
        "holds between counts"
    );
    assert!(
        !constant(SECOND_FRAME_START..RELOADED_EXPIRY),
        "the reload plays again"
    );
    assert!(
        constant(RELOADED_EXPIRY..TIED_FRAMES * SAMPLES_PER_FRAME),
        "holds after the second expiry"
    );
    assert!(voice.is_active());
}

#[test]
fn a_pitch_write_to_a_stopped_square_refreshes_its_held_sample() {
    // A register write runs the stopped channel's duty index up to date and
    // re-latches `duty[index] * volume` (`mgba/src/gb/audio.c:162-171,493-501,916-918`).
    for channel in SQUARES {
        let reference = run(
            &mut Rig::at_step(6),
            &mut square(channel, CgbAdsr::flat(), 0),
            2,
        );
        assert_ne!(
            reference[FIRST_EXPIRY - 1],
            reference[SECOND_FRAME_START],
            "sanity: the duty position moved while the channel was stopped"
        );
        let mut voice = square(channel, CgbAdsr::flat(), TIED_LENGTH);
        let mut rig = Rig::at_step(6);
        rig.frame(&mut voice);
        assert!(voice.is_length_stopped());
        voice.set_track_pitch(0, 0);
        let second = rig.frame(&mut voice);
        assert_eq!(second[0], reference[SECOND_FRAME_START], "{channel:?}");
    }
}

#[test]
fn replacing_a_length_expired_noise_voice_keeps_its_held_latch() {
    // A stopped channel 4 is not run before the replacement's `NR43` writes
    // (`mgba/src/gb/audio.c:602-644`), and its trigger keeps the latch (`:371-383`).
    for width in [WIDE_NOISE, NARROW_NOISE] {
        for key in [24, 36, 48, 60, 72] {
            for frames in 1..=CUT_FRAMES {
                let note = TestNote::at_key(key);
                let mut predecessor =
                    noise_voice(CgbAdsr::flat(), width, note).with_length(TIED_LENGTH);
                let mut rig = Rig::at_step(6);
                run(&mut rig, &mut predecessor, frames);
                assert!(predecessor.is_length_stopped());
                let held = predecessor.noise_output_latch();

                let mut replacement = noise_voice(CgbAdsr::flat(), width, note);
                replacement.carry_hardware_state_from(&predecessor);
                rig.sequencer
                    .advance_clocks_into(SAMPLES_PER_FRAME, &mut rig.clocks);
                replacement.set_sequencer_step(rig.clocks.start_step);
                replacement.begin_frame(false);
                assert_eq!(
                    replacement.noise_output_latch(),
                    held,
                    "width {width} key {key} after {frames} frames"
                );
            }
        }
    }
}
