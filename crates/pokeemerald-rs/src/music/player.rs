//! [`MusicPlayer`]: the frame-driven playback owner (S-3, issue #185,
//! Discussion #227's owner decision).
//!
//! Per that decision, playback for this slice is **frame-driven, not a
//! background thread**: the integration App (`crate::App`) advances one M4A
//! audio frame per game frame, synchronously, on whatever thread drives its
//! own frame loop. `audio` owns all sequencer/mixer/voice/reverb state
//! ([`audio::Sequencer`]); `platform` owns only the ring buffer, resampling,
//! and `cpal` output ([`platform::AudioOutput`]); [`MusicPlayer`] is the one
//! type in this crate that ties the two together and is ticked by
//! [`crate::App::step`].
//!
//! [`MusicPlayer::start`] prefills the ring before starting output (the same
//! decision), so the very first device callback already has audio queued
//! rather than immediately underrunning while the frame loop catches up.

use audio::{Sequencer, Song};
use platform::{AudioOutput, PlatformError, Producer};

use super::MusicError;

/// Ring buffer capacity, in stereo frames -- about 306 ms of headroom at the
/// nominal 13379 Hz mixer rate (matches
/// `crates/audio/examples/play_song.rs`'s own `AudioOutput::open` sizing).
pub const RING_CAPACITY_FRAMES: usize = 4096;

/// An owned, playing song: the sequencer that renders it plus the device
/// ring it feeds. See the module docs.
pub struct MusicPlayer {
    /// Kept so a defensive restart ([`Self::advance_frame`]) can rebuild a
    /// fresh [`Sequencer`] without reloading the pack.
    song: Song,
    sequencer: Sequencer,
    producer: Producer,
    /// Kept alive for the whole player's lifetime: dropping it tears the
    /// `cpal` stream (or null backend) down (`platform::AudioOutput`'s own
    /// docs), which is exactly how [`crate::App`] stops this song's BGM --
    /// dropping its `MusicPlayer` when the title flow leaves the screen
    /// that requested it.
    output: AudioOutput,
}

impl MusicPlayer {
    /// Load `song_name` (e.g. `"mus_title"`) out of `pack`, open an audio
    /// device via `open_audio` (real hardware for [`crate::App::new`], the
    /// headless null backend for tests -- see [`crate::App::boot`]'s own
    /// opener-parameter precedent for `platform::Platform`), and start
    /// playing it.
    ///
    /// # Errors
    ///
    /// Propagates [`MusicError`] from resolving the song out of the pack, or
    /// from `open_audio`/[`Self::start`].
    pub fn start_from_pack(
        pack: &assets::AssetPack,
        song_name: &str,
        open_audio: impl FnOnce() -> Result<AudioOutput, PlatformError>,
    ) -> Result<Self, MusicError> {
        let song = super::load_song_from_pack(pack, song_name)?;
        let output = open_audio()?;
        Self::start(song, output).map_err(MusicError::from)
    }

    /// Start playing an already-resolved `song` through `output`: prefills
    /// the ring, then starts the device.
    ///
    /// # Errors
    ///
    /// [`PlatformError`] if `output` refuses to start (never happens for the
    /// null backend).
    pub fn start(song: Song, mut output: AudioOutput) -> Result<Self, PlatformError> {
        let mut sequencer = Sequencer::new(song.clone());
        let producer = output.producer();
        prefill(&mut sequencer, &producer);
        output.start()?;
        Ok(Self {
            song,
            sequencer,
            producer,
            output,
        })
    }

    /// Advance playback by exactly one game frame: render one
    /// [`audio::Sequencer`] frame and push it to the device ring.
    ///
    /// A real BGM loops forever via its own internal jump commands and
    /// should never actually finish (`crate::music`'s module docs on
    /// continuous playback); if [`Sequencer::is_finished`] is ever true
    /// regardless, this restarts a fresh sequencer from the same song
    /// rather than falling permanently silent.
    pub fn advance_frame(&mut self) {
        if self.sequencer.is_finished() {
            self.sequencer = Sequencer::new(self.song.clone());
        }
        let mut buffer = [0.0_f32; Sequencer::FRAME_SAMPLES];
        self.sequencer.render_frame(&mut buffer);
        let _dropped = self.producer.push(&buffer);
    }

    /// Total samples played as silence so far due to ring-buffer underrun --
    /// the V-5 "no underrun in the frame-driven path" evidence this slice's
    /// definition of done asks for.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.output.underruns()
    }

    /// Whether the underlying device output is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.output.is_running()
    }
}

/// Render and push whole frames until the ring reports no more room --
/// [`MusicPlayer::start`]'s "prefill the ring before starting output" step
/// (module docs).
fn prefill(sequencer: &mut Sequencer, producer: &Producer) {
    let mut buffer = [0.0_f32; Sequencer::FRAME_SAMPLES];
    while producer.available_space() >= Sequencer::FRAME_SAMPLES {
        sequencer.render_frame(&mut buffer);
        let _dropped = producer.push(&buffer);
    }
}

#[cfg(test)]
impl MusicPlayer {
    /// Test-only: drive the null backend by hand (mirrors
    /// `platform::AudioOutput::pull_null`), so a synthetic test can prove
    /// sustained push-then-drain cycles never underrun -- exactly what a
    /// real device callback does once per audio buffer, just invoked
    /// directly here instead of from `cpal`. Lives in this module (not
    /// `super::tests`) since `output` is private to it.
    pub(crate) fn drain_null_for_test(&mut self, out: &mut [f32]) {
        self.output.pull_null(out);
    }
}
