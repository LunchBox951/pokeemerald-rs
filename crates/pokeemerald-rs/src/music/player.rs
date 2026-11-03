//! Drives one sequencer frame per game frame and feeds it to an audio-output ring.
//!
//! Startup queues half the ring before starting the device. The free half
//! absorbs drift between the game loop and audio clock. Underrun and overrun
//! counters expose failure in either direction.
//!
//! Fade-out follows `m4aMPlayFadeOut`'s step schedule (`m4a.c:692`-`:756`),
//! feeding each step's `volX` to [`Sequencer::render_frame_with_fade`].

use audio::{
    Sequencer, Song, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES, MIXER_RATE, SAMPLES_PER_FRAME,
};
use platform::{AudioOutput, PlatformError, Producer};

use super::MusicError;

/// Ring-buffer capacity in stereo frames.
pub const RING_CAPACITY_FRAMES: usize = 4096;

const RING_PREFILL_DIVISOR: usize = 2;

const FADE_VOL_SHIFT: u32 = 2;
const FADE_VOL_MAX: i32 = 64;
const FADE_VOL_STEP: i32 = 4 << FADE_VOL_SHIFT;

/// Frames between title-music fade steps.
pub const TITLE_FADE_OUT_SPEED: u16 = 4;

/// Bounds [`MusicPlayer::drained`]'s wait for a `ring_capacity`-sample ring
/// to empty, so a stalled consumer cannot hold the transition open forever:
/// twice the frames a full ring needs to drain at one rendered frame per game
/// frame, but never fewer than `device_tail_frames`.
///
/// The ring-only figure assumes the consumer drains about as often as the
/// game renders. A device whose callback period outlasts it leaves the ring
/// nonempty until its next callback -- a healthy stream waiting its turn, not
/// a stalled one -- so the same bound that decides how long that device's
/// buffers take to sound also floors the wait for them to be taken.
pub(super) fn max_drain_wait_frames(ring_capacity: usize, device_tail_frames: usize) -> usize {
    (2 * ring_capacity.div_ceil(Sequencer::FRAME_SAMPLES)).max(device_tail_frames)
}

/// Added to the device's advertised callback bound in [`device_tail_millis`]
/// for the queueing between a callback returning and its samples sounding,
/// which the advertisement does not describe.
pub(super) const DEVICE_TAIL_MARGIN_MILLIS: usize = 50;

/// Callback periods the host keeps queued behind the one being filled:
/// cpal's ALSA path holds two, and no supported host holds more.
const HOST_QUEUED_PERIODS: u64 = 2;

/// Floor for [`device_tail_millis`], and the whole wait for a device that
/// advertises no callback bound at all.
pub(super) const DEVICE_TAIL_FLOOR_MILLIS: usize = 200;

/// Ceiling for [`device_tail_millis`]: the advertised bound is an unvalidated
/// device-reported `u32`, so an outsized one must not hold the audio device
/// open for as long as it claims.
pub(super) const DEVICE_TAIL_MAX_MILLIS: usize = 1_000;

/// [`DEVICE_TAIL_FLOOR_MILLIS`] in game frames, the wait every device gets at
/// least.
pub const DEVICE_TAIL_FLOOR_FRAMES: usize = game_frames_in(DEVICE_TAIL_FLOOR_MILLIS);

/// How long a healthy stream stays open past its empty ring, for the samples
/// the callback already took to sound.
///
/// The transport reports no playback position and caps no latency --
/// `build_stream` opens the device's default buffer size -- so this is
/// derived, not measured: [`HOST_QUEUED_PERIODS`] of the largest callback
/// buffer the device advertises, at its own rate, plus
/// [`DEVICE_TAIL_MARGIN_MILLIS`], held between
/// [`DEVICE_TAIL_FLOOR_MILLIS`] and [`DEVICE_TAIL_MAX_MILLIS`]. A device
/// advertising no concrete range, or no rate, gets the floor.
pub(super) fn device_tail_millis(
    max_callback_frames: Option<usize>,
    device_sample_rate: u32,
) -> usize {
    let (Some(frames), rate @ 1..) = (max_callback_frames, u64::from(device_sample_rate)) else {
        return DEVICE_TAIL_FLOOR_MILLIS;
    };
    let buffered = u64::try_from(frames)
        .unwrap_or(u64::MAX)
        .saturating_mul(1000 * HOST_QUEUED_PERIODS)
        / rate;
    usize::try_from(buffered)
        .unwrap_or(usize::MAX)
        .saturating_add(DEVICE_TAIL_MARGIN_MILLIS)
        .clamp(DEVICE_TAIL_FLOOR_MILLIS, DEVICE_TAIL_MAX_MILLIS)
}

