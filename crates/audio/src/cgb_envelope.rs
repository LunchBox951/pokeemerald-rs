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

/// The nibble upstream masks its step-time-and-direction field down to before
/// adding the volume nibble and writing the pair as one NRx2 byte
/// (`m4a.c:1221`..`:1222`). Everything a phase's raw envelope byte sets above
/// bit 3 is discarded before hardware sees it.
const NRX2_ENV_FIELD_MASK: u8 = 0x0F;
/// NRx2 bits 0-2, the envelope step time
/// (`GBAudioRegisterSweep`, `mgba/include/mgba/internal/gb/audio.h:24`).
const NRX2_ENV_STEP_TIME_MASK: u8 = 0x07;
/// NRx2 bit 3, set for an upward envelope: upstream's `CGB_NRx2_ENV_DIR_INC`
/// (`m4a_internal.h:85`, whose `CGB_NRx2_ENV_DIR_DEC` counterpart is zero at
/// `:84`) and mGBA's direction bit
/// (`mgba/include/mgba/internal/gb/audio.h:25`).
const NRX2_ENV_DIR_INC: u8 = 0x08;

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
    /// Awaiting its first `CgbSound` pass; not yet running the attack.
    Starting,
    Attack,
    Decay,
    Sustain,
    Release,
    PseudoEcho,
    Retired,
}

/// How a live square or noise channel's hardware envelope paces itself
/// between `CGB_CHANNEL_MO_VOL` writes: the NRx2 step time it reloads
/// after each step, and NRx2's direction bit
/// (`_updateEnvelope`, `mgba/src/gb/audio.c:931-945`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HardwareEnvelopePacing {
    /// Envelope iterations between hardware steps: NRx2 bits 0-2, so `1..=7`
    /// and never zero (a zero step time is a dead envelope, not a paced one).
    pub(crate) step_time: u8,
    /// Whether the envelope counts up (`CGB_NRx2_ENV_DIR_INC`) rather than down.
    pub(crate) increasing: bool,
}

impl HardwareEnvelopePacing {
    /// Decode the pacing an NRx2 write programs from the step-time-and-direction
    /// value upstream hands it: bits 0-2 are the step time and bit 3 the
    /// direction, everything above them masked away first
    /// (`m4a.c:1221`..`:1222`, `_writeEnvelope`, `mgba/src/gb/audio.c:891`..`:893`).
    ///
    /// A zero step time is a dead envelope rather than a slow one, whatever
    /// bit 3 says (`_updateEnvelopeDead`, `mgba/src/gb/audio.c:948`..`:950`),
    /// so it decodes to `None`. A phase byte above 7 therefore reaches
    /// hardware as some *other* period and possibly the opposite direction:
    /// an attack of 8 writes `(8 + CGB_NRx2_ENV_DIR_INC) & 0xF == 0`, a dead
    /// downward envelope, not an upward step every 8 iterations.
    fn from_nrx2_step_time_and_dir(step_time_and_dir: u8) -> Option<Self> {
        let field = step_time_and_dir & NRX2_ENV_FIELD_MASK;
        let step_time = field & NRX2_ENV_STEP_TIME_MASK;
        (step_time != 0).then_some(Self {
            step_time,
            increasing: field & NRX2_ENV_DIR_INC != 0,
        })
    }
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
            phase: Phase::Starting,
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

