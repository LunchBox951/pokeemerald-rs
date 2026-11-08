//! Frequency-register and phase-accumulator arithmetic shared by the channel generators.

use crate::pitch::MIXER_RATE;

pub(super) const PHASE_ONE: u32 = 1 << 16;
pub(super) const FREQUENCY_REGISTER_RANGE: u16 = 1 << 11;
pub(super) const MAX_FREQUENCY_REGISTER: u16 = FREQUENCY_REGISTER_RANGE - 1;

pub(super) fn register_frequency_hz(frequency_register: u16, clock_hz: f64) -> f64 {
    let frequency_register = f64::from(frequency_register.min(MAX_FREQUENCY_REGISTER));
    clock_hz / (f64::from(FREQUENCY_REGISTER_RANGE) - frequency_register)
}

pub(super) fn phase_delta(hz: f64, steps_per_cycle: f64) -> u32 {
    let delta = (hz * steps_per_cycle * f64::from(PHASE_ONE)) / f64::from(MIXER_RATE);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "register frequencies produce positive deltas within u32"
    )]
    {
        delta as u32
    }
}