/// How long a healthy stream may leave the ring nonempty between callbacks,
/// the floor [`max_drain_wait_frames`] takes: the derived device tail, or
/// [`DEVICE_TAIL_MAX_MILLIS`] for a device advertising no bound, whose
/// cadence is unknown rather than short.
pub(super) fn callback_cadence_frames(
    max_callback_frames: Option<usize>,
    device_sample_rate: u32,
) -> usize {
    match (max_callback_frames, device_sample_rate) {
        (Some(_), 1..) => {
            game_frames_in(device_tail_millis(max_callback_frames, device_sample_rate))
        }
        _ => game_frames_in(DEVICE_TAIL_MAX_MILLIS),
    }
}

/// `millis` as whole game frames, the unit [`MusicPlayer::drained`] polls in.
pub(super) const fn game_frames_in(millis: usize) -> usize {
    (millis * MIXER_RATE as usize).div_ceil(1000 * SAMPLES_PER_FRAME)
}

/// The measured-wait target [`MusicPlayer::drained`] latches: `submitted_frames`
/// plus `settle_margin_frames` (the output's own
/// [`AudioOutput::playback_settle_margin_frames`], read at the same poll).
///
/// A resampled output keeps buffered interpolation state, so its source ring
/// reading empty does not mean it is done producing real, audible content --
/// the deferred lookahead pull can still blend already-buffered real data
/// into the very next callback (see `platform::Resampler`'s module docs).
/// Latching the raw `submitted_frames` count alone would let `drained` end
/// the wait one callback before that real tail finishes sounding; adding the
/// margin covers it. Zero margin (the null backend, or an exact-rate device)
/// leaves the raw count unchanged. Saturates rather than overflowing --
/// `submitted_frames` is a real device's frame counter, nowhere near `u64::MAX`.
pub(super) fn measured_drain_target(submitted_frames: u64, settle_margin_frames: u64) -> u64 {
    submitted_frames.saturating_add(settle_margin_frames)
}

/// Audio state inherited by songs started in the same session.
///
/// Songs without a reverb override inherit the most recently resolved level,
/// matching `m4aSoundMode` (`m4a.c:661`-`:662`).
#[derive(Debug, Clone, Copy)]
pub struct MusicContext {
    pub(super) master_reverb: u8,
}

impl MusicContext {
    /// Creates a session with the driver's initial zero reverb.
    #[must_use]
    pub fn new() -> Self {
        Self { master_reverb: 0 }
    }
}

impl Default for MusicContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
struct FadeOut {
    interval: u16,
    counter: u16,
    volume: i32,
    finished: bool,
}

impl FadeOut {
    fn new(speed: u16) -> Self {
        let interval = speed.max(1);
        Self {
            interval,
            counter: interval,
            volume: FADE_VOL_MAX << FADE_VOL_SHIFT,
            finished: false,
        }
    }

    /// Advances the schedule one step and returns the current `volX` input
    /// to `TrkVolPitSet` (`m4a.c:756`, `:772`), in `0..=64`.
    fn step(&mut self) -> u8 {
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
        u8::try_from(self.volume >> FADE_VOL_SHIFT).unwrap_or(0)
    }
}

/// A song playing through an audio-output ring.
pub struct MusicPlayer {
    song: Song,
    pub(super) sequencer: Sequencer,
    producer: Producer,
    pub(super) output: AudioOutput,
    overruns: u64,
    fade: Option<FadeOut>,
    resolved_reverb: u8,
    /// The ring's fixed total size in samples, which [`Self::drained`]
    /// compares free space against to decide the ring is empty. Read from the
    /// ring itself, not from the free space at construction, so a caller that
    /// queued through [`AudioOutput::producer`] before starting cannot make a
    /// still-queued ring read as drained.
    ring_capacity: usize,
    /// [`Self::drained`]'s poll bound, from [`max_drain_wait_frames`].
    pub(super) max_drain_wait_frames: usize,
    /// [`Self::drained`]'s poll count since the fade finished.
    drain_wait_frames: usize,
    /// [`Self::drained`]'s poll count since the ring first read empty.
    device_tail_frames: usize,
    /// [`Self::drained`]'s device-tail bound, from [`device_tail_millis`] for
    /// the output this instance was started with.
    pub(super) max_device_tail_frames: usize,
    /// The [`measured_drain_target`] [`Self::drained`] latched the first poll
    /// it saw the ring empty, from [`AudioOutput::playback_progress`] and
    /// [`AudioOutput::playback_settle_margin_frames`] -- `None` either before
    /// that first empty poll, or when the output had no measured signal to
    /// latch at that moment, in which case [`Self::max_device_tail_frames`]
    /// alone governs the wait exactly as before.
    measured_drain_target: Option<u64>,
}

