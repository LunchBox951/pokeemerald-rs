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

use audio::{Sequencer, Song, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES};
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use audio::{Adsr, Event, Instrument, Song, ToneData, WaveData};
    use platform::{AudioOutput, PlatformError};

    use super::{MusicContext, MusicPlayer, RING_CAPACITY_FRAMES};

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
