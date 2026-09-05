//! Drives one sequencer frame per game frame and feeds it to an audio-output ring.
//!
//! Startup queues half the ring before starting the device. The free half
//! absorbs drift between the game loop and audio clock. Underrun and overrun
//! counters expose failure in either direction.
//!
//! Fade-out follows `m4aMPlayFadeOut`'s step schedule (`m4a.c:692`-`:756`).
//! The runtime has no per-track gain, so the player scales the mixed frame.
//! This also scales reverb already in the mix and ends its tail sooner than
//! applying the fade before the original feedback loop.

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
fn max_drain_wait_frames(ring_capacity: usize, device_tail_frames: usize) -> usize {
    (2 * ring_capacity.div_ceil(Sequencer::FRAME_SAMPLES)).max(device_tail_frames)
}

/// Added to the device's advertised callback bound in [`device_tail_millis`]
/// for the queueing between a callback returning and its samples sounding,
/// which the advertisement does not describe.
const DEVICE_TAIL_MARGIN_MILLIS: usize = 50;

/// Floor for [`device_tail_millis`], and the whole wait for a device that
/// advertises no callback bound at all.
const DEVICE_TAIL_FLOOR_MILLIS: usize = 200;

/// Ceiling for [`device_tail_millis`]: the advertised bound is an unvalidated
/// device-reported `u32`, so an outsized one must not hold the audio device
/// open for as long as it claims.
const DEVICE_TAIL_MAX_MILLIS: usize = 1_000;

/// [`DEVICE_TAIL_FLOOR_MILLIS`] in game frames, the wait every device gets at
/// least.
pub const DEVICE_TAIL_FLOOR_FRAMES: usize = game_frames_in(DEVICE_TAIL_FLOOR_MILLIS);

/// How long a healthy stream stays open past its empty ring, for the samples
/// the callback already took to sound.
///
/// The transport reports no playback position and caps no latency --
/// `build_stream` opens the device's default buffer size -- so this is
/// derived, not measured: the largest callback buffer the device advertises,
/// at its own rate, plus [`DEVICE_TAIL_MARGIN_MILLIS`], held between
/// [`DEVICE_TAIL_FLOOR_MILLIS`] and [`DEVICE_TAIL_MAX_MILLIS`]. A device
/// advertising no concrete range, or no rate, gets the floor.
fn device_tail_millis(max_callback_frames: Option<usize>, device_sample_rate: u32) -> usize {
    let (Some(frames), rate @ 1..) = (max_callback_frames, u64::from(device_sample_rate)) else {
        return DEVICE_TAIL_FLOOR_MILLIS;
    };
    let buffered = u64::try_from(frames)
        .unwrap_or(u64::MAX)
        .saturating_mul(1000)
        / rate;
    usize::try_from(buffered)
        .unwrap_or(usize::MAX)
        .saturating_add(DEVICE_TAIL_MARGIN_MILLIS)
        .clamp(DEVICE_TAIL_FLOOR_MILLIS, DEVICE_TAIL_MAX_MILLIS)
}

/// `millis` as whole game frames, the unit [`MusicPlayer::drained`] polls in.
const fn game_frames_in(millis: usize) -> usize {
    (millis * MIXER_RATE as usize).div_ceil(1000 * SAMPLES_PER_FRAME)
}

/// Audio state inherited by songs started in the same session.
///
/// Songs without a reverb override inherit the most recently resolved level,
/// matching `m4aSoundMode` (`m4a.c:661`-`:662`).
#[derive(Debug, Clone, Copy)]
pub struct MusicContext {
    master_reverb: u8,
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
        #[expect(
            clippy::cast_precision_loss,
            reason = "fade volume values from zero through 64 are exact in f32"
        )]
        let gain = (self.volume >> FADE_VOL_SHIFT) as f32 / FADE_VOL_MAX as f32;
        gain
    }
}

/// A song playing through an audio-output ring.
pub struct MusicPlayer {
    song: Song,
    sequencer: Sequencer,
    producer: Producer,
    output: AudioOutput,
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
    max_drain_wait_frames: usize,
    /// [`Self::drained`]'s poll count since the fade finished.
    drain_wait_frames: usize,
    /// [`Self::drained`]'s poll count since the ring first read empty.
    device_tail_frames: usize,
    /// [`Self::drained`]'s device-tail bound, from [`device_tail_millis`] for
    /// the output this instance was started with.
    max_device_tail_frames: usize,
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

