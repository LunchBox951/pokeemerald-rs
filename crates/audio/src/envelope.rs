//! The DirectSound per-voice ADSR envelope, advanced once per render frame.
//!
//! Behavioural port of the envelope arm of `SoundMainRAM` (`m4a_1.s:171`..
//! `:263`) — the DirectSound path, *not* the CGB PSG envelope in
//! `m4a.c:CgbSound`, which is out of scope. Only the envelope volume is
//! modelled here; turning that into per-sample left/right gain lives in
//! [`crate::voice`].
//!
//! Quirk-dense corners preserved from upstream:
//! - **Attack is additive**, decay and release are **multiplicative**
//!   (`env = env * rate >> 8`), and sustain is a *level*, not a rate.
//! - The attack phase ends only once volume reaches `255` (`cmp r5, 0xFF;
//!   bcc` keeps attacking while strictly below `0xFF`).
//! - A zero sustain level, or a decay/release that falls to the pseudo-echo
//!   floor, drops straight into the pseudo-echo (`IEC`) tail.

/// Attack/decay/sustain/release parameters copied from a voice's instrument
/// (`ToneData`). Attack is a per-frame increment; decay and release are
/// per-frame `/256` multipliers; sustain is a target level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adsr {
    /// Per-frame additive attack increment.
    pub attack: u8,
    /// Per-frame multiplicative decay ratio (`env = env * decay >> 8`).
    pub decay: u8,
    /// Sustain level the decay settles to.
    pub sustain: u8,
    /// Per-frame multiplicative release ratio, applied once stopped.
    pub release: u8,
}

impl Adsr {
    /// A fixed-gain envelope: instant attack, no decay, full sustain, instant
    /// release. Useful for isolating the mixer in tests.
    #[must_use]
    pub fn flat() -> Self {
        Self {
            attack: 0xFF,
            decay: 0xFF,
            sustain: 0xFF,
            release: 0,
        }
    }
}

/// Which multiplicative/additive rule the envelope currently follows.
///
/// Mirrors `SOUND_CHANNEL_SF_ENV` values `ATTACK=3`, `DECAY=2`, `SUSTAIN=1`.
/// The release state (`0`) is not a distinct phase in the DirectSound path:
/// it is driven by the `stop` flag, so it is not represented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Volume ramps up additively toward `255`.
    Attack,
    /// Volume decays multiplicatively toward the sustain level.
    Decay,
    /// Volume holds at the sustain level until the note is stopped.
    Sustain,
}

/// Live envelope state for one voice.
///
/// The four booleans mirror distinct hardware status-flag bits (`SF_START`,
/// `SF_STOP`, `SF_IEC`, and the active/`SF_ON` state); collapsing them would
/// obscure the one-to-one mapping to `SoundMainRAM`'s branches.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug)]
pub struct Envelope {
    adsr: Adsr,
    phase: Phase,
    volume: u8,
    /// Set on the first frame of a note; consumed into the attack phase.
    start: bool,
    /// Set on note-off; drives the multiplicative release.
    stop: bool,
    /// In the pseudo-echo (`IEC`) tail: volume is frozen and counts down.
    echo: bool,
    /// Pseudo-echo floor volume and length (both `0` for a plain note).
    echo_volume: u8,
    echo_length: u8,
    /// Cleared when the envelope has fully faded and the voice should retire.
    active: bool,
}

impl Envelope {
    /// Begin a fresh note with the given parameters. The first [`Self::step`]
    /// transitions out of the start state and performs the first attack
    /// increment, matching the hardware ordering.
    #[must_use]
    pub fn new(adsr: Adsr, echo_volume: u8, echo_length: u8) -> Self {
        Self {
            adsr,
            phase: Phase::Attack,
            volume: 0,
            start: true,
            stop: false,
            echo: false,
            echo_volume,
            echo_length,
            active: true,
        }
    }

