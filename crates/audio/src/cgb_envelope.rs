//! The CGB PSG envelope and stereo-panning helpers `CgbSound` drives through
//! `NRx2`/`NR51` hardware registers (`m4a.c:903`..`:925`).
//!
//! Unlike the DirectSound envelope ([`crate::envelope`]), which fades a
//! continuous `0..=255` gain, the four CGB channels share one hardware `s8`
//! envelope stepping a coarse `0..=15` level. This crate has no GB APU to
//! delegate that stepping to, so it is reproduced as its own per-render-frame
//! state machine holding whole-frame step counters rather than the
//! hardware's 1/64s envelope clock `(no-verbatim)` — the same deliberate,
//! benign simplification [`crate::psg::Sweep`] documents.

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
    #[must_use]
    pub fn new(adsr: CgbAdsr, goal: u8) -> Self {
        let sustain_goal = sustain_goal_of(goal, adsr.sustain);
        Self {
            adsr,
            goal,
            sustain_goal,
            phase: Phase::Attack,
            volume: 0,
            counter: 0,
            stop: false,
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
        if self.stop {
            // Release. `release == 0` silences the channel this same frame
            // (`m4a.c:1071` goto `envelope_pseudoecho_start`; with no
            // pseudo-echo volume that falls through to `oscillator_off`,
            // `:1102`). A non-zero release paces one level per frame down to
            // `0`, retiring on arrival.
            if self.adsr.release == 0 {
                self.silence();
            } else {
                self.release_step();
            }
            return;
        }
        match self.phase {
            Phase::Attack => self.attack_step(),
            Phase::Decay => self.decay_step(),
            // Sustain holds at `sustain_goal` until `note_off`.
            Phase::Sustain => {}
        }
    }

    /// Wait out `period` frames, then return `true` exactly on the frame a
    /// step should apply. Only called with a non-zero `period`; zero-period
    /// phases never pace — they transition instantly (see [`Self::step`]).
    fn counter_elapsed(&mut self, period: u8) -> bool {
        if self.counter > 0 {
            self.counter -= 1;
            return false;
        }
        self.counter = period;
        true
    }

    /// Attack phase. `attack == 0` skips the ramp entirely (`m4a.c:1039` goto
    /// `envelope_decay_start`); otherwise `+1` per `attack` frames toward
    /// `goal` (`m4a.c:1147`, `:1167`), entering decay once it arrives.
    fn attack_step(&mut self) {
        if self.adsr.attack == 0 {
            self.enter_decay();
            return;
        }
        if !self.counter_elapsed(self.adsr.attack) {
            return;
        }
        if self.volume < self.goal {
            self.volume += 1;
        }
        if self.volume >= self.goal {
            self.enter_decay();
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
        self.counter = 0;
    }

    /// Decay phase (only entered with a non-zero `decay`): `-1` per `decay`
    /// frames toward `sustain_goal` (`m4a.c:1120`, `:1142`), entering the
    /// sustain start once it arrives.
    fn decay_step(&mut self) {
        if !self.counter_elapsed(self.adsr.decay) {
            return;
        }
        if self.volume > self.sustain_goal {
            self.volume -= 1;
        }
        if self.volume <= self.sustain_goal {
            self.enter_sustain_start();
        }
    }

    /// `envelope_sustain_start` (`m4a.c:1125`): a zero `sustain` fraction ends
    /// the note through the pseudo-echo path (`:1129`), which with no
    /// pseudo-echo volume silences the channel; otherwise hold at
    /// `sustain_goal` (`envelope_sustain`, `:1113`).
    fn enter_sustain_start(&mut self) {
        if self.adsr.sustain == 0 {
            self.silence();
            return;
        }
        self.volume = self.sustain_goal;
        self.phase = Phase::Sustain;
        self.counter = 0;
    }

    /// The stop/release ramp: `-1` per `release` frames toward `0`, retiring
    /// the voice once it arrives (`m4a.c:1087`, then `envelope_pseudoecho_start`
    /// -> `oscillator_off`).
    fn release_step(&mut self) {
        if !self.counter_elapsed(self.adsr.release) {
            return;
        }
        if self.volume > 0 {
            self.volume -= 1;
        }
        if self.volume == 0 {
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
        let mut env = CgbEnvelope::new(
            CgbAdsr {
                attack: 2,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            4,
        );
        env.step(); // immediate first step: volume 1
        assert_eq!(env.volume(), 1);
        env.step(); // waiting
        assert_eq!(env.volume(), 1);
        env.step(); // waiting
        assert_eq!(env.volume(), 1);
        env.step(); // second step: volume 2
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
        // Regression guard: a non-zero release still paces one level per frame
        // to 0, retiring on arrival (m4a.c:1087, then envelope_pseudoecho_start
        // -> oscillator_off).
        let mut env = CgbEnvelope::new(
            CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: 15,
                release: 1,
            },
            4,
        );
        env.step();
        // sustain_goal = (4*15+15)>>4 = 4.
        assert_eq!(env.volume(), 4);
        env.note_off();
        env.step(); // first release step: one level down, not instant silence
        assert_eq!(env.volume(), 3, "non-zero release paces, not instant");
        assert!(env.is_active());
        for _ in 0..20 {
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