    /// How the hardware envelope paces itself from its last
    /// `CGB_CHANNEL_MO_VOL` write, or `None` when that write left it dead
    /// and holding, decoded from the exact value upstream hands NRx2
    /// ([`HardwareEnvelopePacing::from_nrx2_step_time_and_dir`]). Attack,
    /// decay, and release each combine their own envelope byte with a
    /// direction bit; sustain-start and pseudo-echo-start write a bare
    /// direction bit and no step time (`m4a.c:1132-1137`, `:1090-1098`),
    /// which freezes the hardware envelope (`stepTime == 0` marks it dead,
    /// `mgba/src/gb/audio.c:948-950`) until another explicit write —
    /// [`Self::sustain_step`]'s refresh and [`Self::pseudo_echo_step`]'s
    /// countdown write neither.
    ///
    /// The phase bytes are whole `u8`s, not 3-bit fields, so only the low
    /// nibble of the combination survives to hardware; the software envelope
    /// keeps pacing itself off the raw byte, as upstream's `envelopeCounter`
    /// does (`m4a.c:1031`, `:1063`, `:1152`).
    #[must_use]
    pub(crate) fn hardware_envelope_pacing(&self) -> Option<HardwareEnvelopePacing> {
        let step_time_and_dir = match self.phase {
            // `channels->attack + CGB_NRx2_ENV_DIR_INC` (`m4a.c:1024`); the
            // sum is masked to a nibble before it reaches NRx2, so wrapping
            // at a byte discards only bits the mask drops anyway.
            Phase::Attack => self.adsr.attack.wrapping_add(NRX2_ENV_DIR_INC),
            // `channels->decay | CGB_NRx2_ENV_DIR_DEC` (`m4a.c:1158`), whose
            // direction constant is zero (`m4a_internal.h:84`).
            Phase::Decay => self.adsr.decay,
            // `channels->release | CGB_NRx2_ENV_DIR_DEC` (`m4a.c:1068`).
            Phase::Release => self.adsr.release,
            Phase::Starting | Phase::Sustain | Phase::PseudoEcho | Phase::Retired => return None,
        };
        HardwareEnvelopePacing::from_nrx2_step_time_and_dir(step_time_and_dir)
    }

    /// Iterations the note-on write leaves before the hardware envelope's
    /// first step, for arming a fresh voice's hardware timer. Upstream's
    /// note-on raises `CGB_CHANNEL_MO_VOL` alongside the attack's own NRx2
    /// step time (`m4a.c:993`, `:1029`), so hardware starts pacing from that
    /// write; this counter carries [`transition_frame_delay`]'s note-on
    /// offset so hardware and software share one phase from the first frame.
    #[must_use]
    pub(crate) fn iterations_until_first_step(&self) -> u8 {
        self.frames_until_step
    }

