//! Software waveform generators for the four CGB PSG channels the M4A driver
//! drives through `NRxx` hardware sound registers in `CgbSound`
//! (`m4a.c:925`).
//!
//! There is no real Game Boy APU underneath this crate, so each channel type
//! is reimplemented from register semantics as its own sample generator:
//! [`SquareChannel`] (duty cycle plus channel-1 frequency sweep),
//! [`WaveChannel`] (32 four-bit samples read from wave RAM), and
//! [`NoiseChannel`] (the 15-bit/7-bit Galois LFSR). Register formulas are
//! cross-checked against `mgba/src/gb/audio.c` for clarification only — the
//! generators here are a from-scratch, unbatched reimplementation
//! `(no-verbatim)`, stepped once per output sample at
//! [`crate::pitch::MIXER_RATE`] rather than at the hardware's own clock.

use crate::pitch::MIXER_RATE;

/// `.16` fixed-point one, used by every channel's phase accumulator.
const PHASE_ONE: u32 = 1 << 16;

/// The four Game Boy square-wave duty patterns (12.5%, 25%, 50%, 75%), one
/// high/low flag per of 8 steps. Matches the hardware's `_squareChannelDuty`
/// table (cross-checked against `mgba/src/gb/audio.c`).
const DUTY_TABLE: [[bool; 8]; 4] = [
    [false, false, false, false, false, false, false, true],
    [true, false, false, false, false, false, false, true],
    [true, false, false, false, false, true, true, true],
    [false, true, true, true, true, true, true, false],
];

/// Convert an 11-bit `NRx3`/`NRx4` register value into the channel's real
/// frequency in Hz. `period_hz` is `131072` for the square channels or
/// `65536` for the wave channel (the wave channel reads twice as many
/// samples per cycle).
fn register_freq_hz(freq_reg: u16, period_hz: f64) -> f64 {
    let reg = f64::from(freq_reg.min(0x7FF));
    period_hz / (2048.0 - reg)
}

/// `.16` fixed-point per-output-sample phase advance for a generator that
/// repeats `steps_per_cycle` internal steps at `hz`.
fn phase_delta(hz: f64, steps_per_cycle: f64) -> u32 {
    let delta = (hz * steps_per_cycle * f64::from(PHASE_ONE)) / f64::from(MIXER_RATE);
    // Frequencies in the supported range keep this well within `u32`; a
    // negative or absurd input (which the register clamp above prevents)
    // would saturate rather than panic.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        delta.max(0.0) as u32
    }
}

/// Channel-1 frequency sweep: periodically nudges the playing frequency up
/// or down, disabling the channel outright on overflow.
///
/// Behavioural port of the sweep unit `CgbSound` configures via `NR10`
/// (`ply_note`'s CGB branch, `m4a_1.s:1773`..`:1783`, and the sweep math
/// itself is hardware, cross-checked against `mgba/src/gb/audio.c`'s
/// `_updateSweep`). The real unit ticks at 128 Hz against a shared frame
/// sequencer; this reimplementation instead counts whole render frames per
/// step `(no-verbatim)` — a deliberate, benign simplification in the spirit
/// of `mixer::clip`'s clip-vs-wrap choice: the audible sweep shape survives,
/// only its exact tick-for-tick cadence does not.
#[derive(Clone, Copy, Debug)]
pub struct Sweep {
    shift: u8,
    subtract: bool,
    /// Frames between steps; `0` disables the sweep entirely (matches
    /// hardware: a zero sweep *time* never fires).
    period: u8,
    counter: u8,
    /// The sweep unit's own shadow frequency register, mutated in place.
    frequency: u16,
}

/// What a [`Sweep::tick`] did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepResult {
    /// No step fired, or the step fired but `shift == 0` so it changed
    /// nothing.
    Unchanged,
    /// The frequency changed; the channel should retune to it.
    Changed(u16),
    /// The frequency overflowed past `2047`; the channel must be silenced.
    Disable,
}

impl Sweep {
    /// Decode a raw `NR10`-style sweep byte: bits `6..=4` are the period,
    /// bit `3` the direction (set = subtract), bits `2..=0` the shift.
    #[must_use]
    pub fn from_byte(byte: u8, initial_freq_reg: u16) -> Self {
        Self {
            shift: byte & 0x07,
            subtract: byte & 0x08 != 0,
            period: (byte >> 4) & 0x07,
            counter: (byte >> 4) & 0x07,
            frequency: initial_freq_reg.min(0x7FF),
        }
    }

