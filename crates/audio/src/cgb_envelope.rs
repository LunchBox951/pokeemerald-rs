//! The CGB PSG envelope and stereo-panning helpers `CgbSound` drives through
//! `NRx2`/`NR51` hardware registers (`m4a.c:903`..`:925`).
//!
//! Unlike the DirectSound envelope ([`crate::envelope`]), which fades a
//! continuous `0..=255` gain, the four CGB channels share one hardware `s8`
//! envelope stepping a coarse `0..=15` level. This crate has no GB APU to
//! delegate that stepping to, so it is reproduced as its own whole-frame-step
//! state machine holding whole-frame step counters rather than the
//! hardware's 1/64s envelope clock `(no-verbatim)` — the same deliberate,
//! benign simplification [`crate::psg::Sweep`] documents. [`CgbEnvelope::step`]
//! is one such software iteration; it does not always land one-for-one with a
//! render frame — see [`CgbEnvelopeCadence`].

/// Mirrors upstream's shared `soundInfo->c15` counter (`m4a.c:941`..`:945`),
/// which paces a once-per-15-frames correction: the once-per-render-frame
/// [`CgbEnvelope::step`] alone runs at ~59.73 Hz, visibly slower than
/// hardware's true 1/64s (~63.71 Hz) envelope rate, so every 15th frame runs
/// a second iteration to keep up (`m4a.c:1173`..`:1180`, "every 15 frames,
/// envelope calculation has to be done twice to keep up with the hardware
/// envelope rate"). `14` single-iteration frames plus `1` double-iteration
/// frame gives 16 iterations per 15 frames, matching hardware's cadence over
/// that span.
///
/// One cadence is shared by all four CGB channels, driven from a single
/// per-render-frame call — upstream updates `soundInfo->c15` once per
/// `CgbSound` call, before its per-channel loop, and every channel reads the
/// same resulting value as `prevC15` (`m4a.c:941`..`:985`). Callers must
/// therefore own exactly one instance across the whole CGB channel set (see
/// [`crate::mixer::Mixer`]'s `cgb_envelope_cadence` field), not one per
/// voice — a per-voice clock would desync the correction frame between
/// channels started at different times, which [`CgbEnvelope::step_frame`]'s
/// contract does not allow for.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CgbEnvelopeCadence {
    /// Upstream's `soundInfo->c15`, `0..=14`.
    c15: u8,
}

impl CgbEnvelopeCadence {
    /// Advance by one render frame, returning whether this frame gets the
    /// extra iteration (`Self`'s doc). `m4a.c:941`..`:945`, `:984`,
    /// `:1177`..`:1180`.
    pub(crate) fn advance_frame(&mut self) -> bool {
        if self.c15 != 0 {
            self.c15 -= 1;
        } else {
            self.c15 = 14;
        }
        self.c15 == 0
    }
}

/// Attack/decay/sustain/release parameters from a CGB instrument's
/// `ToneData` (`m4a_internal.h:57`). `sustain` is a `0..=15` fraction of the
/// note's envelope goal (see [`CgbEnvelope::new`]), not an absolute level —
/// matching `chan->sustainGoal = (envelopeGoal * sustain + 15) >> 4`
/// (`CgbModVol`, `m4a.c:921`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CgbAdsr {
    /// Frames between each `+1` attack step.
    pub attack: u8,
    /// Frames between each `-1` decay step.
    pub decay: u8,
    /// Sustain level as a `0..=15` fraction of the envelope goal.
    pub sustain: u8,
    /// Frames between each `-1` release step, once stopped.
    pub release: u8,
}

impl CgbAdsr {
    /// A fixed-gain envelope: instant attack and release, full sustain.
    /// Useful for isolating channel mixing in tests.
    #[must_use]
    pub fn flat() -> Self {
        Self {
            attack: 0,
            decay: 0,
            sustain: 15,
            release: 0,
        }
    }
}

/// Which additive rule the envelope currently follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Attack,
    Decay,
    Sustain,
}

