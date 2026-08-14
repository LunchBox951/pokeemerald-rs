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

use audio::{Sequencer, Song, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES};
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

/// Session-lived state a [`crate::App`] carries across separate
/// [`MusicPlayer::start_with_context`] calls -- currently just the master
/// reverb level the M4A driver keeps configured between songs.
///
/// Upstream never resets `gMPlayReverb`/`m4aSoundMode`'s reverb byte between
/// `m4aSongNumStart` calls; a song header only touches it when
/// `SongHeader::reverb`'s SET bit is set (`m4a_internal.h:12`..`:13`;
/// `m4a.c:661`..`:662`). A song whose header leaves that bit clear
/// ([`audio::Song::reverb_override`] returning `None`) must keep playing at
/// whatever level the previous song in the same session left configured,
/// not silently reset to `0`.
#[derive(Debug, Clone, Copy)]
pub struct MusicContext {
    /// The master reverb level the most recently started song resolved to,
    /// carried forward for the next song whose header leaves it unset.
    master_reverb: u8,
}

impl MusicContext {
    /// A fresh session with no reverb configured yet, matching the driver's
    /// own zeroed init state before any song has played.
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
/// For `speed == 4` that is 16 steps of 4/64 spaced 4 frames apart: the
/// first step lands on the fourth `FadeOutBody` call, so full volume covers
/// calls 1..=3, 60/64 covers 4..=7, ... 4/64 covers 60..=63, silent and
/// finished at call 64 (about 1.07 s at 59.7275 Hz).
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
/// being fed the already-faded dry signal -- audibly, a marginally *shorter*
/// tail: upstream's wet term carries the older, louder pre-fade volume back
/// around the loop (and keeps recirculating `pcmBuffer` even after `fadeOV`
/// hits zero), while this port scales the whole frame by the current volume
/// and goes exactly silent when the fade finishes.
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
    /// The reverb level [`Self::song`] resolved to at start -- either its own
    /// header override or, absent one, [`MusicContext`]'s carried-forward
    /// level. Kept so a defensive restart ([`Self::advance_frame`]) can
    /// rebuild the sequencer without losing that resolution back to `song`'s
    /// own possibly-`None` header value.
    reverb_level: u8,
}