    /// Whether the overflow calc hardware runs *immediately at trigger* would
    /// disable the channel. On note-on, if the sweep shift is non-zero the
    /// unit runs one upward overflow check right away — regardless of the sweep
    /// period, period `0` included (`mgba audio.c:184`, `_updateSweep(ch,
    /// true)`; the upward branch disables when `frequency + (frequency >>
    /// shift) >= 2048`, `audio.c:975`..`:986`). A subtract-direction sweep
    /// never disables at trigger (its result can only fall, `audio.c:969`), so
    /// this reports `true` only for an additive sweep with non-zero shift whose
    /// first step would overflow `0x7FF`.
    #[must_use]
    pub fn overflows_at_trigger(&self) -> bool {
        if self.subtract || self.shift == 0 {
            return false;
        }
        let delta = self.frequency >> self.shift;
        self.frequency + delta > 0x7FF
    }

    /// Advance the sweep unit by one render frame.
    pub fn tick(&mut self) -> SweepResult {
        if self.period == 0 {
            return SweepResult::Unchanged;
        }
        if self.counter == 0 {
            self.counter = self.period;
        }
        self.counter -= 1;
        if self.counter != 0 {
            return SweepResult::Unchanged;
        }
        self.counter = self.period;
        if self.shift == 0 {
            return SweepResult::Unchanged;
        }

        let delta = self.frequency >> self.shift;
        let new_freq = if self.subtract {
            i32::from(self.frequency) - i32::from(delta)
        } else {
            i32::from(self.frequency) + i32::from(delta)
        };
        if !(0..=0x7FF).contains(&new_freq) {
            return SweepResult::Disable;
        }
        // `new_freq` was just range-checked into `0..=0x7FF`.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        {
            self.frequency = new_freq as u16;
        }
        SweepResult::Changed(self.frequency)
    }
}

/// A square-wave channel (CGB hardware channels 1 and 2): an 8-step duty
/// pattern read at a frequency-derived rate, with an optional sweep on
/// channel 1.
#[derive(Clone, Debug)]
pub struct SquareChannel {
    duty: u8,
    phase: u32,
    step_delta: u32,
    sweep: Option<Sweep>,
    /// Set when the channel-1 sweep's trigger-time overflow check already fired
    /// (see [`Sweep::overflows_at_trigger`]): the channel is born dead and the
    /// voice layer must treat it as immediately inactive, exactly as a sweep
    /// overflow on a later tick retires it.
    disabled: bool,
}

impl SquareChannel {
    /// A square channel at `freq_reg` with duty pattern `duty` (`0..=3`,
    /// wrapped). `sweep` is `Some` only for channel 1 — channel 2 has no
    /// hardware `NR20` sweep register to drive (`CgbSound`'s `switch(ch)`
    /// only writes the sweep register in the channel-1 arm before falling
    /// through, `m4a.c:998`).
    #[must_use]
    pub fn new(duty: u8, freq_reg: u16, sweep: Option<Sweep>) -> Self {
        let disabled = sweep.as_ref().is_some_and(Sweep::overflows_at_trigger);
        let mut chan = Self {
            duty: duty & 0x03,
            phase: 0,
            step_delta: 0,
            sweep,
            disabled,
        };
        chan.set_frequency(freq_reg);
        chan
    }

    /// Whether this channel was born dead because its sweep's trigger-time
    /// overflow check already fired (see [`Sweep::overflows_at_trigger`]).
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Retune to a new 11-bit frequency register value.
    pub fn set_frequency(&mut self, freq_reg: u16) {
        let hz = register_freq_hz(freq_reg, 131_072.0);
        self.step_delta = phase_delta(hz, 8.0);
    }

    /// Advance the sweep unit (channel 1 only) by one render frame,
    /// retuning this channel's rate on a frequency change. Returns `false`
    /// once the sweep has overflowed and the channel must be silenced.
    pub fn step_sweep_frame(&mut self) -> bool {
        let Some(sweep) = self.sweep.as_mut() else {
            return true;
        };
        match sweep.tick() {
            SweepResult::Unchanged => true,
            SweepResult::Changed(freq) => {
                self.set_frequency(freq);
                true
            }
            SweepResult::Disable => false,
        }
    }