/// Live envelope state for one CGB voice.
#[derive(Clone, Debug)]
pub struct CgbEnvelope {
    adsr: CgbAdsr,
    /// The note's target volume (`envelopeGoal`, `0..=31` — see
    /// [`cgb_envelope_goal`]), computed once at note-on from the track's
    /// resolved stereo base volumes.
    goal: u8,
    sustain_goal: u8,
    phase: Phase,
    volume: u8,
    /// Frames remaining until the next step.
    counter: u8,
    stop: bool,
    /// In the pseudo-echo tail: volume is frozen at [`Self::echo_volume`]'s
    /// resolved floor and [`Self::echo_length`] counts down.
    echo: bool,
    /// Raw `xIECV` byte: the numerator `envelope_pseudoecho_start` scales the
    /// envelope goal by (`m4a.c:1091`). `0` disables the tail outright.
    echo_volume: u8,
    /// Raw `xIECL` byte: frames the tail holds before silencing.
    echo_length: u8,
    active: bool,
}

/// `(goal * sustain + 15) >> 4`, clamped into `u8` (`CgbModVol`,
/// `m4a.c:921`). Shared by [`CgbEnvelope::new`] and [`CgbEnvelope::set_goal`].
fn sustain_goal_of(goal: u8, sustain: u8) -> u8 {
    let raw = (u32::from(goal) * u32::from(sustain) + 15) >> 4;
    u8::try_from(raw.min(255)).unwrap_or(255)
}

impl CgbEnvelope {
    /// Begin a fresh note ramping toward `goal` (see [`cgb_envelope_goal`]).
    /// `echo_volume`/`echo_length` are the track's active pseudo-echo state
    /// (`xIECV`/`xIECL`), captured once at note-on like the DirectSound
    /// [`crate::envelope::Envelope`] (both `0` disables the tail).
    #[must_use]
    pub fn new(adsr: CgbAdsr, goal: u8, echo_volume: u8, echo_length: u8) -> Self {
        let sustain_goal = sustain_goal_of(goal, adsr.sustain);
        Self {
            adsr,
            goal,
            sustain_goal,
            phase: Phase::Attack,
            volume: 0,
            // Arm-on-entry: note-on sets `envelopeCounter = attack` (m4a.c:1031)
            // with `envelopeVolume = 0` (:1034), then the note-on frame renders
            // volume 0 and only decrements the counter via `goto
            // envelope_step_complete` (:1035, decrement at :1176) — no step
            // fires that frame. The first `+1` increment lands exactly `attack`
            // frames later. This crate's step machine is decrement-first
            // (fires the frame the counter hits 0, like `sustain_step`), so the
            // extra `+1` here reserves that note-on frame the driver renders at
            // volume 0 before the ramp begins. `attack == 0` (instant) ignores
            // the counter entirely (see [`Self::attack_step`]).
            counter: adsr.attack.saturating_add(1),
            stop: false,
            echo: false,
            echo_volume,
            echo_length,
            active: true,
        }
    }