    /// Enter the release phase, reporting whether release itself is the
    /// retrigger-worthy volume write, once only (`m4a.c:1060-1069,1062`).
    /// Only a live phase releases: a note stopped before its first `step`
    /// records the request and retires on that pass instead
    /// (`m4a.c:988..1046`, `:1053..1057`), and one already past sustain has
    /// no release write left to report.
    pub fn note_off(&mut self) -> bool {
        let live = matches!(self.phase, Phase::Attack | Phase::Decay | Phase::Sustain);
        self.note_off_requested = true;
        if !live {
            return false;
        }
        self.phase = Phase::Release;
        self.frames_until_step = transition_frame_delay(self.adsr.release);
        self.adsr.release != 0
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
    ///
    /// Returns whether this iteration crossed a retrigger-worthy
    /// `CGB_CHANNEL_MO_VOL` transition (`m4a.c:1090-1158`; see
    /// [`crate::cgb_voice::Oscillator::retrigger`]).
    pub fn step(&mut self) -> bool {
        match self.phase {
            Phase::Starting => self.start(),
            Phase::Attack => self.attack_step(),
            Phase::Decay => self.decay_step(),
            Phase::Sustain => {
                self.sustain_step();
                false
            }
            Phase::Release if self.adsr.release == 0 => self.enter_pseudo_echo_or_silence(),
            Phase::Release => self.release_step(),
            Phase::PseudoEcho => {
                self.pseudo_echo_step();
                false
            }
            Phase::Retired => false,
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
    ///
    /// Returns whether either iteration retriggered: upstream applies at
    /// most one hardware write per `CgbSound` iteration (`m4a.c:1206-1226`).
    pub(crate) fn step_frame(&mut self, extra_iteration: bool) -> bool {
        let retriggered = self.step();
        if extra_iteration && !matches!(self.phase, Phase::PseudoEcho | Phase::Retired) {
            return self.step() || retriggered;
        }
        retriggered
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

    /// Apply this note's first `CgbSound` pass: retire at once if it was
    /// already stopped, else begin its attack (`Self::note_off`'s doc).
    /// Reports the attack's own volume write; silencing a stopped note is
    /// not one (see [`Self::enter_pseudo_echo_or_silence`]).
    fn start(&mut self) -> bool {
        if self.note_off_requested {
            self.silence();
            return false;
        }
        self.phase = Phase::Attack;
        self.attack_step()
    }

    fn attack_step(&mut self) -> bool {
        if self.adsr.attack == 0 {
            return self.enter_decay();
        }
        if !self.paced_step_is_due() {
            return false;
        }
        if self.volume < self.goal {
            self.volume += 1;
        }
        if self.volume >= self.goal {
            self.enter_decay()
        } else {
            self.frames_until_step = self.adsr.attack;
            false
        }
    }

    /// Enter decay, reporting whether this is the attack-to-decay
    /// volume write (`m4a.c:1140-1158`).
    fn enter_decay(&mut self) -> bool {
        if self.adsr.decay == 0 {
            return self.enter_sustain_start();
        }
        self.volume = self.goal;
        self.phase = Phase::Decay;
        self.frames_until_step = self.adsr.decay;
        true
    }

    fn decay_step(&mut self) -> bool {
        if !self.paced_step_is_due() {
            return false;
        }
        if self.volume > self.sustain_goal {
            self.volume -= 1;
        }
        if self.volume <= self.sustain_goal {
            self.enter_sustain_start()
        } else {
            self.frames_until_step = self.adsr.decay;
            false
        }
    }

    /// Enter sustain, reporting whether this is the decay-to-sustain
    /// volume write (`m4a.c:1126-1137`).
    fn enter_sustain_start(&mut self) -> bool {
        if self.adsr.sustain == 0 {
            return self.enter_pseudo_echo_or_silence();
        }
        self.volume = self.sustain_goal;
        self.phase = Phase::Sustain;
        self.frames_until_step = SUSTAIN_REFRESH_FRAMES;
        true
    }

    fn sustain_step(&mut self) {
        if self.paced_step_is_due() {
            self.volume = self.sustain_goal;
            self.frames_until_step = SUSTAIN_REFRESH_FRAMES;
        }
    }

    fn release_step(&mut self) -> bool {
        if !self.paced_step_is_due() {
            return false;
        }
        if self.volume > 0 {
            self.volume -= 1;
        }
        if self.volume == 0 {
            self.enter_pseudo_echo_or_silence()
        } else {
            self.frames_until_step = self.adsr.release;
            false
        }
    }

    /// Enter the pseudo-echo tail, or silence outright when its floor is
    /// zero, reporting whether entering the tail is the pseudo-echo-start
    /// volume write (`m4a.c:1090-1103`; see [`crate::cgb_voice::Oscillator::retrigger`]
    /// for why silence is not).
    fn enter_pseudo_echo_or_silence(&mut self) -> bool {
        let floor = cgb_echo_floor(self.goal, self.echo_volume);
        if floor == 0 {
            self.silence();
            false
        } else {
            self.phase = Phase::PseudoEcho;
            self.volume = floor;
            true
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
    fn hardware_envelope_is_paced_only_through_attack_decay_and_release() {
        // Pins every phase `hardware_envelope_pacing` classifies, and the
        // step time and direction each paced phase writes into NRx2 (that
        // method's doc). Each phase gets its own minimal envelope so a
        // zero-delay neighbor can't skip past it before its pacing is observed.
        let up = |step_time| {
            Some(HardwareEnvelopePacing {
                step_time,
                increasing: true,
            })
        };
        let down = |step_time| {
            Some(HardwareEnvelopePacing {
                step_time,
                increasing: false,
            })
        };

        let mut attacking = plain_envelope(adsr(5, 0, 8, 0), 10);
        assert_eq!(
            attacking.hardware_envelope_pacing(),
            None,
            "not yet started"
        );
        attacking.step(); // Starting -> Attack, paced by attack == 5
        assert_eq!(
            attacking.hardware_envelope_pacing(),
            up(5),
            "attack paces hardware upward at its own step time"
        );

        let mut decaying = plain_envelope(adsr(0, 5, 8, 0), 10);
        decaying.step(); // Starting -> Attack (attack == 0) -> Decay
        assert_eq!(
            decaying.hardware_envelope_pacing(),
            down(5),
            "decay paces hardware downward at its own step time"
        );

        let mut sustaining = plain_envelope(adsr(0, 0, 8, 0), 10);
        sustaining.step(); // Starting -> Attack -> Decay (both == 0) -> Sustain
        assert_eq!(
            sustaining.hardware_envelope_pacing(),
            None,
            "sustain's zero step-time nibble holds hardware dead"
        );

        let mut releasing = plain_envelope(adsr(0, 0, 8, 4), 10);
        releasing.step(); // ... -> Sustain
        releasing.note_off(); // Sustain -> Release, paced by release == 4
        assert_eq!(
            releasing.hardware_envelope_pacing(),
            down(4),
            "release paces hardware downward at its own step time"
        );

        let mut echoing = CgbEnvelope::new(adsr(0, 0, 15, 0), 8, 128, 2);
        echoing.step(); // ... -> Sustain
        echoing.note_off(); // Sustain -> Release (release == 0, held for one step)
        echoing.step(); // Release -> PseudoEcho
        assert_eq!(
            echoing.hardware_envelope_pacing(),
            None,
            "the pseudo-echo tail's zero step-time nibble holds hardware dead"
        );
    }

    #[test]
    fn envelope_bytes_above_seven_pace_hardware_through_the_nrx2_nibble() {
        // Pins `hardware_envelope_pacing`'s decode: a phase byte wider than
        // NRx2's 3-bit step time reaches hardware as the low nibble of the
        // byte upstream combines with its direction constant, so it can pace
        // at a different period and in the opposite direction from the raw
        // byte, or not at all.
        let up = |step_time| {
            Some(HardwareEnvelopePacing {
                step_time,
                increasing: true,
            })
        };
        let down = |step_time| {
            Some(HardwareEnvelopePacing {
                step_time,
                increasing: false,
            })
        };
        let attacking = |attack| {
            let mut env = plain_envelope(adsr(attack, 0, 8, 0), 10);
            env.step(); // Starting -> Attack
            env
        };
        let decaying = |decay| {
            let mut env = plain_envelope(adsr(0, decay, 8, 0), 10);
            env.step(); // Starting -> Attack (attack == 0) -> Decay
            env
        };
        let releasing = |release| {
            let mut env = plain_envelope(adsr(0, 0, 8, release), 10);
            env.step(); // ... -> Sustain
            env.note_off(); // Sustain -> Release
            env
        };

        // `(8 + CGB_NRx2_ENV_DIR_INC) & 0xF == 0`: a dead, downward envelope,
        // not an upward step every 8 iterations.
        assert_eq!(attacking(8).hardware_envelope_pacing(), None);
        // `(9 + CGB_NRx2_ENV_DIR_INC) & 0xF == 1`.
        assert_eq!(attacking(9).hardware_envelope_pacing(), down(1));
        // `15 + CGB_NRx2_ENV_DIR_INC` keeps only the step time it overflows into.
        assert_eq!(attacking(15).hardware_envelope_pacing(), down(7));

        // `8 | CGB_NRx2_ENV_DIR_DEC == 8`: bit 3 alone, so dead again.
        assert_eq!(decaying(8).hardware_envelope_pacing(), None);
        // `9 | CGB_NRx2_ENV_DIR_DEC == 9`: bit 3 turns the decay upward.
        assert_eq!(decaying(9).hardware_envelope_pacing(), up(1));

        assert_eq!(releasing(12).hardware_envelope_pacing(), up(4));
        assert_eq!(releasing(u8::MAX).hardware_envelope_pacing(), up(7));
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
    fn note_off_before_the_first_pass_retires_at_once_unlike_after_it() {
        // Stopped before its first pass: `CgbSound`'s `SF_START | SF_STOP`
        // check retires it at once, bypassing pseudo-echo (`m4a.c:988..1046`).
        let mut env = CgbEnvelope::new(adsr(0, 0, 15, 0), 8, 128, 2);

        assert!(
            !env.note_off(),
            "a note-off before the first pass takes no release transition, so it owes no \
             CGB_CHANNEL_MO_VOL write"
        );
        assert!(
            !env.step(),
            "the short-circuit reaches CgbOscOff through `goto oscillator_off`, past the \
             modify/CgbModVol write (`m4a.c:988..996`, `:1040..1047`)"
        );

        assert!(!env.is_active());
        assert_eq!(env.volume(), 0);
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
