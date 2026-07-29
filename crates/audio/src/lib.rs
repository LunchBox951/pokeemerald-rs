//! Audio subsystem (S-3): the M4A ("MP2K"/"Sappy") sequence engine.
//!
//! This crate re-implements the *behaviour* of `pokeemerald`'s M4A sound
//! driver `(behavioral-fidelity)` — no GBA emulation, no transliterated C
//! `(no-verbatim)`. Slice 1 covers the DirectSound (PCM) music path; slice 2
//! adds the four CGB PSG channels, LFO/vibrato, and pattern execution:
//!
//! - [`sequence`] — the typed track model and the decoder that turns MP2K's
//!   byte-coded command stream into [`Event`]s once, ahead of playback.
//! - [`song`] — a loaded [`Song`] (voicegroup + decoded tracks); its
//!   [`song::Instrument`] selects a DirectSound sample or a CGB PSG kind.
//! - [`envelope`] — the per-voice DirectSound ADSR state machine.
//! - [`voice`] — one playing DirectSound [`Voice`]: pitch-stepped, interpolated
//!   sample playback shaped by an envelope.
//! - [`cgb_pitch`] — MIDI key → CGB hardware frequency register / noise
//!   control byte.
//! - [`psg`] — the four CGB PSG waveform generators (square/wave/noise).
//! - [`cgb_envelope`] — the CGB hardware's coarse `0..=15` envelope, plus its
//!   `CgbPan`/`CgbModVol` stereo-routing helpers.
//! - [`cgb_voice`] — one playing CGB [`cgb_voice::CgbVoice`], tying an
//!   oscillator, envelope, and panning together.
//! - [`mixer`] — the software [`Mixer`] that sums DirectSound and CGB voices
//!   to interleaved stereo `f32` and clips.
//! - [`sequencer`] — the owned [`Sequencer`] tying it together: the tick engine
//!   (including LFO/vibrato and `PATT`/`PEND`/`REPT` pattern execution) plus
//!   an offline, device-free rendering path ([`Sequencer::mix_into`]).
//! - [`pitch`] — the MIDI-key → frequency table and the fixed-point step math.
//!
//! Slice 3 adds key-split (`TONEDATA_TYPE_SPL`) and rhythm
//! (`TONEDATA_TYPE_RHY`) voice indirection ([`song::KeySplit`],
//! [`song::Rhythm`]), fixed-rate DirectSound (`TONEDATA_TYPE_FIX`,
//! [`song::ToneData::fixed`]), and the `xIECV`/`xIECL` pseudo-echo `XCMD`s
//! for both DirectSound and CGB voices.
//!
//! Everything renders at exactly [`pitch::MIXER_RATE`] (13379 Hz), the rate the
//! `platform` producer expects; a unit test pins the two together.
//!
//! ## Out of scope for this slice
//!
//! Reverb, SFX priority/interruption and voice stealing, the M4A player
//! command interface, compressed/reversed DirectSound waves. `MEMACC` and
//! every other `XCMD` sub-command are still only *decoded*, not executed, so
//! the byte stream stays in sync.

// This crate's docs cite upstream C symbols and hardware names heavily
// (DirectSound, MP2K, SongHeader, …); backticking every prose mention adds
// noise without clarity. The volume/pitch pipeline also has intrinsically
// close names (volMR/volML, keyM/pitM) taken verbatim from the reference.
#![allow(clippy::doc_markdown, clippy::similar_names)]

pub mod cgb_envelope;
pub mod cgb_pitch;
pub mod cgb_voice;
pub mod envelope;
pub mod mixer;
pub mod pitch;
pub mod psg;
pub mod sample;
pub mod sequence;
pub mod sequencer;
pub mod song;
pub mod voice;

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
        let song = Song::new(
            vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))],
            vec![events],
            150,
        );
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