    /// Produce the next bipolar unit sample (`-1` or `1`).
    pub fn sample(&mut self) -> i8 {
        let step = (self.phase / PHASE_ONE) as usize & 0x07;
        self.phase = self.phase.wrapping_add(self.step_delta);
        if DUTY_TABLE[self.duty as usize][step] {
            1
        } else {
            -1
        }
    }
}

/// The programmable wave channel (CGB hardware channel 3): 32 four-bit
/// samples read cyclically at a frequency-derived rate.
///
/// The channel carries no output-level byte of its own: upstream's
/// `voice_programmable_wave` instrument has no level field (`music_voice.inc`;
/// `ToneData`, `m4a_internal.h:57`), and channel-3 amplitude comes *solely*
/// from the envelope, which the driver quantises through `gCgb3Vol` when
/// writing `NR32` (`*nrx2ptr = gCgb3Vol[envelopeVolume]`, `m4a.c:1211`). That
/// stepped level is applied in the voice's render path
/// ([`crate::cgb_voice`]); this generator emits the raw decoded nibble.
#[derive(Clone, Debug)]
pub struct WaveChannel {
    samples: [i8; 32],
    phase: u32,
    step_delta: u32,
}

impl WaveChannel {
    /// Unpack 16 wave-RAM bytes (two 4-bit samples each, high nibble first)
    /// into 32 signed samples centred on zero (`nibble - 8`).
    #[must_use]
    pub fn decode_wave_ram(bytes: &[u8; 16]) -> [i8; 32] {
        let mut out = [0i8; 32];
        for (i, &byte) in bytes.iter().enumerate() {
            // `nibble` is `0..=15`; the cast to `i8` never truncates.
            #[allow(clippy::cast_possible_wrap)]
            let hi = (byte >> 4) as i8 - 8;
            #[allow(clippy::cast_possible_wrap)]
            let lo = (byte & 0x0F) as i8 - 8;
            out[i * 2] = hi;
            out[i * 2 + 1] = lo;
        }
        out
    }

    /// A wave channel over already-decoded `samples` at `freq_reg`.
    #[must_use]
    pub fn new(samples: [i8; 32], freq_reg: u16) -> Self {
        let mut chan = Self {
            samples,
            phase: 0,
            step_delta: 0,
        };
        chan.set_frequency(freq_reg);
        chan
    }

    /// Retune to a new 11-bit frequency register value.
    pub fn set_frequency(&mut self, freq_reg: u16) {
        let hz = register_freq_hz(freq_reg, 65_536.0);
        self.step_delta = phase_delta(hz, 32.0);
    }

    /// Produce the next decoded nibble sample, in `-8..=7`. Channel-3 output
    /// level is applied later from the envelope (see the type's doc comment),
    /// not here.
    pub fn sample(&mut self) -> i8 {
        let step = (self.phase / PHASE_ONE) as usize & 0x1F;
        self.phase = self.phase.wrapping_add(self.step_delta);
        self.samples[step]
    }
}

/// The noise channel (CGB hardware channel 4): a Galois LFSR clocked at a
/// rate derived from an `NR43`-style control byte, in either 15-bit (normal)
/// or 7-bit (narrow) width mode.
#[derive(Clone, Debug)]
pub struct NoiseChannel {
    lfsr: u16,
    narrow: bool,
    phase: u32,
    step_delta: u32,
    output: i8,
}

/// Decode an `NR43`-style control byte (clock shift in bits `7..=4`, width
/// mode in bit `3`, divisor ratio in bits `2..=0`) into `(step_delta,
/// narrow)`. Divisor formula from the Game Boy noise channel: ratio `0` acts
/// as `0.5`, so its base rate is double every other ratio's.
fn noise_control(byte: u8) -> (u32, bool) {
    let shift = byte >> 4;
    let narrow = byte & 0x08 != 0;
    let ratio = byte & 0x07;
    let base_hz = if ratio == 0 {
        524_288.0 * 2.0
    } else {
        524_288.0 / f64::from(ratio)
    };
    let hz = base_hz / f64::from(1u32 << (shift + 1));
    (phase_delta(hz, 1.0), narrow)
}

impl NoiseChannel {
    /// Decode an `NR43`-style control byte into a noise channel. The LFSR
    /// resets to `0`, matching a real channel retrigger.
    #[must_use]
    pub fn from_control_byte(byte: u8) -> Self {
        let (step_delta, narrow) = noise_control(byte);
        let mut chan = Self {
            lfsr: 0,
            narrow,
            phase: 0,
            step_delta,
            output: -1,
        };
        chan.shift_lfsr();
        chan
    }