    fn start_with_context_and_starter(
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
        let device_tail = game_frames_in(device_tail_millis(
            output.max_callback_frames(),
            output.device_sample_rate(),
        ));
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
            max_drain_wait_frames: max_drain_wait_frames(ring_capacity, device_tail),
            drain_wait_frames: 0,
            device_tail_frames: 0,
            max_device_tail_frames: device_tail,
        })
    }

    /// Renders and queues one game frame of audio.
    ///
    /// Restarts a finished song with its resolved reverb instead of leaving
    /// the stream silent. Looping BGM normally never reaches this path.
    pub fn advance_frame(&mut self) {
        if self.sequencer.is_finished() {
            self.sequencer = Sequencer::with_resolved_reverb(
                self.song.clone(),
                DEFAULT_MASTER_VOLUME,
                DEFAULT_MAX_VOICES,
                self.resolved_reverb,
            );
        }
        // MPlayMain advances FadeOutBody before mixing the affected frame.
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

    /// Starts an `m4aMPlayFadeOut`-scheduled fade with `speed` frames per step.
    ///
    /// A zero speed is treated as one. Calling this during a fade does not
    /// restart the fade.
    pub fn fade_out(&mut self, speed: u16) {
        if self.fade.is_none() {
            self.fade = Some(FadeOut::new(speed));
        }
    }

    /// Returns whether the active fade has reached silence.
    #[must_use]
    pub fn fade_finished(&self) -> bool {
        self.fade.is_some_and(|fade| fade.finished)
    }

    /// Whether it is now safe to drop this player, which closes the output
    /// stream where it stands rather than playing out what it holds.
    ///
    /// An empty ring only proves the output callback took the last samples,
    /// so a healthy stream is held further frames for them to sound --
    /// [`device_tail_millis`] for this output's own device, not a fixed wait.
    /// A reported stream error ([`AudioOutput::stream_errors`]) or an elapsed
    /// [`max_drain_wait_frames`] bound answers `true` at once: neither has a
    /// tail left to play. Counts one poll per call; meaningful only once
    /// [`Self::fade_finished`].
    #[must_use]
    pub fn drained(&mut self) -> bool {
        if self.output.stream_errors() > 0 {
            return true;
        }
        if self.producer.available_space() >= self.ring_capacity {
            self.device_tail_frames += 1;
            return self.device_tail_frames > self.max_device_tail_frames;
        }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use audio::{Adsr, Event, Instrument, Song, ToneData, WaveData};
    use platform::{AudioOutput, PlatformError};

    use super::{
        device_tail_millis, game_frames_in, max_drain_wait_frames, MusicContext, MusicPlayer,
        DEVICE_TAIL_FLOOR_MILLIS, DEVICE_TAIL_MARGIN_MILLIS, DEVICE_TAIL_MAX_MILLIS,
        RING_CAPACITY_FRAMES,
    };

    fn short_song_without_its_own_reverb() -> Song {
        let wave = Arc::new(WaveData::one_shot(1 << 20, vec![100; 64]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let events = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(2),
            Event::Fine,
        ];
        Song::new(voices, vec![events], 150)
    }

    const REVERB_TAIL_PROBE_FRAMES: usize = 25;

    fn first_playthrough_finishes_within(player: &mut MusicPlayer, budget: usize) -> bool {
        for _ in 0..budget {
            player.advance_frame();
            if player.sequencer.is_finished() {
                return true;
            }
        }
        false
    }

    /// A device that advertises no concrete buffer range says nothing about
    /// its latency, so the floor is the whole wait.
    #[test]
    fn an_unadvertised_callback_bound_waits_the_floor() {
        assert_eq!(device_tail_millis(None, 48_000), DEVICE_TAIL_FLOOR_MILLIS);
    }

    /// A rate of zero cannot turn a frame count into a duration; the floor
    /// stands rather than a division by zero or a nonsense wait.
    #[test]
    fn a_rateless_device_waits_the_floor() {
        assert_eq!(device_tail_millis(Some(4_096), 0), DEVICE_TAIL_FLOOR_MILLIS);
    }

    /// A small callback buffer derives a wait under the floor, and the floor
    /// wins: the advertised bound covers the callback, not the OS queueing
    /// behind it.
    #[test]
    fn a_short_callback_bound_still_waits_the_floor() {
        assert_eq!(
            device_tail_millis(Some(512), 48_000),
            DEVICE_TAIL_FLOOR_MILLIS
        );
    }

    /// The case the fixed 200 ms wait got wrong: a high-latency
    /// configuration whose own callback buffer outlasts the floor must widen
    /// the wait rather than expire mid-buffer.
    #[test]
    fn a_callback_bound_past_the_floor_widens_the_wait() {
        let tail = device_tail_millis(Some(24_000), 48_000);
        assert_eq!(tail, 500 + DEVICE_TAIL_MARGIN_MILLIS);
        assert!(tail > DEVICE_TAIL_FLOOR_MILLIS);
    }

    /// An advertised maximum the device may never reach must not hold the
    /// title -> main menu transition open for as long as it claims.
    #[test]
    fn an_outsized_callback_bound_is_capped() {
        assert_eq!(
            device_tail_millis(Some(480_000), 48_000),
            DEVICE_TAIL_MAX_MILLIS
        );
    }

    /// The pre-empty bound governs a ring that has not drained yet. Derived
    /// from the ring alone it assumes the consumer drains about as often as
    /// the game renders: at the production ring that expires after 38 game
    /// frames, roughly 0.64 s. A device whose callback period outlasts that
    /// leaves the ring nonempty until its next callback, so the bound must
    /// widen to that device's tail rather than call a healthy stream stalled
    /// and drop the queued fade.
    #[test]
    fn a_long_advertised_callback_widens_the_pre_empty_bound() {
        let ring_capacity = RING_CAPACITY_FRAMES * usize::from(AudioOutput::CHANNELS);
        let ring_only = max_drain_wait_frames(ring_capacity, 0);
        let device_tail = game_frames_in(device_tail_millis(Some(48_000), 48_000));

        assert!(
            device_tail > ring_only,
            "a one-second callback bound must outlast the {ring_only}-frame ring-only figure, \
             or this test proves nothing"
        );
        assert_eq!(
            max_drain_wait_frames(ring_capacity, device_tail),
            device_tail,
            "the pre-empty bound must cover the device's own callback cadence"
        );
    }

    /// A device advertising no callback bound gets the tail floor, which is
    /// shorter than the production ring's own figure, so the ring still
    /// governs: the null backend's bound is unchanged.
    #[test]
    fn an_unadvertised_callback_leaves_the_pre_empty_bound_on_the_ring() {
        let ring_capacity = RING_CAPACITY_FRAMES * usize::from(AudioOutput::CHANNELS);
        let device_tail = game_frames_in(device_tail_millis(None, 0));

        assert_eq!(
            max_drain_wait_frames(ring_capacity, device_tail),
            max_drain_wait_frames(ring_capacity, 0)
        );
    }

    /// [`MusicPlayer::start`] is public and takes any [`AudioOutput`], so a
    /// caller may have queued through [`AudioOutput::producer`] first. The
    /// ring's capacity is what `drained` compares against to call the ring
    /// empty, so it must come from the ring rather than from the free space
    /// left at construction -- otherwise those pre-queued samples set the
    /// bar low and their own tail could be dropped undelivered.
    #[test]
    fn a_pre_queued_producer_still_records_the_full_ring_capacity() {
        const RING_FRAMES: usize = 512;
        let full_ring = RING_FRAMES * usize::from(AudioOutput::CHANNELS);

        let output = AudioOutput::null(RING_FRAMES);
        let queued = output.producer().push(&[0.25; 64]);
        assert_eq!(queued, 64, "the null ring must accept this priming push");

        let player = MusicPlayer::start(short_song_without_its_own_reverb(), output)
            .expect("null backend never errors");

        assert_eq!(
            player.ring_capacity_for_test(),
            full_ring,
            "capacity must be the ring's own, not the free space a pre-queued producer left"
        );
    }

    #[test]
    fn a_songs_own_reverb_override_leaves_a_pending_tail() {
        let mut context = MusicContext::new();
        let song = short_song_without_its_own_reverb().with_reverb(100);
        assert_eq!(song.reverb_override(), Some(100));
        let output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut player = MusicPlayer::start_with_context(&mut context, song, output)
            .expect("null backend never errors");
        assert!(
            !first_playthrough_finishes_within(&mut player, REVERB_TAIL_PROBE_FRAMES),
            "an explicit reverb override must leave a tail pending well past the note's own end"
        );
    }

    #[test]
    fn failed_output_start_preserves_music_context() {
        let mut context = MusicContext::new();
        let priming_song = short_song_without_its_own_reverb().with_reverb(77);
        let priming_output = AudioOutput::null(RING_CAPACITY_FRAMES);
        MusicPlayer::start_with_context(&mut context, priming_song, priming_output)
            .expect("null backend never errors");
        assert_eq!(context.master_reverb, 77);

        let song = short_song_without_its_own_reverb().with_reverb(100);
        let output = AudioOutput::null(RING_CAPACITY_FRAMES);

        let result =
            MusicPlayer::start_with_context_and_starter(&mut context, song, output, |_| {
                Err(PlatformError::NoAudioDevice)
            });

        assert!(matches!(result, Err(PlatformError::NoAudioDevice)));
        assert_eq!(context.master_reverb, 77);
    }

    #[test]
    fn a_song_with_no_reverb_override_inherits_the_sessions_previous_level() {
        let mut context = MusicContext::new();
        let priming = short_song_without_its_own_reverb().with_reverb(100);
        let priming_output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut priming_player =
            MusicPlayer::start_with_context(&mut context, priming, priming_output)
                .expect("null backend never errors");
        assert!(!first_playthrough_finishes_within(
            &mut priming_player,
            REVERB_TAIL_PROBE_FRAMES
        ));

        let inheriting = short_song_without_its_own_reverb();
        assert_eq!(inheriting.reverb_override(), None);
        let inheriting_output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut inheriting_player =
            MusicPlayer::start_with_context(&mut context, inheriting, inheriting_output)
                .expect("null backend never errors");
        assert!(
            !first_playthrough_finishes_within(&mut inheriting_player, REVERB_TAIL_PROBE_FRAMES),
            "a header-less song must inherit the session's previously configured reverb level"
        );
    }

    #[test]
    fn an_explicit_zero_reverb_overrides_the_sessions_previous_level() {
        let mut context = MusicContext::new();
        let priming = short_song_without_its_own_reverb().with_reverb(100);
        let priming_output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut priming_player =
            MusicPlayer::start_with_context(&mut context, priming, priming_output)
                .expect("null backend never errors");
        assert!(!first_playthrough_finishes_within(
            &mut priming_player,
            REVERB_TAIL_PROBE_FRAMES
        ));

        let disabling = short_song_without_its_own_reverb().with_reverb(0);
        assert_eq!(disabling.reverb_override(), Some(0));
        let disabling_output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut disabling_player =
            MusicPlayer::start_with_context(&mut context, disabling, disabling_output)
                .expect("null backend never errors");
        assert!(
            first_playthrough_finishes_within(&mut disabling_player, REVERB_TAIL_PROBE_FRAMES),
            "an explicit reverb of 0 must disable the tail even though the session had one"
        );
    }

    #[test]
    fn an_inheriting_song_keeps_its_resolved_reverb_across_the_defensive_restart() {
        let mut context = MusicContext::new();
        let priming = short_song_without_its_own_reverb().with_reverb(100);
        let priming_output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut priming_player =
            MusicPlayer::start_with_context(&mut context, priming, priming_output)
                .expect("null backend never errors");
        assert!(!first_playthrough_finishes_within(
            &mut priming_player,
            REVERB_TAIL_PROBE_FRAMES
        ));

        let inheriting = short_song_without_its_own_reverb();
        assert_eq!(inheriting.reverb_override(), None);
        let output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut player = MusicPlayer::start_with_context(&mut context, inheriting, output)
            .expect("null backend never errors");

        let mut frames = 0;
        while !player.sequencer.is_finished() {
            player.advance_frame();
            frames += 1;
            assert!(
                frames < 5_000,
                "the inherited reverb tail must eventually drain"
            );
        }

        assert!(
            !first_playthrough_finishes_within(&mut player, REVERB_TAIL_PROBE_FRAMES),
            "the defensive restart must reuse the resolved reverb level, not the song header's"
        );
    }
}
