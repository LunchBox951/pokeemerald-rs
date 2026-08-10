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
//! rather than immediately underrunning while the frame loop catches up. It
//! fills to [`PREFILL_FRACTION`] of the ring rather than to the brim: a full
//! ring is all latency and no slack, and leaves nowhere for a frame to land
//! when the game loop and the audio device clock drift apart.
//!
//! # Stream health
//!
//! Both directions of ring misuse are counted, not just one:
//! [`MusicPlayer::underruns`] (device wanted samples that were not there)
//! and [`MusicPlayer::overruns`] (this player rendered samples the ring had
//! no room for, which `platform::Producer::push` drops). A healthy
//! frame-driven stream holds both at zero.
//!
//! # Fading out
//!
//! Upstream does not hard-cut the title BGM when A/START is pressed: it
//! calls `FadeOutBGM(4)` (`pokeemerald/src/title_screen.c:784`) before
//! `CB2_GoToMainMenu` (`:786`). [`MusicPlayer::fade_out`] models that fade —
//! see [`FadeOut`] for the upstream arithmetic and the one documented
//! divergence.

use audio::{Sequencer, Song};
use platform::{AudioOutput, PlatformError, Producer};

use super::MusicError;

/// Ring buffer capacity, in stereo frames -- about 306 ms of headroom at the
/// nominal 13379 Hz mixer rate (matches
/// `crates/audio/examples/play_song.rs`'s own `AudioOutput::open` sizing).
pub const RING_CAPACITY_FRAMES: usize = 4096;

/// How much of the ring [`MusicPlayer::start`] prefills, as a divisor: `2`
/// means "about half".
///
/// Half rather than "as much as fits": the remaining half is the headroom
/// that absorbs drift between the game loop's frame cadence and the audio
/// device's own clock, so a frame that arrives slightly early has somewhere
/// to go instead of being dropped by `platform::Producer::push`. It also
/// halves the latency the prefill itself adds -- roughly 306 ms down to
/// roughly 153 ms at [`RING_CAPACITY_FRAMES`] and the nominal mixer rate.
const PREFILL_FRACTION: usize = 2;

/// `FADE_VOL_SHIFT` (`pokeemerald/include/gba/m4a_internal.h:325`): the fade
/// volume is tracked at 4x resolution and shifted down when it is handed to a
/// track as `volX`.
const FADE_VOL_SHIFT: u32 = 2;

/// `FADE_VOL_MAX` (`m4a_internal.h:324`): the unfaded `track->volX`, i.e. the
/// value that means "full volume".
const FADE_VOL_MAX: i32 = 64;

/// One fade step, in [`FADE_VOL_SHIFT`]-scaled units: `4 << FADE_VOL_SHIFT`
/// (`m4a.c:715`). Sixteen of these take a fade from full to silent.
const FADE_VOL_STEP: i32 = 4 << FADE_VOL_SHIFT;

/// `FadeOutBGM`'s speed at the title screen's A/START handler
/// (`title_screen.c:784`) -- the number of frames between fade steps.
pub const TITLE_FADE_OUT_SPEED: u16 = 4;