    /// Retune to a new control byte without resetting the LFSR, matching a
    /// live `NR43` rewrite on real hardware (only a channel *trigger*, not a
    /// frequency change, resets the shift register). The width selector (bit 3)
    /// is preserved from the note-on control byte, not taken from `byte`,
    /// mirroring `CgbSound`'s `*nrx3ptr = (*nrx3ptr & 0x08) | frequency`
    /// (`m4a.c:1200`) — the retune frequency (a `gNoiseTable` entry) never
    /// carries a width bit of its own.
    pub fn retune(&mut self, byte: u8) {
        let (step_delta, _narrow) = noise_control(byte);
        self.step_delta = step_delta;
    }

    /// One Galois LFSR clock: feed back `!(bit0 ^ bit1)` into the top bit
    /// (and, in narrow mode, into bit 6 as well), then shift right.
    fn shift_lfsr(&mut self) {
        let feedback = !(self.lfsr ^ (self.lfsr >> 1)) & 1;
        self.lfsr >>= 1;
        if feedback == 1 {
            self.lfsr |= 0x4000;
            if self.narrow {
                self.lfsr |= 0x0040;
            }
        } else {
            self.lfsr &= !0x4000;
            if self.narrow {
                self.lfsr &= !0x0040;
            }
        }
        self.output = if feedback == 1 { 1 } else { -1 };
    }

    /// Whether the channel is in narrow (7-bit periodic) mode.
    #[cfg(test)]
    pub(crate) fn is_narrow(&self) -> bool {
        self.narrow
    }