impl MusicPlayer {
    /// Loads a packed song, opens an audio output, and starts playback.
    ///
    /// Songs without a reverb override use zero. Use
    /// [`Self::start_from_pack_with_context`] to inherit session state.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError`] when song loading or audio startup fails.
    pub fn start_from_pack(
        pack: &assets::AssetPack,
        song_name: &str,
        open_audio: impl FnOnce() -> Result<AudioOutput, PlatformError>,
    ) -> Result<Self, MusicError> {
        Self::start_from_pack_with_context(&mut MusicContext::new(), pack, song_name, open_audio)
    }

    /// Loads and starts a packed song with session reverb inheritance.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError`] when song loading or audio startup fails.
    pub fn start_from_pack_with_context(
        context: &mut MusicContext,
        pack: &assets::AssetPack,
        song_name: &str,
        open_audio: impl FnOnce() -> Result<AudioOutput, PlatformError>,
    ) -> Result<Self, MusicError> {
        let song = super::load_song_from_pack(pack, song_name)?;
        let output = open_audio()?;
        Self::start_with_context(context, song, output).map_err(MusicError::from)
    }

    /// Starts an already-resolved song after prefilling the output ring.
    ///
    /// Songs without a reverb override use zero. Use [`Self::start_with_context`]
    /// to inherit session state.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError`] if the output refuses to start.
    pub fn start(song: Song, output: AudioOutput) -> Result<Self, PlatformError> {
        Self::start_with_context(&mut MusicContext::new(), song, output)
    }

    /// Starts a song with its reverb override or the session's inherited level.
    /// The resolved level updates `context` after the output starts.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError`] if the output refuses to start.
    pub fn start_with_context(
        context: &mut MusicContext,
        song: Song,
        output: AudioOutput,
    ) -> Result<Self, PlatformError> {
        Self::start_with_context_and_starter(context, song, output, AudioOutput::start)
    }

    pub(super) fn start_with_context_and_starter(
        context: &mut MusicContext,
        song: Song,
        mut output: AudioOutput,
        start_output: impl FnOnce(&mut AudioOutput) -> Result<(), PlatformError>,
    ) -> Result<Self, PlatformError> {
        let reverb_level = song.reverb_override().unwrap_or(context.master_reverb);
        let mut sequencer = Sequencer::with_resolved_reverb(
            song.clone(),
            DEFAULT_MASTER_VOLUME,
            DEFAULT_MAX_VOICES,
            reverb_level,
        );
        let producer = output.producer();
        let ring_capacity = producer.capacity();
        let advertised = output.max_callback_frames();
        let rate = output.device_sample_rate();
        let device_tail = game_frames_in(device_tail_millis(advertised, rate));
        let cadence = callback_cadence_frames(advertised, rate);
        let overruns = prefill(&mut sequencer, &producer);
        start_output(&mut output)?;
        context.master_reverb = reverb_level;
        Ok(Self {
            song,
            sequencer,
            producer,
            output,
            overruns,
            fade: None,
            resolved_reverb: reverb_level,
            ring_capacity,
            max_drain_wait_frames: max_drain_wait_frames(ring_capacity, cadence),
            drain_wait_frames: 0,
            device_tail_frames: 0,
            max_device_tail_frames: device_tail,
            measured_drain_target: None,
        })
    }

    /// Renders and queues one game frame of audio.
    ///
    /// Restarts a finished song with its resolved reverb instead of leaving
    /// the stream silent. Looping BGM normally never reaches this path. A
    /// song the terminal fade step paused is never restarted: upstream
    /// leaves it stopped in `MUSICPLAYER_STATUS_PAUSE` (`m4a.c:740`) and
    /// only keeps mixing, which is what the frames after that step render.
    pub fn advance_frame(&mut self) {
        if self.sequencer.is_finished() && !self.sequencer.is_paused() {
            self.sequencer = Sequencer::with_resolved_reverb(
                self.song.clone(),
                DEFAULT_MASTER_VOLUME,
                DEFAULT_MAX_VOICES,
                self.resolved_reverb,
            );
        }
        // MPlayMain advances FadeOutBody before this frame's tick.
        let fade_vol_x = self.fade.as_mut().map(FadeOut::step);
        let mut buffer = [0.0_f32; Sequencer::FRAME_SAMPLES];
        self.sequencer
            .render_frame_with_fade(&mut buffer, fade_vol_x);
        let pushed = self.producer.push(&buffer);
        self.overruns += (buffer.len() - pushed) as u64;
    }

    /// Starts an `m4aMPlayFadeOut`-scheduled fade with `speed` frames per step.
    ///
    /// A zero speed is treated as one. Calling this during a fade does not
    /// restart the fade.
    pub fn fade_out(&mut self, speed: u16) {
        if self.fade.is_none() {
            self.fade = Some(FadeOut::new(speed));
        }
    }

