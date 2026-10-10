//! M4A (MP2K/Sappy) sequencing and software mixing.
//!
//! This crate implements pokeemerald's sound-driver behaviour
//! `(behavioral-fidelity)` without GBA emulation or transliterated C
//! `(no-verbatim)`. [`Song`] holds a voicegroup and decoded
//! tracks; [`Sequencer`] executes their events and drives an owned [`Mixer`].
//!
//! - [`sequence`] decodes MP2K track bytecode into typed [`Event`]s.
//! - [`song`] models DirectSound and CGB instruments, including
//!   [`KeySplit`], [`Rhythm`], and fixed-rate [`ToneData`].
//! - [`sample`], [`pitch`], [`envelope`], and [`voice`] implement signed
//!   8-bit PCM playback, pitch stepping, ADSR, and DirectSound [`Voice`]s.
//! - [`cgb_pitch`], [`psg`], [`cgb_envelope`], and [`cgb_voice`] implement
//!   the four CGB channels: two squares, programmable wave, and noise.
//! - [`mixer`] allocates voices by priority and track, mixes DirectSound
//!   and CGB output, and produces clipped interleaved stereo `f32` samples.
//! - [`sequencer`] handles track timing, LFO modulation, pattern calls,
//!   repeats, memory-accumulator mutations and conditional jumps, and the
//!   `xIECV`/`xIECL` pseudo-echo commands.
//!
//! [`Sequencer::mix_into`] renders whole frames offline, deterministically
//! and without an audio device or wall clock. Each frame contains
//! [`SAMPLES_PER_FRAME`] stereo sample frames. [`MIXER_RATE`] is the rounded
//! nominal PCM rate, 13,379 Hz, matching the platform producer's
//! `platform::AudioOutput::M4A_MIXER_RATE` contract.
//!
//! The private `reverb` module supplies feedback reverb for the DirectSound
//! mix; CGB output does not enter its delay ring. [`Song::with_reverb`] sets
//! the song override and [`Song::reverb`] reads its level. Callers can supply
//! a resolved session level through [`Sequencer::with_resolved_reverb`].
//!
//! ## Limitations
//!
//! Cross-song SFX priority/interruption and compressed or reversed
//! DirectSound waves are not implemented. Single-song voice allocation is
//! implemented. `PORT` and `XCMD` commands other than `xIECV`/`xIECL` are
//! decoded but not executed.

// M4A documentation uses hardware names and upstream symbols in prose;
// repetitive backticks obscure the explanation. Related volume and pitch
// variables retain the driver's closely related names.
#![allow(clippy::doc_markdown, clippy::similar_names)]

pub mod cgb_envelope;
pub mod cgb_pitch;
pub mod cgb_voice;
pub mod envelope;
mod gate;
pub mod mixer;
pub mod pitch;
pub mod psg;
pub mod sample;
pub mod sequence;
pub mod sequencer;
pub mod song;
pub mod voice;

/// Internal DirectSound feedback reverb, configured from [`Song::reverb`]
/// through `Mixer::with_reverb_level`. Public callers set the song override
/// with [`Song::with_reverb`] or use [`Sequencer::with_resolved_reverb`].
mod reverb;

pub use cgb_envelope::{CgbAdsr, CgbEnvelope};
pub use cgb_voice::CgbVoice;
pub use envelope::{Adsr, Envelope, Phase};
pub use mixer::{Mixer, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES};
pub use pitch::{MIXER_RATE, SAMPLES_PER_FRAME};
pub use sample::WaveData;
pub use sequence::{decode_track, DecodeError, Event};
pub use sequencer::Sequencer;
pub use song::{
    rhythm_pan_from_pan_sweep, Instrument, KeySplit, Rhythm, RhythmChild, Song, ToneData, KEY_SLOTS,
};
pub use voice::Voice;

#[cfg(test)]
// The direct null backend copies samples without resampling; exact float
// equality verifies that this ring-buffer round-trip leaves them unchanged.
#[allow(clippy::float_cmp)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// Pins the audio crate's nominal PCM rate to the platform producer contract.
    #[test]
    fn mixer_rate_matches_platform_producer() {
        assert_eq!(MIXER_RATE, platform::AudioOutput::M4A_MIXER_RATE);
    }

    /// Decodes and renders a small song offline, then verifies an exact
    /// producer-to-null-consumer round-trip without opening an audio device.
    #[test]
    fn renders_a_decoded_song_through_the_platform_producer() {
        let bytes = [0xBD, 0x00, 0xBE, 127, 0xE7, 60, 127, 0xB0, 0xB1];
        let events = decode_track(&bytes).expect("valid track");
        assert_eq!(
            events,
            [
                Event::Voice(0),
                Event::Volume(127),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 24
                },
                Event::Wait(96),
                Event::Fine,
            ]
        );

        let wave = Arc::new(WaveData::one_shot(
            13_697_024,
            vec![80; SAMPLES_PER_FRAME * 4],
        ));
        let song = Song::new(
            vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))],
            vec![events],
            150,
        );
        let mut seq = Sequencer::new(song);

        // Render two sequencer frames of interleaved stereo samples offline.
        let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES * 2];
        seq.mix_into(&mut buffer);
        assert!(
            buffer.iter().any(|&s| s.abs() > 0.0),
            "expected audible output"
        );

        // Push the rendered samples into the ring, then drain its direct null
        // consumer without a device callback or resampling.
        let mut output = platform::AudioOutput::null(Sequencer::FRAME_SAMPLES * 2);
        let produced = output.producer().push(&buffer);
        assert_eq!(produced, buffer.len());

        let mut drained = vec![0.0_f32; buffer.len()];
        output.pull_null(&mut drained);
        assert_eq!(drained, buffer);
    }
}