impl MusicPlayer {
    /// Load `song_name` (e.g. `"mus_title"`) out of `pack`, open an audio
    /// device via `open_audio` (real hardware for [`crate::App::new`], the
    /// headless null backend for tests -- see [`crate::App::boot`]'s own
    /// opener-parameter precedent for `platform::Platform`), and start
    /// playing it.
    ///
    /// Starts a fresh, one-off [`MusicContext`]: the started song's reverb
    /// falls back to `0`, not a previous session's level, if its header
    /// leaves reverb unset. Use [`Self::start_from_pack_with_context`] to
    /// carry a level across songs in the same session.
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
        Self::start_from_pack_with_context(&mut MusicContext::new(), pack, song_name, open_audio)
    }

    /// As [`Self::start_from_pack`], resolving reverb inheritance against
    /// `context` -- see [`Self::start_with_context`].
    ///
    /// # Errors
    ///
    /// Propagates [`MusicError`] from resolving the song out of the pack, or
    /// from `open_audio`/[`Self::start_with_context`].
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

    /// Start playing an already-resolved `song` through `output`: prefills
    /// about half the ring, then starts the device.
    ///
    /// Starts a fresh, one-off [`MusicContext`]: the started song's reverb
    /// falls back to `0`, not a previous session's level, if its header
    /// leaves reverb unset. Use [`Self::start_with_context`] to carry a level
    /// across songs in the same session.
    ///
    /// # Errors
    ///
    /// [`PlatformError`] if `output` refuses to start (never happens for the
    /// null backend).
    pub fn start(song: Song, output: AudioOutput) -> Result<Self, PlatformError> {
        Self::start_with_context(&mut MusicContext::new(), song, output)
    }

    /// As [`Self::start`], resolving reverb inheritance against `context`:
    /// `song`'s own header override
    /// ([`audio::Song::reverb_override`]) wins if set, otherwise
    /// `context`'s carried-forward level is used, and `context` is updated to
    /// that resolved level for the next song started against it (module
    /// docs' [`MusicContext`]).
    ///
    /// # Errors
    ///
    /// [`PlatformError`] if `output` refuses to start (never happens for the
    /// null backend).
    pub fn start_with_context(
        context: &mut MusicContext,
        song: Song,
        mut output: AudioOutput,
    ) -> Result<Self, PlatformError> {
        let reverb_level = song.reverb_override().unwrap_or(context.master_reverb);
        context.master_reverb = reverb_level;
        let mut sequencer = Sequencer::with_resolved_reverb(
            song.clone(),
            DEFAULT_MASTER_VOLUME,
            DEFAULT_MAX_VOICES,
            reverb_level,
        );
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
            reverb_level,
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
            self.sequencer = Sequencer::with_resolved_reverb(
                self.song.clone(),
                DEFAULT_MASTER_VOLUME,
                DEFAULT_MAX_VOICES,
                self.reverb_level,
            );
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use audio::{Adsr, Event, Instrument, Song, ToneData, WaveData};
    use platform::AudioOutput;

    use super::{MusicContext, MusicPlayer, RING_CAPACITY_FRAMES};

    /// A short, loud one-shot note under no reverb override of its own
    /// (`Song::reverb_override` stays `None`), so any tail heard after it
    /// stops can only come from an inherited or explicitly-set level.
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

    /// How many frames [`first_playthrough_finishes_within`] gives a song:
    /// comfortably past the short note's own gate/wait window (which settles
    /// in a handful of ticks), but comfortably short of how long a `level:
    /// 100` reverb tail takes to drain (dozens of frames at minimum, per
    /// `crate::audio::reverb`'s own decay-rate tests).
    const SETTLE_BUDGET: usize = 25;

    /// Advances `player` one frame at a time, stopping the instant its
    /// sequencer reports finished, and reports whether that happened within
    /// `budget` frames.
    ///
    /// Deliberately stops driving the moment [`audio::Sequencer::is_finished`]
    /// first reports `true`, rather than running for a fixed window: past
    /// that point, [`MusicPlayer::advance_frame`]'s defensive restart would
    /// immediately re-trigger the note, and a longer window would then
    /// measure that recurring note-on blip instead of (or as well as) any
    /// reverb tail -- indistinguishable from the tail this helper exists to
    /// detect. Reading [`audio::Sequencer::is_finished`] directly (rather
    /// than sampling rendered audio for silence) sidesteps that confound
    /// entirely: a pending reverb tail holds it `false` no matter how quiet
    /// the tail's samples happen to be at any single instant.
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
            !first_playthrough_finishes_within(&mut player, SETTLE_BUDGET),
            "an explicit reverb override must leave a tail pending well past the note's own end"
        );
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
            SETTLE_BUDGET
        ));

        // The next song's header leaves reverb unset entirely; it must
        // inherit the level the first song resolved to, not reset to 0.
        let inheriting = short_song_without_its_own_reverb();
        assert_eq!(inheriting.reverb_override(), None);
        let inheriting_output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut inheriting_player =
            MusicPlayer::start_with_context(&mut context, inheriting, inheriting_output)
                .expect("null backend never errors");
        assert!(
            !first_playthrough_finishes_within(&mut inheriting_player, SETTLE_BUDGET),
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
            SETTLE_BUDGET
        ));

        // `Some(0)` is an explicit disable, distinct from "no override" --
        // it must NOT fall back to the session's carried-forward level.
        let disabling = short_song_without_its_own_reverb().with_reverb(0);
        assert_eq!(disabling.reverb_override(), Some(0));
        let disabling_output = AudioOutput::null(RING_CAPACITY_FRAMES);
        let mut disabling_player =
            MusicPlayer::start_with_context(&mut context, disabling, disabling_output)
                .expect("null backend never errors");
        assert!(
            first_playthrough_finishes_within(&mut disabling_player, SETTLE_BUDGET),
            "an explicit reverb of 0 must disable the tail even though the session had one"
        );
    }
}
