//! Per-frame DirectSound ADSR and pseudo-echo state.

const ENVELOPE_RATIO_BITS: u32 = 8;

/// DirectSound attack, decay, sustain, and release parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adsr {
    /// Volume added on each attack frame.
    pub attack: u8,
    /// Per-frame decay ratio out of 256.
    pub decay: u8,
    /// Volume held after decay.
    pub sustain: u8,
    /// Per-frame release ratio out of 256.
    pub release: u8,
}

impl Adsr {
    /// Return an instant, full-sustain, instant-release envelope.
    #[must_use]
    pub fn flat() -> Self {
        Self {
            attack: u8::MAX,
            decay: u8::MAX,
            sustain: u8::MAX,
            release: 0,
        }
    }
}

/// The current attack, decay, or sustain phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Add the attack rate until full volume.
    Attack,
    /// Multiply by the decay ratio until reaching sustain.
    Decay,
    /// Hold the sustain volume until note-off.
    Sustain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lifecycle {
    Starting,
    Active,
    Releasing,
    PseudoEcho,
    Retired,
}

/// Live envelope state for one voice.
#[derive(Clone, Debug)]
pub struct Envelope {
    adsr: Adsr,
    phase: Phase,
    lifecycle: Lifecycle,
    volume: u8,
    note_off_requested: bool,
    echo_volume: u8,
    echo_length: u8,
}

impl Envelope {
    /// Begin a note with its ADSR and pseudo-echo settings.
    #[must_use]
    pub fn new(adsr: Adsr, echo_volume: u8, echo_length: u8) -> Self {
        Self {
            adsr,
            phase: Phase::Attack,
            lifecycle: Lifecycle::Starting,
            volume: 0,
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

    /// Return the current ADSR phase.
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Return whether the voice can still produce sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.lifecycle != Lifecycle::Retired
    }

    /// Return whether note-off has been requested.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.note_off_requested
    }

    /// Request note-off. A note stopped before its first step retires at once.
    pub fn note_off(&mut self) {
        self.note_off_requested = true;
        if self.lifecycle == Lifecycle::Active {
            self.lifecycle = Lifecycle::Releasing;
        }
    }

    /// Retire the voice immediately.
    pub fn retire(&mut self) {
        self.lifecycle = Lifecycle::Retired;
    }

    /// Advance the envelope by one render frame.
    pub fn step(&mut self) {
        match self.lifecycle {
            Lifecycle::Starting => self.start(),
            Lifecycle::Active => self.active_step(),
            Lifecycle::Releasing => self.release_step(),
            Lifecycle::PseudoEcho => self.pseudo_echo_step(),
            Lifecycle::Retired => {}
        }
    }

    fn start(&mut self) {
        if self.note_off_requested {
            // `SoundMainRAM` retires `START | STOP` before initializing attack
            // (`m4a_1.s:177..184`).
            self.lifecycle = Lifecycle::Retired;
            return;
        }
        self.lifecycle = Lifecycle::Active;
        self.phase = Phase::Attack;
        self.volume = 0;
        self.attack_step();
    }

    fn release_step(&mut self) {
        self.volume = apply_ratio(self.volume, self.adsr.release);
        if self.volume <= self.echo_volume {
            self.enter_pseudo_echo();
        }
    }

    fn active_step(&mut self) {
        match self.phase {
            Phase::Attack => self.attack_step(),
            Phase::Decay => {
                self.volume = apply_ratio(self.volume, self.adsr.decay);
                if self.volume <= self.adsr.sustain {
                    self.volume = self.adsr.sustain;
                    if self.adsr.sustain == 0 {
                        self.enter_pseudo_echo();
                    } else {
                        self.phase = Phase::Sustain;
                    }
                }
            }
            Phase::Sustain => {}
        }
    }

    fn attack_step(&mut self) {
        let raised = u16::from(self.volume) + u16::from(self.adsr.attack);
        if raised < u16::from(u8::MAX) {
            self.volume = u8::try_from(raised).unwrap_or(u8::MAX);
        } else {
            self.volume = u8::MAX;
            self.phase = Phase::Decay;
        }
    }

    fn enter_pseudo_echo(&mut self) {
        if self.echo_volume == 0 {
            self.lifecycle = Lifecycle::Retired;
        } else {
            self.lifecycle = Lifecycle::PseudoEcho;
            self.volume = self.echo_volume;
        }
    }

    fn pseudo_echo_step(&mut self) {
        // `SoundMainRAM` keeps the tail only when its byte decrement is
        // unsigned greater than zero (`subs; bhi`, `m4a_1.s:205..213`).
        if self.echo_length <= 1 {
            self.lifecycle = Lifecycle::Retired;
        } else {
            self.echo_length -= 1;
        }
    }
}

/// DirectSound decay and release use an 8-bit multiplier
/// (`m4a_1.s:218..243`).
fn apply_ratio(volume: u8, ratio: u8) -> u8 {
    let scaled = (u16::from(volume) * u16::from(ratio)) >> ENVELOPE_RATIO_BITS;
    u8::try_from(scaled).unwrap_or(u8::MAX)
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
        env.step();
        assert_eq!(env.volume(), 0x40);
        assert_eq!(env.phase(), Phase::Attack);
        env.step();
        assert_eq!(env.volume(), 0x80);
        env.step();
        assert_eq!(env.volume(), 0xC0);
        env.step();
        assert_eq!(env.volume(), 0xFF);
        assert_eq!(env.phase(), Phase::Decay);
    }

    #[test]
    fn instant_attack_lands_on_full_and_decays_next_frame() {
        let mut env = Envelope::new(adsr(0xFF, 0x80, 0x40, 0), 0, 0);
        env.step();
        assert_eq!(env.volume(), 0xFF);
        assert_eq!(env.phase(), Phase::Decay);
    }

    #[test]
    fn multiplicative_decay_settles_at_sustain() {
        let mut env = Envelope::new(adsr(0xFF, 0x80, 0x40, 0), 0, 0);
        env.step();
        env.step();
        assert_eq!(env.volume(), 0x7F);
        env.step();
        assert_eq!(env.volume(), 0x40);
        assert_eq!(env.phase(), Phase::Sustain);
        env.step();
        assert_eq!(env.volume(), 0x40);
        assert_eq!(env.phase(), Phase::Sustain);
    }

    #[test]
    fn zero_sustain_decays_into_silence_and_retires() {
        let mut env = Envelope::new(adsr(0xFF, 0x80, 0, 0), 0, 0);
        env.step();
        for _ in 0..12 {
            env.step();
        }
        assert!(!env.is_active());
    }

    #[test]
    fn release_multiplies_down_after_note_off() {
        let mut env = Envelope::new(adsr(0xFF, 0xFF, 0xFF, 0x80), 0, 0);
        env.step();
        env.step();
        assert!(env.volume() >= 0xFE);
        env.note_off();
        env.step();
        let after_first = env.volume();
        assert!(after_first < 0xFE);
        env.step();
        assert!(env.volume() < after_first);
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
        let mut env = Envelope::new(adsr(0xFF, 0xFF, 0xFF, 0x80), 0x30, 3);
        env.step();
        env.note_off();
        while env.volume() > 0x30 {
            env.step();
            if !env.is_active() {
                break;
            }
        }
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
