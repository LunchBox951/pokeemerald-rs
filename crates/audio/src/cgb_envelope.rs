//! Per-frame CGB PSG envelope and stereo-panning state.
//!
//! The native mixer models the hardware's coarse 16-level envelope with
//! whole-frame step counters rather than the hardware's 1/64s envelope clock
//! `(no-verbatim)` — the same deliberate, benign simplification
//! [`crate::psg::Sweep`] documents, since it has no GB APU envelope clock to
//! delegate to. [`CgbEnvelope::step`] is one such software iteration; it does
//! not always land one-for-one with a render frame — see
//! [`CgbEnvelopeCadence`].

/// Mirrors upstream's shared `soundInfo->c15` counter (`m4a.c:941`..`:945`),
/// which paces a once-per-15-frames correction: one [`CgbEnvelope::step`] per
/// render frame alone runs at ~59.73 Hz, visibly slower than
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

const CGB_ENVELOPE_LEVELS: u32 = 16;
const CGB_ENVELOPE_LEVEL_MAX: u8 = 15;
const PSEUDO_ECHO_SCALE: u32 = 256;
const SUSTAIN_REFRESH_FRAMES: u8 = 7;
const HARD_PAN_RATIO: u16 = 2;

/// CGB attack, decay, sustain, and release parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CgbAdsr {
    /// Frames between attack increments; zero skips attack.
    pub attack: u8,
    /// Frames between decay decrements; zero skips decay.
    pub decay: u8,
    /// Sustain level as one of 16 fractions of the current goal; zero ends it.
    pub sustain: u8,
    /// Frames between release decrements; zero ends release immediately.
    pub release: u8,
}

impl CgbAdsr {
    /// Return an instant, full-sustain envelope.
    #[must_use]
    pub fn flat() -> Self {
        Self {
            attack: 0,
            decay: 0,
            sustain: CGB_ENVELOPE_LEVEL_MAX,
            release: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Attack,
    Decay,
    Sustain,
    Release,
    PseudoEcho,
    Retired,
}

/// Live envelope state for one CGB voice.
#[derive(Clone, Debug)]
pub struct CgbEnvelope {
    adsr: CgbAdsr,
    goal: u8,
    sustain_goal: u8,
    phase: Phase,
    volume: u8,
    frames_until_step: u8,
    note_off_requested: bool,
    echo_volume: u8,
    echo_length: u8,
}

fn sustain_goal_of(goal: u8, sustain: u8) -> u8 {
    let scaled = u32::from(goal) * u32::from(sustain);
    let rounded_up = scaled.div_ceil(CGB_ENVELOPE_LEVELS);
    u8::try_from(rounded_up.min(u32::from(u8::MAX))).unwrap_or(u8::MAX)
}

fn transition_frame_delay(period: u8) -> u8 {
    // CgbSound renders the note-on or note-off frame before decrementing its
    // counter (m4a.c:1031..1035, 1063..1067, 1176). This state machine checks
    // before rendering, so one extra frame preserves that cadence.
    period.saturating_add(1)
}

impl CgbEnvelope {
    /// Begin a note with its resolved volume goal and pseudo-echo settings.
    /// An `echo_volume` of zero disables the tail.
    #[must_use]
    pub fn new(adsr: CgbAdsr, goal: u8, echo_volume: u8, echo_length: u8) -> Self {
        let sustain_goal = sustain_goal_of(goal, adsr.sustain);
        Self {
            adsr,
            goal,
            sustain_goal,
            phase: Phase::Attack,
            volume: 0,
            frames_until_step: transition_frame_delay(adsr.attack),
            note_off_requested: false,
            echo_volume,
            echo_length,
        }
    }

    /// Return the current envelope volume.
    #[must_use]
    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// Return whether the voice can still produce sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.phase != Phase::Retired
    }

    /// Return whether note-off has been requested.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.note_off_requested
    }

    /// Enter the release phase.
    pub fn note_off(&mut self) {
        self.note_off_requested = true;
        if !matches!(self.phase, Phase::PseudoEcho | Phase::Retired) {
            self.phase = Phase::Release;
            self.frames_until_step = transition_frame_delay(self.adsr.release);
        }
    }

    /// Retire the voice immediately.
    pub fn retire(&mut self) {
        self.phase = Phase::Retired;
    }

    /// Update a live note's volume and sustain goals without restarting it.
    pub fn set_goal(&mut self, adsr: CgbAdsr, goal: u8) {
        self.adsr = adsr;
        self.goal = goal;
        self.sustain_goal = sustain_goal_of(goal, adsr.sustain);
    }

