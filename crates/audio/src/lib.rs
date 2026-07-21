//! Audio subsystem (S-3): the M4A ("MP2K"/"Sappy") sequence engine.
//!
//! This crate re-implements the *behaviour* of `pokeemerald`'s M4A sound
//! driver `(behavioral-fidelity)` — no GBA emulation, no transliterated C
//! `(no-verbatim)`. Slice 1 covers the DirectSound (PCM) music path:
//!
//! - [`sequence`] — the typed track model and the decoder that turns MP2K's
//!   byte-coded command stream into [`Event`]s once, ahead of playback.
//! - [`song`] — a loaded [`Song`] (voicegroup + decoded tracks).
//! - [`envelope`] — the per-voice ADSR state machine.
//! - [`voice`] — one playing DirectSound [`Voice`]: pitch-stepped, interpolated
//!   sample playback shaped by an envelope.
//! - [`mixer`] — the software [`Mixer`] that sums voices to interleaved stereo
//!   `f32` and clips.
//! - [`sequencer`] — the owned [`Sequencer`] tying it together: the tick engine
//!   plus an offline, device-free rendering path ([`Sequencer::mix_into`]).
//! - [`pitch`] — the MIDI-key → frequency table and the fixed-point step math.
//!
//! Everything renders at exactly [`pitch::MIXER_RATE`] (13379 Hz), the rate the
//! `platform` producer expects; a unit test pins the two together.
//!
//! ## Out of scope for this slice
//!
//! CGB PSG channels (square/wave/noise), reverb, SFX priority/interruption and
//! voice stealing, the M4A player command interface, compressed/fixed-rate
//! waves, LFO/vibrato, and patterns (`PATT`/`PEND`/`REPT`). Commands the engine
//! does not yet execute are still *decoded* so the byte stream stays in sync.

// This crate's docs cite upstream C symbols and hardware names heavily
// (DirectSound, MP2K, SongHeader, …); backticking every prose mention adds
// noise without clarity. The volume/pitch pipeline also has intrinsically
// close names (volMR/volML, keyM/pitM) taken verbatim from the reference.
#![allow(clippy::doc_markdown, clippy::similar_names)]

pub mod envelope;
pub mod mixer;
pub mod pitch;
pub mod sample;
pub mod sequence;
pub mod sequencer;
pub mod song;
pub mod voice;

pub use envelope::{Adsr, Envelope, Phase};
pub use mixer::{Mixer, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES};
pub use pitch::{MIXER_RATE, SAMPLES_PER_FRAME};
pub use sample::WaveData;
pub use sequence::{decode_track, DecodeError, Event};
pub use sequencer::Sequencer;
pub use song::{Song, ToneData};
pub use voice::Voice;

#[cfg(test)]
// The round-trip through the ring buffer preserves samples bit-for-bit, so the
// buffer equality check is exact on purpose.
#[allow(clippy::float_cmp)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// The crate's render rate must equal the platform producer's nominal
    /// rate, or every song would play at the wrong pitch and timing.
    #[test]
    fn mixer_rate_matches_platform_producer() {
        assert_eq!(MIXER_RATE, platform::AudioOutput::M4A_MIXER_RATE);
    }

    /// A tiny song built from a decoded byte program, rendered offline and fed
    /// through the real (headless/null) `platform::AudioOutput` producer — the
    /// end-to-end smoke path, with no audio device.
    #[test]
    fn renders_a_decoded_song_through_the_platform_producer() {
        // VOICE 0; note N24 key 60 vel 127; W48; FINE.
        let bytes = [0xBD, 0x00, 0xE7, 60, 127, 0xB0, 0xB1];
        let events = decode_track(&bytes).expect("valid track");

        let wave = Arc::new(WaveData::one_shot(
            13_697_024,
            vec![80; SAMPLES_PER_FRAME * 4],
        ));
        let song = Song::new(vec![ToneData::new(wave, Adsr::flat())], vec![events], 150);
        let mut seq = Sequencer::new(song);

        // Render two frames' worth of audio.
        let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES * 2];
        seq.mix_into(&mut buffer);
        assert!(
            buffer.iter().any(|&s| s.abs() > 0.0),
            "expected audible output"
        );

        // Push it through the platform producer and pull it back out via the
        // null backend — exactly the path a real device callback would drain.
        let mut output = platform::AudioOutput::null(Sequencer::FRAME_SAMPLES * 2);
        let produced = output.producer().push(&buffer);
        assert_eq!(produced, buffer.len());

        let mut drained = vec![0.0_f32; buffer.len()];
        output.pull_null(&mut drained);
        assert_eq!(drained, buffer);
    }
}