    /// Current envelope volume (`0..=31`, though realistically `0..=15` for
    /// any hard-panned note — see [`cgb_envelope_goal`]).
    #[must_use]
    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// Whether the voice is still producing sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Whether the note has entered its release (stopped) state.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.stop
    }

    /// Request note-off: subsequent steps ramp down by `release` per step.
    pub fn note_off(&mut self) {
        self.stop = true;
        // The stop transition arms the release counter (`m4a.c:1063`,
        // `envelopeCounter = release`) so the ramp paces from a clean start
        // regardless of where the interrupted phase's counter stood — in
        // particular the live 7-frame counter a sustaining note carries. As at
        // note-on, the note-off frame itself renders the *held* level and only
        // decrements the counter (`goto envelope_step_complete`, m4a.c:1067);
        // the note holds its current volume for `release` frames before the
        // first `-1`. The `+1` reserves that held note-off frame for this
        // crate's decrement-first machine (matching [`Self::new`]). `release ==
        // 0` silences the same frame, ignoring the counter (see [`Self::step`]).
        self.counter = self.adsr.release.saturating_add(1);
    }

    /// Retire the voice immediately (e.g. a channel-1 sweep overflow, which
    /// silences the hardware channel outright — `CgbOscOff`, `m4a.c:857`).
    pub fn retire(&mut self) {
        self.active = false;
    }

    /// Re-run `CgbModVol`'s goal resolution against this live envelope from
    /// updated base volumes (`MPT_FLG_VOLCHG`): rewrites the envelope target
    /// and sustain level without disturbing the current volume or phase, so
    /// a mid-note `VOL`/`PAN` change bends the ongoing ramp rather than
    /// restarting it.
    pub fn set_goal(&mut self, adsr: CgbAdsr, goal: u8) {
        self.adsr = adsr;
        self.goal = goal;
        self.sustain_goal = sustain_goal_of(goal, adsr.sustain);
    }

    /// Advance the envelope by one render frame.
    ///
    /// A zero-period phase is *instant*: within this one call the volume lands
    /// on the phase's target and chains straight into the next phase, exactly
    /// as upstream's `goto` chain does (`m4a.c:1031`..`:1163`). Only a
    /// non-zero period paces a ramp one level per frame (`m4a.c:1147` attack,
    /// `:1120` decay).
    pub fn step(&mut self) {
        if !self.active {
            return;
        }
        if self.echo {
            self.echo_step();
            return;
        }
        if self.stop {
            // Release. `release == 0` reaches the pseudo-echo tail this same
            // frame (`m4a.c:1071` goto `envelope_pseudoecho_start`); a
            // non-zero release paces one level per frame down to `0` first,
            // then also reaches the tail (`:1087`). Either way the tail
            // itself silences outright when there is no echo floor (`:1102`).
            if self.adsr.release == 0 {
                self.enter_echo_or_silence();
            } else {
                self.release_step();
            }
            return;
        }
        match self.phase {
            Phase::Attack => self.attack_step(),
            Phase::Decay => self.decay_step(),
            Phase::Sustain => self.sustain_step(),
        }
    }

    /// Advance by one render frame, honoring [`CgbEnvelopeCadence`]'s extra
    /// iteration (`envelope_step_complete`'s `prevC15 == 0` re-entry into
    /// `envelope_step_repeat`, `m4a.c:1176`..`:1180`).
    ///
    /// Skipped whenever the first iteration this call already entered the
    /// pseudo-echo tail or retired the voice: those transitions, and an
    /// already-ongoing tail, all reach upstream via a `goto` that jumps past
    /// `envelope_step_complete`'s doubling check entirely
    /// (`m4a.c:1048`..`:1059`, `:1087`..`:1103`, `:1125`..`:1129`). Every
    /// other transition — note-on/note-off held frames included — falls
    /// through to that check normally and can be doubled.
    pub(crate) fn step_frame(&mut self, extra_iteration: bool) {
        self.step();
        if extra_iteration && self.active && !self.echo {
            self.step();
        }
    }

    /// Decrement this phase's frame counter and report whether it just reached
    /// zero — the frame a paced step fires. The counter is *armed on entry* to
    /// the phase ([`Self::new`], [`Self::enter_decay`], [`Self::note_off`]) and
    /// reloaded by the caller only while the phase continues, the same
    /// decrement-first `counter -= 1; fire at 0` shape [`Self::sustain_step`]
    /// uses. Upstream decrements `envelopeCounter` once per frame at
    /// `envelope_step_complete` (`m4a.c:1176`) and fires the next frame whose
    /// `envelope_step_repeat` sees it at zero (`m4a.c:1080`). Only ever called
    /// with a non-zero armed counter; zero-period phases transition instantly
    /// and never reach here (see [`Self::step`]).
    fn counter_reached_zero(&mut self) -> bool {
        self.counter -= 1;
        self.counter == 0
    }

    /// Attack phase. `attack == 0` skips the ramp entirely (`m4a.c:1039` goto
    /// `envelope_decay_start`); otherwise `+1` per `attack` frames toward
    /// `goal` (`m4a.c:1147`, `:1167`), entering decay once it arrives. The
    /// counter was armed on entry, so the first increment lands `attack` frames
    /// after note-on rather than on the first frame.
    fn attack_step(&mut self) {
        if self.adsr.attack == 0 {
            self.enter_decay();
            return;
        }
        if !self.counter_reached_zero() {
            return;
        }
        if self.volume < self.goal {
            self.volume += 1;
        }
        if self.volume >= self.goal {
            self.enter_decay();
        } else {
            self.counter = self.adsr.attack;
        }
    }

    /// `envelope_decay_start` (`m4a.c:1150`): a non-zero `decay` snaps the
    /// volume to `goal` (`:1156`) and paces the decay ramp; a zero `decay`
    /// skips the ramp and chains to the sustain start (`:1160`).
    fn enter_decay(&mut self) {
        if self.adsr.decay == 0 {
            self.enter_sustain_start();
            return;
        }
        self.volume = self.goal;
        self.phase = Phase::Decay;
        // Arm-on-entry: `envelope_decay_start` loads `envelopeCounter = decay`
        // (`m4a.c:1156`). The transition frame is consumed here, so the first
        // `-1` lands `decay` frames later — decrement-first from `decay`.
        self.counter = self.adsr.decay;
    }

    /// Decay phase (only entered with a non-zero `decay`): `-1` per `decay`
    /// frames toward `sustain_goal` (`m4a.c:1120`, `:1142`), entering the
    /// sustain start once it arrives.
    fn decay_step(&mut self) {
        if !self.counter_reached_zero() {
            return;
        }
        if self.volume > self.sustain_goal {
            self.volume -= 1;
        }
        if self.volume <= self.sustain_goal {
            self.enter_sustain_start();
        } else {
            self.counter = self.adsr.decay;
        }
    }

    /// `envelope_sustain_start` (`m4a.c:1125`): a zero `sustain` fraction ends
    /// the note through the pseudo-echo path (`:1129`), which with no
    /// pseudo-echo volume silences the channel; otherwise hold at
    /// `sustain_goal` (`envelope_sustain`, `:1113`).
    fn enter_sustain_start(&mut self) {
        if self.adsr.sustain == 0 {
            self.enter_echo_or_silence();
            return;
        }
        self.volume = self.sustain_goal;
        self.phase = Phase::Sustain;
        // `envelope_sustain` loads `envelopeCounter = 7` *after* landing on the
        // goal (`m4a.c:1108`..`:1114`) — the fixed sustain re-snap cadence, not
        // the instant-fire `0` the attack/decay enters use.
        self.counter = 7;
    }

    /// Sustain phase (`envelope_sustain`, `m4a.c:1112`..`:1114`): every 7 frames
    /// the counter elapses, upstream re-runs `CgbModVol`, and `envelopeVolume`
    /// snaps back to the freshly recomputed `sustainGoal` before the counter
    /// reloads with `7`. It is a *snap* to the live goal, not a gradual step.
    /// A mid-note `VOL`/`PAN` change or `MODT` tremolo reaches this envelope
    /// only through [`Self::set_goal`] rewriting `sustain_goal`, so this
    /// re-snap is what makes those changes audible — with the upstream-visible
    /// lag of up to 7 frames.
    fn sustain_step(&mut self) {
        self.counter -= 1;
        if self.counter == 0 {
            self.volume = self.sustain_goal;
            self.counter = 7;
        }
    }

    /// The stop/release ramp: `-1` per `release` frames toward `0`, then
    /// entering the pseudo-echo tail (or silencing outright with no echo
    /// floor) once it arrives (`m4a.c:1087`, `envelope_pseudoecho_start`).
    fn release_step(&mut self) {
        if !self.counter_reached_zero() {
            return;
        }
        if self.volume > 0 {
            self.volume -= 1;
        }
        if self.volume == 0 {
            self.enter_echo_or_silence();
        } else {
            self.counter = self.adsr.release;
        }
    }

    /// `envelope_pseudoecho_start` (`m4a.c:1090`..`:1102`): resolve the
    /// echo floor as `(goal * echo_volume + 0xFF) >> 8`; a `0` floor
    /// silences the channel outright (no tail), otherwise the tail holds at
    /// that floor for [`Self::echo_length`] frames (see [`Self::echo_step`]).
    /// Reused for both the paced release's arrival at `0` and the
    /// `release == 0` immediate case ([`Self::step`]).
    fn enter_echo_or_silence(&mut self) {
        let floor = cgb_echo_floor(self.goal, self.echo_volume);
        if floor == 0 {
            self.silence();
        } else {
            self.echo = true;
            self.volume = floor;
        }
    }

    /// Pseudo-echo tail: hold [`Self::volume`] at its frozen floor and count
    /// [`Self::echo_length`] down, retiring once it's exhausted. Unlike the
    /// DirectSound [`crate::envelope::Envelope`]'s unsigned tail
    /// (`m4a_1.s:_081DCFA0`, `subs`/`bhi`), the CGB side checks the
    /// post-decrement length as a *signed* byte — `pseudoEchoLength--; if
    /// ((s8)(pseudoEchoLength & 0xff) <= 0)` (`m4a.c:1050`..`:1051`) — so an
    /// `xIECL` of `129..=255` (post-decrement bit 7 set) retires the channel
    /// on the very first tail frame rather than holding for hundreds.
    fn echo_step(&mut self) {
        let post = self.echo_length.wrapping_sub(1);
        self.echo_length = post;
        if post == 0 || post >= 0x80 {
            self.active = false;
        }
    }

    /// Silence and retire the channel this frame (`oscillator_off`,
    /// `m4a.c:1053`).
    fn silence(&mut self) {
        self.volume = 0;
        self.active = false;
    }
}