    /// Current envelope volume (`0..=255`).
    #[must_use]
    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// The current phase (for inspection/testing).
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Whether the voice is still producing (or about to produce) sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Whether the note has entered its release (stopped) state.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.stop
    }

    /// Request note-off: the next steps apply the multiplicative release.
    /// Idempotent (`SOUND_CHANNEL_SF_STOP` is a sticky flag).
    pub fn note_off(&mut self) {
        self.stop = true;
    }

    /// Retire the voice immediately (its wave ran out with no loop, matching
    /// the mixer clearing `statusFlags` when `count` is exhausted,
    /// `m4a_1.s:378`). No further steps produce sound.
    pub fn retire(&mut self) {
        self.active = false;
    }

    /// Advance the envelope by one render frame.
    ///
    /// Sequenced exactly as `SoundMainRAM`'s envelope arm: start → attack,
    /// echo countdown, stop-driven release, then the phase-specific
    /// attack/decay/sustain update.
    pub fn step(&mut self) {
        if !self.active {
            return;
        }

        if self.start {
            self.start = false;
            if self.stop {
                // START|STOP arrives as an immediate kill (`_081DCFB0`).
                self.active = false;
                return;
            }
            self.phase = Phase::Attack;
            self.volume = 0;
            self.attack_step();
            return;
        }

        if self.echo {
            // Pseudo-echo tail: freeze volume, count the length down. `subs;
            // bhi` keeps the tail alive only while the pre-decrement length is
            // at least 2 (`_081DCFA0`).
            let old = self.echo_length;
            self.echo_length = old.wrapping_sub(1);
            if old < 2 {
                self.active = false;
            }
            return;
        }

        if self.stop {
            self.volume = mul_shift8(self.volume, self.adsr.release);
            if self.volume <= self.echo_volume {
                self.enter_echo();
            }
            return;
        }

        match self.phase {
            Phase::Attack => self.attack_step(),
            Phase::Decay => {
                self.volume = mul_shift8(self.volume, self.adsr.decay);
                if self.volume <= self.adsr.sustain {
                    self.volume = self.adsr.sustain;
                    if self.adsr.sustain == 0 {
                        self.enter_echo();
                    } else {
                        self.phase = Phase::Sustain;
                    }
                }
            }
            // Sustain holds until `note_off` flips `stop`.
            Phase::Sustain => {}
        }
    }

    /// One additive attack increment; clamps at `255` and hands off to decay
    /// (`_081DCFF8`). Kept `< 0xFF` stays in attack, matching `bcc`.
    fn attack_step(&mut self) {
        let raised = u16::from(self.volume) + u16::from(self.adsr.attack);
        if raised < 0xFF {
            self.volume = u8::try_from(raised).unwrap_or(0xFF);
        } else {
            self.volume = 0xFF;
            self.phase = Phase::Decay;
        }
    }

    /// Drop into the pseudo-echo tail, or retire the voice if there is no echo
    /// floor (`_081DCFC8`).
    fn enter_echo(&mut self) {
        if self.echo_volume == 0 {
            self.active = false;
        } else {
            self.echo = true;
            self.volume = self.echo_volume;
        }
    }
}