    /// Produce the next bipolar unit sample (`-1` or `1`), clocking the LFSR
    /// whenever the phase accumulator completes a step.
    pub fn sample(&mut self) -> i8 {
        self.phase = self.phase.wrapping_add(self.step_delta);
        while self.phase >= PHASE_ONE {
            self.phase -= PHASE_ONE;
            self.shift_lfsr();
        }
        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duty_pattern_matches_the_hardware_table() {
        // Duty 2 (50%) is high for steps 0 and 5..=7 (four of eight).
        let mut chan = SquareChannel::new(2, 0, None);
        // Force one full 8-step cycle by advancing the phase directly rather
        // than depending on a specific frequency/sample-rate ratio.
        chan.step_delta = PHASE_ONE; // one internal step per sample() call
        let steps: Vec<bool> = (0..8).map(|_| chan.sample() > 0).collect();
        assert_eq!(
            steps,
            vec![true, false, false, false, false, true, true, true]
        );
    }

    #[test]
    fn higher_frequency_register_raises_pitch() {
        let low = SquareChannel::new(2, 0, None);
        let high = SquareChannel::new(2, 1900, None);
        assert!(high.step_delta > low.step_delta);
    }

    #[test]
    fn sweep_up_raises_frequency_by_the_shift_formula() {
        // shift=1, add direction (bit3 clear), period=1: freq += freq>>1.
        let mut sweep = Sweep::from_byte(0b0001_0001, 100);
        assert_eq!(sweep.tick(), SweepResult::Changed(150)); // 100 + 50
        assert_eq!(sweep.tick(), SweepResult::Changed(225)); // 150 + 75
    }

    #[test]
    fn sweep_down_lowers_frequency() {
        // shift=1, subtract direction (bit3 set), period=1.
        let mut sweep = Sweep::from_byte(0b0001_1001, 100);
        assert_eq!(sweep.tick(), SweepResult::Changed(50)); // 100 - 50
    }

    #[test]
    fn sweep_overflow_disables_the_channel() {
        // A large starting frequency plus an aggressive shift overflows 0x7FF.
        let mut sweep = Sweep::from_byte(0b0001_0001, 0x700); // add, shift 1
        assert_eq!(sweep.tick(), SweepResult::Disable);
    }

    #[test]
    fn zero_period_sweep_never_fires() {
        let mut sweep = Sweep::from_byte(0b0000_0001, 100); // period 0
        for _ in 0..10 {
            assert_eq!(sweep.tick(), SweepResult::Unchanged);
        }
    }

    #[test]
    fn zero_shift_sweep_ticks_without_changing_frequency() {
        let mut sweep = Sweep::from_byte(0b0001_0000, 100); // period 1, shift 0
        assert_eq!(sweep.tick(), SweepResult::Unchanged);
    }

    #[test]
    fn upward_sweep_overflow_disables_the_channel_at_trigger() {
        // Hardware runs the overflow calc immediately on trigger when the sweep
        // shift is non-zero, regardless of the sweep period — period 0 included
        // (`mgba audio.c:184`). A high starting frequency with an upward shift
        // overflows 0x7FF and the channel is born dead.
        let period_zero = Sweep::from_byte(0b0000_0001, 0x700); // period 0, add, shift 1
        assert!(period_zero.overflows_at_trigger());
        assert!(SquareChannel::new(2, 0x700, Some(period_zero)).is_disabled());

        let with_period = Sweep::from_byte(0b0011_0001, 0x700); // period 3, add, shift 1
        assert!(with_period.overflows_at_trigger());
    }

    #[test]
    fn trigger_overflow_spares_normal_and_downward_sweeps() {
        // A frequency whose first upward step stays within 0x7FF is untouched.
        let normal = Sweep::from_byte(0b0000_0001, 0x100); // add, shift 1: 0x100 + 0x80
        assert!(!normal.overflows_at_trigger());
        assert!(!SquareChannel::new(2, 0x100, Some(normal)).is_disabled());

        // A subtract-direction sweep never disables at trigger (its result can
        // only fall), even from a high frequency.
        let down = Sweep::from_byte(0b0000_1001, 0x700); // subtract, shift 1
        assert!(!down.overflows_at_trigger());

        // Shift 0 never runs the trigger check at all.
        let no_shift = Sweep::from_byte(0b0000_0000, 0x7FF); // add, shift 0
        assert!(!no_shift.overflows_at_trigger());
    }

    #[test]
    fn wave_ram_decodes_high_nibble_first() {
        let bytes = [0xF0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let samples = WaveChannel::decode_wave_ram(&bytes);
        // 0xF0: hi=0xF(15)-8=7, lo=0x0(0)-8=-8.
        assert_eq!(samples[0], 7);
        assert_eq!(samples[1], -8);
        // 0x08: hi=0x0(0)-8=-8, lo=0x8(8)-8=0.
        assert_eq!(samples[2], -8);
        assert_eq!(samples[3], 0);
    }

    #[test]
    fn wave_channel_emits_raw_decoded_nibbles() {
        // The wave generator carries no output-level of its own — that comes
        // solely from the envelope via `gCgb3Vol` in the voice render path
        // (`m4a.c:1211`). `sample()` returns the decoded nibble unattenuated,
        // so a full-swing entry reads back as its raw `-8..=7` value.
        let bytes = [0xF0; 16]; // every decoded sample[0] is 0xF - 8 = 7
        let full = WaveChannel::decode_wave_ram(&bytes);
        let mut chan = WaveChannel::new(full, 0);
        chan.step_delta = 0; // stay on sample 0
        assert_eq!(chan.sample(), 7);
    }

    #[test]
    fn noise_narrow_mode_repeats_much_sooner_than_wide_mode() {
        // 7-bit (narrow) mode has at most 127 distinct LFSR states, so the
        // output sequence must repeat within that many steps; 15-bit (wide)
        // mode's period is far longer and must not repeat that quickly.
        let mut narrow = NoiseChannel::from_control_byte(0x08); // width bit set
        narrow.step_delta = PHASE_ONE;
        let first = narrow.lfsr;
        let mut repeated = false;
        for _ in 0..127 {
            narrow.sample();
            if narrow.lfsr == first {
                repeated = true;
                break;
            }
        }
        assert!(repeated, "narrow-mode LFSR should repeat within 127 steps");

        let mut wide = NoiseChannel::from_control_byte(0x00); // width bit clear
        wide.step_delta = PHASE_ONE;
        let first_wide = wide.lfsr;
        let mut repeated_wide = false;
        for _ in 0..127 {
            wide.sample();
            if wide.lfsr == first_wide {
                repeated_wide = true;
                break;
            }
        }
        assert!(
            !repeated_wide,
            "wide-mode LFSR should not repeat within 127 steps"
        );
    }

    #[test]
    fn noise_channel_is_deterministic() {
        let mut a = NoiseChannel::from_control_byte(0x25);
        let mut b = NoiseChannel::from_control_byte(0x25);
        a.step_delta = PHASE_ONE;
        b.step_delta = PHASE_ONE;
        let seq_a: Vec<i8> = (0..32).map(|_| a.sample()).collect();
        let seq_b: Vec<i8> = (0..32).map(|_| b.sample()).collect();
        assert_eq!(seq_a, seq_b);
    }
}