    /// Advance the envelope by one software iteration. A render frame is one
    /// or two iterations, so production rendering drives the envelope through
    /// `step_frame` rather than calling this directly (module docs).
    pub fn step(&mut self) {
        match self.phase {
            Phase::Attack => self.attack_step(),
            Phase::Decay => self.decay_step(),
            Phase::Sustain => self.sustain_step(),
            Phase::Release if self.adsr.release == 0 => self.enter_pseudo_echo_or_silence(),
            Phase::Release => self.release_step(),
            Phase::PseudoEcho => self.pseudo_echo_step(),
            Phase::Retired => {}
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
        if extra_iteration && !matches!(self.phase, Phase::PseudoEcho | Phase::Retired) {
            self.step();
        }
    }

    /// Decrement this phase's frame counter and report whether it just reached
    /// zero — the frame a paced step fires. The counter is *armed on entry* to
    /// the phase ([`Self::new`], [`Self::enter_decay`], [`Self::note_off`]) and
    /// reloaded by the caller only while the phase continues, the same
    /// decrement-first `frames_until_step -= 1; fire at 0` shape
    /// [`Self::sustain_step`] uses. Upstream decrements `envelopeCounter` once
    /// per frame at `envelope_step_complete` (`m4a.c:1176`) and fires the next
    /// frame whose `envelope_step_repeat` sees it at zero (`m4a.c:1080`). Only
    /// ever called with a non-zero armed counter; zero-period phases
    /// transition instantly and never reach here (see [`Self::step`]).
    fn paced_step_is_due(&mut self) -> bool {
        self.frames_until_step -= 1;
        self.frames_until_step == 0
    }

    fn attack_step(&mut self) {
        if self.adsr.attack == 0 {
            self.enter_decay();
            return;
        }
        if !self.paced_step_is_due() {
            return;
        }
        if self.volume < self.goal {
            self.volume += 1;
        }
        if self.volume >= self.goal {
            self.enter_decay();
        } else {
            self.frames_until_step = self.adsr.attack;
        }
    }

    fn enter_decay(&mut self) {
        if self.adsr.decay == 0 {
            self.enter_sustain_start();
            return;
        }
        self.volume = self.goal;
        self.phase = Phase::Decay;
        self.frames_until_step = self.adsr.decay;
    }

    fn decay_step(&mut self) {
        if !self.paced_step_is_due() {
            return;
        }
        if self.volume > self.sustain_goal {
            self.volume -= 1;
        }
        if self.volume <= self.sustain_goal {
            self.enter_sustain_start();
        } else {
            self.frames_until_step = self.adsr.decay;
        }
    }

    fn enter_sustain_start(&mut self) {
        if self.adsr.sustain == 0 {
            self.enter_pseudo_echo_or_silence();
            return;
        }
        self.volume = self.sustain_goal;
        self.phase = Phase::Sustain;
        self.frames_until_step = SUSTAIN_REFRESH_FRAMES;
    }

    fn sustain_step(&mut self) {
        if self.paced_step_is_due() {
            self.volume = self.sustain_goal;
            self.frames_until_step = SUSTAIN_REFRESH_FRAMES;
        }
    }

    fn release_step(&mut self) {
        if !self.paced_step_is_due() {
            return;
        }
        if self.volume > 0 {
            self.volume -= 1;
        }
        if self.volume == 0 {
            self.enter_pseudo_echo_or_silence();
        } else {
            self.frames_until_step = self.adsr.release;
        }
    }

    fn enter_pseudo_echo_or_silence(&mut self) {
        let floor = cgb_echo_floor(self.goal, self.echo_volume);
        if floor == 0 {
            self.silence();
        } else {
            self.phase = Phase::PseudoEcho;
            self.volume = floor;
        }
    }

    fn pseudo_echo_step(&mut self) {
        self.echo_length = self.echo_length.wrapping_sub(1);
        // CgbSound tests the decremented xIECL counter as an i8
        // (m4a.c:1050..1051), so values with the high bit set are exhausted.
        if i8::from_ne_bytes([self.echo_length]) <= 0 {
            self.phase = Phase::Retired;
        }
    }

    fn silence(&mut self) {
        self.volume = 0;
        self.phase = Phase::Retired;
    }
}

fn cgb_echo_floor(goal: u8, echo_volume: u8) -> u8 {
    let scaled = u32::from(goal) * u32::from(echo_volume);
    u8::try_from(scaled.div_ceil(PSEUDO_ECHO_SCALE)).unwrap_or(u8::MAX)
}

/// Which side(s) of the stereo field a CGB channel plays to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panning {
    Left,
    Right,
    Both,
}