/// `envelope_pseudoecho_start`'s floor volume: `(goal * echo_volume + 0xFF)
/// >> 8`, clamped into `u8` (`m4a.c:1091`). `goal` maxes at `31` (a centred
/// note, [`cgb_envelope_goal`]) and `echo_volume` at `255`, so the raw
/// product (`8160`) never approaches the clamp in practice — kept for
/// defensive symmetry with [`sustain_goal_of`].
fn cgb_echo_floor(goal: u8, echo_volume: u8) -> u8 {
    let raw = (u32::from(goal) * u32::from(echo_volume) + 0xFF) >> 8;
    u8::try_from(raw.min(255)).unwrap_or(255)
}

/// Which side(s) of the stereo field a CGB channel plays to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panning {
    Left,
    Right,
    Both,
}

/// Decide a channel's panning from its resolved stereo base volumes.
///
/// Behavioural port of `CgbPan` (`m4a.c:878`): a side is used exclusively
/// only when it is at least double the other; otherwise the note plays to
/// both sides (centred).
#[must_use]
pub fn cgb_pan(right: u8, left: u8) -> Panning {
    if right >= left {
        if right / 2 >= left {
            return Panning::Right;
        }
    } else if left / 2 >= right {
        return Panning::Left;
    }
    Panning::Both
}