    /// Cancels an in-progress fade, so the next [`Self::advance_frame`]
    /// renders at full volume again. A no-op with no fade active.
    pub fn cancel_fade(&mut self) {
        if self.fade.take().is_some() {
            self.sequencer.restore_full_volume();
        }
    }

    /// Returns whether the active fade has reached silence.
    ///
    /// The terminal step stops every track, so no voice sounds after it --
    /// but the master-mix reverb ring still holds the frames it delayed, and
    /// upstream's mixer keeps running through the pause (`SoundMain` and
    /// `SoundMainRAM_Reverb`, `m4a_1.s:20`-`:119`). Callers that stop
    /// rendering here must keep [`Self::advance_frame`] going while
    /// [`Self::tail_sounding`].
    #[must_use]
    pub fn fade_finished(&self) -> bool {
        self.fade.is_some_and(|fade| fade.finished)
    }

    /// Whether another [`Self::advance_frame`] would still render sound
    /// after the fade's terminal step: the master-mix reverb ring's delayed
    /// samples ring down over the frames that follow it, as does any voice
    /// an already-ended track left in release, and cutting either short
    /// truncates the song's tail.
    #[must_use]
    pub fn tail_sounding(&self) -> bool {
        self.sequencer.is_sounding()
    }

    /// Whether it is now safe to drop this player, which closes the output
    /// stream where it stands rather than playing out what it holds.
    ///
    /// An empty ring only proves the output callback took the last samples,
    /// so a healthy stream is held further frames for them to sound. The
    /// first poll that sees the ring empty latches
    /// [`measured_drain_target`] of [`AudioOutput::playback_progress`]'s
    /// submitted-frame count -- widened by [`AudioOutput::playback_settle_margin_frames`]
    /// so a resampled output's buffered interpolation tail is not cut off --
    /// then every later poll returns `true` as soon as the output's measured
    /// sounded-frame count reaches it: waiting on the device's own measured
    /// playback position, not a poll count. If the output has no measured
    /// signal at that first empty poll (a host that reports no usable
    /// timestamp, including an unmodified null backend), this falls back to
    /// [`device_tail_millis`] for this output's own device exactly as
    /// before. Either way [`Self::max_device_tail_frames`] remains an
    /// absolute cap: a stalled device, or a measured signal that stalls,
    /// still cannot hold the transition open past it.
    ///
    /// An elapsed [`max_drain_wait_frames`] bound answers `true` while the
    /// ring stays nonempty, so a consumer that never takes the fade cannot
    /// hold the device open; a stream error is not read, since cpal's ALSA
    /// worker reports a recoverable XRUN through the same counter and keeps
    /// running. Counts one poll per call. A ring seen empty clears the
    /// stall count, and a ring that refills restarts the device tail (and
    /// the latched measured target); meaningful only once
    /// [`Self::fade_finished`].
    #[must_use]
    pub fn drained(&mut self) -> bool {
        if self.producer.available_space() >= self.ring_capacity {
            self.drain_wait_frames = 0;
            if self.device_tail_frames == 0 {
                let margin = self.output.playback_settle_margin_frames();
                self.measured_drain_target = self
                    .output
                    .playback_progress()
                    .map(|p| measured_drain_target(p.submitted_frames, margin));
            }
            self.device_tail_frames += 1;
            if let Some(target) = self.measured_drain_target {
                if let Some(progress) = self.output.playback_progress() {
                    if progress.sounded_frames >= target {
                        return true;
                    }
                }
            }
            return self.device_tail_frames > self.max_device_tail_frames;
        }
        self.device_tail_frames = 0;
        self.measured_drain_target = None;
        self.drain_wait_frames += 1;
        self.drain_wait_frames >= self.max_drain_wait_frames
    }

    /// Returns the number of samples replaced with silence after an underrun.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.output.underruns()
    }

    /// Returns the number of rendered samples dropped because the ring was full.
    #[must_use]
    pub fn overruns(&self) -> u64 {
        self.overruns
    }

    /// Returns whether the audio output is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.output.is_running()
    }
}

fn prefill(sequencer: &mut Sequencer, producer: &Producer) -> u64 {
    let target = producer.available_space() / RING_PREFILL_DIVISOR;
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
    pub(crate) fn drain_null_for_test(&mut self, out: &mut [f32]) {
        self.output.pull_null(out);
    }

    pub(crate) fn ring_free_for_test(&self) -> usize {
        self.producer.available_space()
    }

    pub(crate) fn ring_capacity_for_test(&self) -> usize {
        self.ring_capacity
    }
}