fn volume_dominates(dominant: u8, other: u8) -> bool {
    u16::from(dominant) >= u16::from(other) * HARD_PAN_RATIO
}

/// Hard-pan when one side is at least twice as loud; otherwise use both sides.
/// Two silent sides resolve to [`Panning::Right`].
#[must_use]
pub fn cgb_pan(right: u8, left: u8) -> Panning {
    if right >= left && volume_dominates(right, left) {
        Panning::Right
    } else if volume_dominates(left, right) {
        Panning::Left
    } else {
        Panning::Both
    }
}

/// Resolve an envelope goal from the stereo base volumes and panning.
///
/// Centred goals keep the full combined volume and can reach 31. Hard-panned
/// goals stop at 15 (`CgbModVol`, `m4a.c:903..921`).
#[must_use]
pub fn cgb_envelope_goal(right: u8, left: u8, panning: Panning) -> u8 {
    let combined_goal = (u32::from(right) + u32::from(left)) / CGB_ENVELOPE_LEVELS;
    match panning {
        Panning::Both => u8::try_from(combined_goal).unwrap_or(u8::MAX),
        Panning::Left | Panning::Right => {
            u8::try_from(combined_goal.min(u32::from(CGB_ENVELOPE_LEVEL_MAX)))
                .unwrap_or(CGB_ENVELOPE_LEVEL_MAX)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adsr(attack: u8, decay: u8, sustain: u8, release: u8) -> CgbAdsr {
        CgbAdsr {
            attack,
            decay,
            sustain,
            release,
        }
    }

    fn plain_envelope(adsr: CgbAdsr, goal: u8) -> CgbEnvelope {
        CgbEnvelope::new(adsr, goal, 0, 0)
    }

    fn step_volumes<const N: usize>(envelope: &mut CgbEnvelope) -> [u8; N] {
        std::array::from_fn(|_| {
            envelope.step();
            envelope.volume()
        })
    }

    fn step_frames(envelope: &mut CgbEnvelope, frames: usize) {
        for _ in 0..frames {
            envelope.step();
        }
    }

    #[test]
    fn attack_zero_and_decay_zero_land_on_sustain_immediately() {
        let mut env = plain_envelope(adsr(0, 0, 8, 0), 10);

        env.step();

        assert_eq!(env.volume(), 5);
        assert_eq!(step_volumes::<10>(&mut env), [5; 10]);
    }

    #[test]
    fn attack_zero_with_nonzero_decay_starts_at_goal() {
        let mut env = plain_envelope(adsr(0, 3, 8, 0), 10);

        env.step();

        assert_eq!(env.volume(), 10);
    }

    #[test]
    fn nonzero_decay_paces_down_to_sustain() {
        let mut env = plain_envelope(adsr(0, 1, 8, 0), 10);

        assert_eq!(step_volumes::<7>(&mut env), [10, 9, 8, 7, 6, 5, 5]);
    }

    #[test]
    fn attack_period_paces_the_ramp() {
        let mut env = plain_envelope(adsr(2, 0, 15, 0), 4);

        assert_eq!(step_volumes::<5>(&mut env), [0, 0, 1, 1, 2]);
    }

    #[test]
    fn release_zero_silences_the_same_frame() {
        let mut env = plain_envelope(adsr(0, 0, 15, 0), 8);
        env.step();
        assert_eq!(env.volume(), 8);
        assert!(env.is_active());

        env.note_off();
        env.step();

        assert!(!env.is_active());
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn nonzero_release_ramps_to_zero_and_retires() {
        let mut env = plain_envelope(adsr(0, 0, 15, 1), 4);
        env.step();
        assert_eq!(env.volume(), 4);

        env.note_off();

        assert_eq!(step_volumes::<5>(&mut env), [4, 3, 2, 1, 0]);
        assert!(!env.is_active());
    }

    #[test]
    fn sustain_re_snaps_live_goal_within_seven_frames() {
        let adsr = adsr(0, 0, 8, 0);
        let mut env = plain_envelope(adsr, 10);
        env.step();
        assert_eq!(env.volume(), 5);

        env.set_goal(adsr, 20);

        assert_eq!(step_volumes::<7>(&mut env), [5, 5, 5, 5, 5, 5, 10]);
    }

    #[test]
    fn cgb_echo_floor_rounds_up_by_byte_scale() {
        assert_eq!(cgb_echo_floor(10, 128), 5);
        assert_eq!(cgb_echo_floor(0, 255), 0);
        assert_eq!(cgb_echo_floor(31, 255), 31);
        assert_eq!(cgb_echo_floor(10, 0), 0);
    }

    #[test]
    fn nonzero_release_pseudo_echo_holds_then_retires() {
        let mut env = CgbEnvelope::new(adsr(0, 0, 15, 1), 20, 128, 3);
        env.step();
        assert_eq!(env.volume(), 19);

        env.note_off();

        assert_eq!(
            step_volumes::<20>(&mut env),
            [19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 10]
        );
        assert!(env.is_active());
        assert_eq!(step_volumes::<2>(&mut env), [10, 10]);
        assert!(env.is_active());
        env.step();
        assert!(!env.is_active());
    }

    #[test]
    fn release_zero_with_pseudo_echo_holds_instead_of_silencing() {
        let mut env = CgbEnvelope::new(adsr(0, 0, 15, 0), 8, 128, 2);
        env.step();
        assert_eq!(env.volume(), 8);

        env.note_off();
        env.step();
        assert_eq!(env.volume(), 4);
        assert!(env.is_active());

        env.step();
        assert_eq!(env.volume(), 4);
        assert!(env.is_active());

        env.step();
        assert!(!env.is_active());
    }

    #[test]
    fn zero_sustain_with_pseudo_echo_enters_the_tail() {
        let mut env = CgbEnvelope::new(adsr(0, 0, 0, 0), 8, 128, 2);

        env.step();

        assert_eq!(env.volume(), 4);
        assert!(env.is_active());
        assert_eq!(step_volumes::<1>(&mut env), [4]);
        assert!(env.is_active());
        env.step();
        assert!(!env.is_active());
    }

    #[test]
    fn zero_sustain_with_no_echo_still_silences_at_sustain_start() {
        let mut env = plain_envelope(adsr(0, 0, 0, 0), 8);

        env.step();

        assert!(!env.is_active());
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn echo_length_signed_boundary_128_holds_129_retires_at_once() {
        let adsr = adsr(0, 0, 15, 0);
        let mut longest_tail = CgbEnvelope::new(adsr, 8, 128, 128);
        longest_tail.step();
        longest_tail.note_off();
        longest_tail.step();
        assert!(longest_tail.is_active());

        for _ in 0..127 {
            longest_tail.step();
            assert!(longest_tail.is_active());
        }
        longest_tail.step();
        assert!(!longest_tail.is_active());

        let mut signed_negative_tail = CgbEnvelope::new(adsr, 8, 128, 129);
        signed_negative_tail.step();
        signed_negative_tail.note_off();
        signed_negative_tail.step();
        assert!(signed_negative_tail.is_active());

        signed_negative_tail.step();

        assert!(!signed_negative_tail.is_active());
    }

    #[test]
    fn zero_echo_volume_silences_immediately_after_release() {
        let mut env = plain_envelope(adsr(0, 0, 15, 1), 4);
        env.step();
        env.note_off();

        step_frames(&mut env, 6);

        assert!(!env.is_active());
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn cgb_pan_hard_pans_a_dominant_side_and_biases_a_silent_tie_right() {
        assert_eq!(cgb_pan(200, 50), Panning::Right);
        assert_eq!(cgb_pan(50, 200), Panning::Left);
        assert_eq!(cgb_pan(100, 80), Panning::Both);
        assert_eq!(cgb_pan(0, 0), Panning::Right);
    }

    #[test]
    fn centred_goal_can_exceed_a_hard_panned_goal() {
        let centred = cgb_envelope_goal(255, 255, Panning::Both);
        let panned = cgb_envelope_goal(255, 0, Panning::Right);

        assert_eq!(centred, 31);
        assert_eq!(panned, 15);
        assert!(centred > panned);
    }
}

/// [`CgbEnvelopeCadence`]/[`CgbEnvelope::step_frame`]'s own tests, kept out
/// of [`tests`] for the same per-file-size reason `mixer.rs` splits off
/// `mixer_priority.rs`/`mixer_mixing.rs` (issue #453).
#[cfg(test)]
#[path = "cgb_envelope_cadence.rs"]
mod cadence_tests;