/// A running `m4aMPlayFadeOut` (`m4a.c:202`), stepped once per rendered
/// frame exactly as `MPlayMain` steps `FadeOutBody` (`m4a.c:692`) once per
/// V-blank.
///
/// # Upstream arithmetic
///
/// `MPlayFadeOut` seeds `fadeOC = fadeOI = speed` and
/// `fadeOV = 64 << FADE_VOL_SHIFT` (`m4a.c:63`-`:65`). Every `FadeOutBody`
/// call decrements `fadeOC` and returns unless it hit zero (`m4a.c:700`);
/// on the frames it does, `fadeOC` is reloaded from `fadeOI` and `fadeOV`
/// drops by `4 << FADE_VOL_SHIFT` (`m4a.c:715`). Each surviving track then
/// takes `volX = fadeOV >> FADE_VOL_SHIFT` (`m4a.c:756`). When `fadeOV`
/// reaches zero or below, every track is stopped and the player is paused
/// (`m4a.c:717`-`:744`) -- the point at which the song is really over.
///
/// For `speed == 4` that is 16 steps of 4/64 spaced 4 frames apart: full
/// volume for frames 0..4, 60/64 for 4..8, ... 4/64 for 60..64, silent and
/// finished at frame 64 (about 1.07 s at 59.7275 Hz).
///
/// # Divergence `(behavioral-fidelity)`
///
/// Upstream multiplies each *track's* volume by `volX` before mixing
/// (`TrkVolPitSet`, `m4a.c:772`), so the fade is applied per channel, in
/// integer arithmetic, upstream of the master-mix reverb's feedback loop.
/// This port has no sequencer-level track-volume knob to reach
/// (`audio::Sequencer` exposes no per-track volume override, and adding one
/// is out of scope for this slice), so it applies the same `volX / 64` gain
/// to the already-mixed `f32` frame instead. The schedule, the step size and
/// the total duration are upstream's exactly; what differs is that the gain
/// is applied once to the sum rather than per track (so per-channel integer
/// rounding differs by well under an LSB), and that the reverb tail
/// recirculating inside `audio::Mixer` is faded on its way out rather than
/// being fed the already-faded dry signal -- audibly, a marginally longer
/// tail at the very end of the fade.
#[derive(Clone, Copy, Debug)]
struct FadeOut {
    /// `MusicPlayerInfo::fadeOI`: frames between steps.
    interval: u16,
    /// `MusicPlayerInfo::fadeOC`: frames left before the next step.
    counter: u16,
    /// `MusicPlayerInfo::fadeOV`: the [`FADE_VOL_SHIFT`]-scaled volume.
    volume: i32,
    /// Whether the fade has run out (upstream's "stop every track and pause
    /// the player" terminal state).
    finished: bool,
}

impl FadeOut {
    /// Seed a fade at `speed` frames per step (`MPlayFadeOut`,
    /// `m4a.c:63`-`:65`). A `speed` of `0` would never step, so it is
    /// clamped to `1`, matching how every caller in `sound.c` passes a
    /// nonzero literal.
    fn new(speed: u16) -> Self {
        let interval = speed.max(1);
        Self {
            interval,
            counter: interval,
            volume: FADE_VOL_MAX << FADE_VOL_SHIFT,
            finished: false,
        }
    }