/// `(a * b) >> 8`, the multiplicative decay/release step (`muls; lsrs #8`).
fn mul_shift8(a: u8, b: u8) -> u8 {
    // `a * b <= 255 * 255 = 65025`, so `>> 8 <= 254`; the cast never truncates.
    #[allow(clippy::cast_possible_truncation)]
    {
        ((u16::from(a) * u16::from(b)) >> 8) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adsr(attack: u8, decay: u8, sustain: u8, release: u8) -> Adsr {
        Adsr {
            attack,
            decay,
            sustain,
            release,
        }
    }

    #[test]
    fn additive_attack_reaches_full_then_enters_decay() {
        let mut env = Envelope::new(adsr(0x40, 0xFF, 0x80, 0), 0, 0);
        env.step(); // start -> attack, +0x40
        assert_eq!(env.volume(), 0x40);
        assert_eq!(env.phase(), Phase::Attack);
        env.step();
        assert_eq!(env.volume(), 0x80);
        env.step();
        assert_eq!(env.volume(), 0xC0);
        env.step(); // 0xC0 + 0x40 = 0x100 >= 0xFF -> clamp + decay
        assert_eq!(env.volume(), 0xFF);
        assert_eq!(env.phase(), Phase::Decay);
    }

    #[test]
    fn instant_attack_lands_on_full_and_decays_next_frame() {
        // attack 0xFF: 0 + 0xFF = 0xFF, not < 0xFF, so clamp + straight to decay.
        let mut env = Envelope::new(adsr(0xFF, 0x80, 0x40, 0), 0, 0);
        env.step();
        assert_eq!(env.volume(), 0xFF);
        assert_eq!(env.phase(), Phase::Decay);
    }

    #[test]
    fn multiplicative_decay_settles_at_sustain() {
        let mut env = Envelope::new(adsr(0xFF, 0x80, 0x40, 0), 0, 0);
        env.step(); // -> 0xFF, decay
        env.step(); // 0xFF * 0x80 >> 8 = 0x7F
        assert_eq!(env.volume(), 0x7F);
        env.step(); // 0x7F * 0x80 >> 8 = 0x3F  <= sustain 0x40 -> clamp to 0x40, sustain
        assert_eq!(env.volume(), 0x40);
        assert_eq!(env.phase(), Phase::Sustain);
        env.step(); // holds
        assert_eq!(env.volume(), 0x40);
        assert_eq!(env.phase(), Phase::Sustain);
    }

    #[test]
    fn zero_sustain_decays_into_silence_and_retires() {
        let mut env = Envelope::new(adsr(0xFF, 0x80, 0, 0), 0, 0);
        env.step(); // full
                    // Decay halves each frame until <= sustain(0), no echo -> retire.
        for _ in 0..12 {
            env.step();
        }
        assert!(!env.is_active());
    }

    #[test]
    fn release_multiplies_down_after_note_off() {
        let mut env = Envelope::new(adsr(0xFF, 0xFF, 0xFF, 0x80), 0, 0);
        env.step(); // full, decay (decay 0xFF keeps it at 0xFF)
        env.step(); // 0xFF*0xFF>>8 = 0xFE  (> sustain 0xFF? no) -> sustain path
        assert!(env.volume() >= 0xFE);
        env.note_off();
        env.step(); // release: vol * 0x80 >> 8
        let after_first = env.volume();
        assert!(after_first < 0xFE);
        env.step();
        assert!(env.volume() < after_first);
        // With no echo floor, release eventually retires the voice.
        for _ in 0..16 {
            env.step();
        }
        assert!(!env.is_active());
    }

    #[test]
    fn start_and_stop_on_same_frame_kills_immediately() {
        let mut env = Envelope::new(Adsr::flat(), 0, 0);
        env.note_off();
        env.step();
        assert!(!env.is_active());
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn pseudo_echo_tail_holds_volume_then_retires() {
        // release drops to the echo floor, which then counts down `echo_length`.
        let mut env = Envelope::new(adsr(0xFF, 0xFF, 0xFF, 0x80), 0x30, 3);
        env.step();
        env.note_off();
        // Step release until it reaches the echo floor.
        while !env.is_stopping() || env.volume() > 0x30 {
            env.step();
            if !env.is_active() {
                break;
            }
        }
        // Once in the echo tail the volume is pinned at the floor and the
        // `echo_length` (3) counts down over three tail frames.
        assert_eq!(env.volume(), 0x30);
        assert!(env.is_active());
        env.step();
        assert_eq!(env.volume(), 0x30);
        assert!(env.is_active());
        env.step();
        assert!(env.is_active());
        env.step();
        assert!(!env.is_active());
    }
}