/// Resolve a note's envelope target from its stereo base volumes and
/// panning.
///
/// Behavioural port of `CgbModVol` (`m4a.c:903`): a centred (both-sides)
/// note's goal is the *unclamped* sum of both base volumes over 16 — which
/// can reach `31`, genuinely louder than any hard-panned note, whose goal is
/// clamped to `15`. This is a real, if surprising, piece of hardware
/// behaviour, not an oversight.
#[must_use]
pub fn cgb_envelope_goal(right: u8, left: u8, panning: Panning) -> u8 {
    let sum = (u32::from(right) + u32::from(left)) / 16;
    if panning == Panning::Both {
        u8::try_from(sum).unwrap_or(u8::MAX)
    } else {
        u8::try_from(sum.min(15)).unwrap_or(15)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_zero_and_decay_zero_land_on_sustain_immediately() {
        // attack==0 & decay==0 chains the whole goto sequence in a single
        // frame: envelope_decay_start (decay==0) -> envelope_sustain_start ->
        // envelope_sustain, landing on sustainGoal (m4a.c:1040, :1160, :1113).
        // This test previously expected a one-level-per-frame ramp, which is
        // the bug being fixed: zero-period phases are instant, not paced.
        let mut env = CgbEnvelope::new(
            CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: 8,
                release: 0,
            },
            10,
            0,
            0,
        );
        env.step();
        // sustain_goal = (10*8+15)>>4 = 5, reached on the first frame.
        assert_eq!(env.volume(), 5, "should land on sustain_goal instantly");
        // ...and then hold there.
        for _ in 0..10 {
            env.step();
        }
        assert_eq!(env.volume(), 5);
    }

    #[test]
    fn attack_zero_with_nonzero_decay_starts_at_goal() {
        // attack==0 skips the ramp (m4a.c:1039 goto envelope_decay_start);
        // with a non-zero decay, envelopeVolume snaps to envelopeGoal on the
        // first frame (m4a.c:1156) before the decay ramp paces it down.
        let mut env = CgbEnvelope::new(
            CgbAdsr {
                attack: 0,
                decay: 3,
                sustain: 8,
                release: 0,
            },
            10,
            0,
            0,
        );
        env.step();
        assert_eq!(env.volume(), 10, "attack==0 lands on the goal, not ramped");
    }

    #[test]
    fn nonzero_decay_paces_down_to_sustain() {
        // Regression guard: a non-zero decay still steps one level per frame
        // toward sustainGoal (m4a.c:1120), not instantly.
        let mut env = CgbEnvelope::new(
            CgbAdsr {
                attack: 0,
                decay: 1,
                sustain: 8,
                release: 0,
            },
            10,
            0,
            0,
        );
        env.step(); // attack==0 -> at goal
        assert_eq!(env.volume(), 10);
        env.step(); // first decay step: one level down, not a jump to sustain
        assert_eq!(env.volume(), 9, "non-zero decay paces, not instant");
        // sustain_goal = 5; paced down and then held.
        for _ in 0..20 {
            env.step();
        }
        assert_eq!(env.volume(), 5);
    }

    #[test]
    fn attack_period_paces_the_ramp() {
        // Corrected cadence (was: an immediate first increment on frame 0 with
        // an `attack + 1` steady interval — the bug being fixed). Upstream arms
        // `envelopeCounter = attack` at note-on with `envelopeVolume = 0`
        // (m4a.c:1031, :1034); the note-on frame renders volume 0 and only
        // decrements the counter (:1035 goto envelope_step_complete, :1176), so
        // the FIRST +1 lands exactly `attack` frames after note-on and the
        // steady interval is exactly `attack` frames (:1147, :1167). Traced by
        // hand for attack == 2: frames 0,1 render 0; frame 2 -> 1; frames 3
        // renders 1; frame 4 -> 2.
        let mut env = CgbEnvelope::new(
            CgbAdsr {
                attack: 2,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            4,
            0,
            0,
        );
        env.step(); // frame 0 (note-on frame): still 0
        assert_eq!(env.volume(), 0);
        env.step(); // frame 1: still 0
        assert_eq!(env.volume(), 0);
        env.step(); // frame 2 == attack: first increment
        assert_eq!(env.volume(), 1);
        env.step(); // frame 3: holding between steps
        assert_eq!(env.volume(), 1);
        env.step(); // frame 4 == 2*attack: second increment
        assert_eq!(env.volume(), 2);
    }

    #[test]
    fn release_zero_silences_the_same_frame() {
        // release==0 ends the note the same frame note_off takes effect:
        // m4a.c:1071 goto envelope_pseudoecho_start; with no pseudo-echo volume
        // the channel reaches oscillator_off (m4a.c:1102). This test previously
        // expected a one-level-per-frame release ramp, which is the bug fixed.
        let mut env = CgbEnvelope::new(
            CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            8,
            0,
            0,
        );
        env.step();
        // sustain_goal = (8*15+15)>>4 = 8, reached instantly.
        assert_eq!(env.volume(), 8);
        assert!(env.is_active());
        env.note_off();
        env.step();
        assert!(!env.is_active(), "release==0 silences the same frame");
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn nonzero_release_ramps_to_zero_and_retires() {
        // Corrected cadence (was: an immediate `-1` on the first frame after
        // note-off — the shortened-tail bug being fixed). Note-off arms
        // `envelopeCounter = release` (m4a.c:1063) and the note-off frame
        // renders the *held* level, only decrementing the counter (:1067 goto
        // envelope_step_complete, :1176); the note holds for `release` frames
        // before the first `-1`, which then paces one level per frame to 0,
        // retiring on arrival (:1087 -> envelope_pseudoecho_start ->
        // oscillator_off). Traced for release == 1: the note-off frame holds 4,
        // then frame +1 -> 3, +2 -> 2, ... down to 0.
        let mut env = CgbEnvelope::new(
            CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: 15,
                release: 1,
            },
            4,
            0,
            0,
        );
        env.step();
        // sustain_goal = (4*15+15)>>4 = 4.
        assert_eq!(env.volume(), 4);
        env.note_off();
        env.step(); // note-off frame: holds the current level, no `-1` yet
        assert_eq!(env.volume(), 4, "release holds for `release` frames first");
        assert!(env.is_active());
        env.step(); // first paced `-1`
        assert_eq!(env.volume(), 3, "non-zero release paces, not instant");
        for _ in 0..20 {
            env.step();
        }
        assert!(!env.is_active());
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn sustain_re_snaps_live_goal_within_seven_frames() {
        // A held PSG note with instant attack+decay lands straight in sustain.
        // While sustaining, upstream re-runs CgbModVol and snaps envelopeVolume
        // to the recomputed sustainGoal every 7 frames (envelope_sustain reloads
        // envelopeCounter = 7, m4a.c:1112-1114). A mid-note VOL/PAN change routed
        // through set_goal must therefore reach the audible volume within 7
        // frames — never sooner (the counter has not elapsed), never later.
        let adsr = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 8,
            release: 0,
        };
        let mut env = CgbEnvelope::new(adsr, 10, 0, 0);
        env.step(); // lands on sustain_goal = (10*8+15)>>4 = 5, counter := 7
        assert_eq!(env.volume(), 5);

        // Live VOL bump mid-sustain: set_goal only rewrites goal/sustain_goal
        // (goal 10 -> 20 => sustain_goal = (20*8+15)>>4 = 10), not the counter.
        env.set_goal(adsr, 20);

        // The 7-frame counter has not elapsed, so the new goal must NOT reach
        // the volume yet — this pins the deliberate upstream-visible lag.
        for frame in 1..7 {
            env.step();
            assert_eq!(
                env.volume(),
                5,
                "sustain must not re-snap before the counter elapses (frame {frame})"
            );
        }
        // On the 7th frame the counter reaches 0 and the volume snaps to the
        // recomputed sustain_goal.
        env.step();
        assert_eq!(
            env.volume(),
            10,
            "sustain re-snaps to the live sustain_goal on the 7th frame"
        );
    }

    // --- CGB pseudo-echo tail (`xIECV`/`xIECL`) -----------------------------

    #[test]
    fn cgb_echo_floor_pins_the_scaled_formula() {
        // `(goal * echo_volume + 0xFF) >> 8` (`m4a.c:1091`).
        assert_eq!(cgb_echo_floor(10, 128), 5); // (1280+255)>>8 = 5
        assert_eq!(cgb_echo_floor(0, 255), 0);
        assert_eq!(cgb_echo_floor(31, 255), 31); // (7905+255)>>8 = 31
        assert_eq!(cgb_echo_floor(10, 0), 0); // no xIECV -> no floor
    }

    #[test]
    fn nonzero_release_pseudo_echo_holds_then_retires() {
        // A paced release that reaches 0 with a nonzero echo floor enters the
        // tail instead of retiring outright, then holds for `echo_length`
        // frames before finally silencing -- reusing the DirectSound
        // envelope's decrement-then-check tail shape.
        let adsr = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 15,
            release: 1,
        };
        let mut env = CgbEnvelope::new(adsr, 20, 128, 3);
        env.step(); // sustain_goal = (20*15+15)>>4 = 19
        assert_eq!(env.volume(), 19);
        env.note_off();
        env.step(); // note-off frame: holds, no `-1` yet (release paces from 1)
        assert_eq!(env.volume(), 19);
        // Paced release: 19 -> 0 over 19 further steps (release period 1).
        for _ in 0..19 {
            env.step();
        }
        // floor = (20*128+255)>>8 = 10
        assert_eq!(
            env.volume(),
            10,
            "release arrival must land on the echo floor"
        );
        assert!(
            env.is_active(),
            "the tail must hold, not retire immediately"
        );
        env.step(); // echo_length 3 -> 2
        assert!(env.is_active());
        assert_eq!(env.volume(), 10, "volume stays frozen through the tail");
        env.step(); // -> 1
        assert!(env.is_active());
        env.step(); // old==1 < 2 -> retire
        assert!(!env.is_active());
    }

    #[test]
    fn release_zero_with_pseudo_echo_holds_instead_of_silencing() {
        // With echo configured, even an instant (release == 0) note-off must
        // land in the tail rather than silencing on the same frame -- unlike
        // `release_zero_silences_the_same_frame`, which pins the no-echo case.
        let adsr = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 15,
            release: 0,
        };
        let mut env = CgbEnvelope::new(adsr, 8, 128, 2);
        env.step();
        assert_eq!(env.volume(), 8);
        env.note_off();
        env.step();
        // floor = (8*128+255)>>8 = 4
        assert_eq!(env.volume(), 4);
        assert!(
            env.is_active(),
            "an echo floor must hold, not silence outright"
        );
        env.step(); // echo_length 2 -> 1
        assert!(env.is_active());
        env.step(); // old==1 < 2 -> retire
        assert!(!env.is_active());
    }

    #[test]
    fn zero_sustain_with_pseudo_echo_enters_the_tail() {
        // `envelope_sustain_start` with `sustain == 0` routes through
        // `envelope_pseudoecho_start` (`m4a.c:1129`), so a configured echo
        // floor must hold there instead of the note vanishing on arrival —
        // same tail contract the release paths pin above.
        let adsr = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
        };
        let mut env = CgbEnvelope::new(adsr, 8, 128, 2);
        env.step(); // instant attack+decay -> sustain start with sustain == 0
                    // floor = (8*128+255)>>8 = 4
        assert_eq!(env.volume(), 4, "zero sustain must land on the echo floor");
        assert!(
            env.is_active(),
            "an echo floor must hold, not silence outright"
        );
        env.step(); // echo_length 2 -> 1
        assert!(env.is_active());
        env.step(); // old==1 < 2 -> retire
        assert!(!env.is_active());
    }

    #[test]
    fn zero_sustain_with_no_echo_still_silences_at_sustain_start() {
        // The no-echo (xIECV unset) zero-sustain note keeps its original
        // fate: it retires the frame decay lands on the sustain start.
        let adsr = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
        };
        let mut env = CgbEnvelope::new(adsr, 8, 0, 0);
        env.step();
        assert!(!env.is_active());
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn echo_length_signed_boundary_128_holds_129_retires_at_once() {
        // The CGB tail checks the post-decrement length as a *signed* byte
        // (`(s8)(pseudoEchoLength & 0xff) <= 0`, m4a.c:1050..:1051): 128 is
        // the longest tail (post-decrement 127), while 129 lands on
        // post-decrement 128 (s8 -128) and retires on the first tail frame.
        let adsr = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 15,
            release: 0,
        };
        let mut env = CgbEnvelope::new(adsr, 8, 128, 128);
        env.step();
        env.note_off();
        env.step(); // lands on the echo floor
        assert!(env.is_active());
        // 127 further frames tick post-decrement 127..=1 down, all held.
        for frame in 0..127 {
            env.step();
            assert!(env.is_active(), "tail must hold (frame {frame})");
        }
        env.step(); // post-decrement 0 -> retire
        assert!(!env.is_active(), "128-frame tail exhausts");

        let mut env = CgbEnvelope::new(adsr, 8, 128, 129);
        env.step();
        env.note_off();
        env.step(); // lands on the echo floor
        assert!(env.is_active());
        env.step(); // post-decrement 128: signed-negative -> retire at once
        assert!(
            !env.is_active(),
            "xIECL 129 must retire on the first tail frame, not hold ~130 frames"
        );
    }

    #[test]
    fn zero_echo_volume_still_silences_immediately_after_release() {
        // The default (no xIECV) must reproduce the pre-echo behaviour
        // exactly: a paced release reaching 0 retires outright.
        let adsr = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 15,
            release: 1,
        };
        let mut env = CgbEnvelope::new(adsr, 4, 0, 0);
        env.step();
        env.note_off();
        for _ in 0..6 {
            env.step();
        }
        assert!(!env.is_active());
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn cgb_pan_hard_pans_when_one_side_dominates() {
        assert_eq!(cgb_pan(200, 50), Panning::Right); // 200/2=100 >= 50
        assert_eq!(cgb_pan(50, 200), Panning::Left);
        assert_eq!(cgb_pan(100, 80), Panning::Both); // 100/2=50 < 80
                                                     // A silent note still resolves to a side under the real formula
                                                     // (0/2 >= 0): harmless in practice since nothing is audible either
                                                     // way, but faithfully reproduced rather than special-cased.
        assert_eq!(cgb_pan(0, 0), Panning::Right);
    }

    #[test]
    fn centred_goal_can_exceed_a_hard_panned_goal() {
        // Same total energy, but centred sums unclamped while panned clamps.
        let centred = cgb_envelope_goal(255, 255, Panning::Both);
        let panned = cgb_envelope_goal(255, 0, Panning::Right);
        assert_eq!(centred, 31); // (255+255)/16 = 31, unclamped
        assert_eq!(panned, 15); // (255+0)/16 = 15, already within range
        assert!(centred > panned);
    }
}

/// [`CgbEnvelopeCadence`]/[`CgbEnvelope::step_frame`]'s own tests, kept out
/// of [`tests`] for the same per-file-size reason `mixer.rs` splits off
/// `mixer_priority.rs`/`mixer_mixing.rs` (issue #453).
#[cfg(test)]
#[path = "cgb_envelope_cadence.rs"]
mod cadence_tests;