    /// One `FadeOutBody` call (module docs): advance the fade by a frame and
    /// return the gain to apply to that frame's render.
    fn step(&mut self) -> f32 {
        if !self.finished {
            self.counter -= 1;
            if self.counter == 0 {
                self.counter = self.interval;
                self.volume -= FADE_VOL_STEP;
                if self.volume <= 0 {
                    self.volume = 0;
                    self.finished = true;
                }
            }
        }
        // `track->volX = fadeOV >> FADE_VOL_SHIFT` against a nominal maximum
        // of `FADE_VOL_MAX` (module docs). Both sides are integers in
        // `0..=64`, so the ratio is exact in `f32`.
        #[allow(clippy::cast_precision_loss)]
        let gain = (self.volume >> FADE_VOL_SHIFT) as f32 / FADE_VOL_MAX as f32;
        gain
    }
}

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
    /// dropping its `MusicPlayer` once the title flow's fade-out has run to
    /// completion.
    output: AudioOutput,
    /// Total samples this player rendered that the ring had no room for
    /// (module docs' "Stream health"). Surfaced by [`Self::overruns`].
    overruns: u64,
    /// The running `FadeOutBGM`, once [`Self::fade_out`] has been called.
    fade: Option<FadeOut>,
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
    /// about half the ring, then starts the device.
    ///
    /// # Errors
    ///
    /// [`PlatformError`] if `output` refuses to start (never happens for the
    /// null backend).
    pub fn start(song: Song, mut output: AudioOutput) -> Result<Self, PlatformError> {
        let mut sequencer = Sequencer::new(song.clone());
        let producer = output.producer();
        let overruns = prefill(&mut sequencer, &producer);
        output.start()?;
        Ok(Self {
            song,
            sequencer,
            producer,
            output,
            overruns,
            fade: None,
        })
    }

    /// Advance playback by exactly one game frame: render one
    /// [`audio::Sequencer`] frame, apply any running fade's gain, and push it
    /// to the device ring.
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
        // `MPlayMain` steps the fade before `SoundMainRAM` mixes the frame it
        // applies to ([`FadeOut`]'s docs), so step first, then scale.
        let gain = self.fade.as_mut().map(FadeOut::step);
        let mut buffer = [0.0_f32; Sequencer::FRAME_SAMPLES];
        self.sequencer.render_frame(&mut buffer);
        if let Some(gain) = gain {
            for sample in &mut buffer {
                *sample *= gain;
            }
        }
        let pushed = self.producer.push(&buffer);
        self.overruns += (buffer.len() - pushed) as u64;
    }

    /// Begin an `m4aMPlayFadeOut`-equivalent fade at `speed` frames per step
    /// ([`FadeOut`]'s docs) -- for the title screen, `TITLE_FADE_OUT_SPEED`.
    ///
    /// Idempotent: calling it again while a fade is already running leaves
    /// the running fade alone rather than restarting it at full volume,
    /// which is what lets [`crate::App`] call it unconditionally on every
    /// frame the scene is no longer the title screen.
    pub fn fade_out(&mut self, speed: u16) {
        if self.fade.is_none() {
            self.fade = Some(FadeOut::new(speed));
        }
    }

    /// Whether a fade started by [`Self::fade_out`] has run all the way to
    /// upstream's "stop every track and pause the player" terminal state
    /// (`m4a.c:717`-`:744`) -- i.e. whether this player can now be dropped.
    #[must_use]
    pub fn fade_finished(&self) -> bool {
        self.fade.is_some_and(|fade| fade.finished)
    }

    /// Total samples played as silence so far due to ring-buffer underrun --
    /// the V-5 "no underrun in the frame-driven path" evidence this slice's
    /// definition of done asks for.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.output.underruns()
    }

    /// Total samples dropped so far because the ring was full when this
    /// player pushed a frame (module docs' "Stream health") -- the other
    /// half of the same evidence, which a discarded `push` return value
    /// would have hidden.
    #[must_use]
    pub fn overruns(&self) -> u64 {
        self.overruns
    }

    /// Whether the underlying device output is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.output.is_running()
    }
}

/// Render and push whole frames until about [`PREFILL_FRACTION`] of the ring
/// is queued -- [`MusicPlayer::start`]'s prefill step (module docs).
/// Returns however many samples were nevertheless dropped (zero, unless the
/// ring is too small to hold a single frame).
///
/// The ring is empty when this runs, so `available_space` is its capacity.
fn prefill(sequencer: &mut Sequencer, producer: &Producer) -> u64 {
    let target = producer.available_space() / PREFILL_FRACTION;
    let mut buffer = [0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut queued = 0;
    let mut dropped = 0;
    while queued + Sequencer::FRAME_SAMPLES <= target {
        sequencer.render_frame(&mut buffer);
        let pushed = producer.push(&buffer);
        dropped += (buffer.len() - pushed) as u64;
        queued += Sequencer::FRAME_SAMPLES;
    }
    dropped
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

    /// Test-only: how much room the ring has left, for asserting the
    /// prefill's own sizing against a capacity the test captured before
    /// [`MusicPlayer::start`] consumed the output.
    pub(crate) fn ring_free_for_test(&self) -> usize {
        self.producer.available_space()
    }
}
