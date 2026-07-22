//! Local smoke tool: build a tiny hand-authored song and play it through the
//! real `platform::AudioOutput` device.
//!
//! Not run in CI (examples are compiled, not executed, by `cargo test`); this
//! is the manual "does sound actually come out?" check. On a headless machine
//! with no audio device it prints a note and exits cleanly.
//!
//! Run with: `cargo run -p audio --example play_song`.

use std::sync::Arc;
use std::time::Duration;

use audio::{decode_track, Adsr, Instrument, Sequencer, Song, ToneData, WaveData, MIXER_RATE};
use platform::AudioOutput;

fn main() {
    let song = build_song();
    let mut seq = Sequencer::new(song);

    let mut output = match AudioOutput::open(4096) {
        Ok(output) => output,
        Err(err) => {
            println!("no audio device ({err}); nothing to play — this is expected in CI/headless");
            return;
        }
    };
    output.start().expect("start playback");
    println!("playing a short scale at {MIXER_RATE} Hz — Ctrl-C to stop");

    let producer = output.producer();
    let frame_samples = u32::try_from(audio::SAMPLES_PER_FRAME).expect("frame fits u32");
    let frame_period = Duration::from_secs_f64(f64::from(frame_samples) / f64::from(MIXER_RATE));
    let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES];

    // Render frame by frame, pacing to real time, until the song finishes.
    while !seq.is_finished() {
        seq.render_frame(&mut buffer);
        // Spin briefly if the ring buffer is momentarily full.
        let mut written = 0;
        while written < buffer.len() {
            written += producer.push(&buffer[written..]);
            if written < buffer.len() {
                std::thread::sleep(frame_period / 4);
            }
        }
        std::thread::sleep(frame_period);
    }
    // Let the tail drain.
    std::thread::sleep(Duration::from_millis(200));
}

/// A short ascending scale played on a looping square-wave instrument.
fn build_song() -> Song {
    // A 64-sample square wave; `freq` chosen so key 60 renders near unity.
    let mut data = vec![90_i8; 64];
    for sample in data.iter_mut().skip(32) {
        *sample = -90;
    }
    let wave = Arc::new(WaveData::looping(13_697_024, 0, data));
    let instrument = ToneData::new(
        wave,
        Adsr {
            attack: 0xFF,
            decay: 0xF0,
            sustain: 0xA0,
            release: 0xE0,
        },
    );

    // VOICE 0; VOL 110; then C-D-E-F-G-A-B-C quarter notes (key 60..72) each
    // followed by a quarter-note wait; FINE.
    let mut bytes = vec![0xBD, 0x00, 0xBE, 110];
    for key in [60_u8, 62, 64, 65, 67, 69, 71, 72] {
        bytes.push(0xE7); // N24 (quarter note)
        bytes.push(key);
        bytes.push(0x7F); // velocity
        bytes.push(0x98); // W24
    }
    bytes.push(0xB1); // FINE

    let events = decode_track(&bytes).expect("valid demo track");
    Song::new(vec![Instrument::DirectSound(instrument)], vec![events], 120)
}
