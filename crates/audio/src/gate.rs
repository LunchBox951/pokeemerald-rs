//! Note gate countdown shared by the DirectSound and CGB voices.

/// Countdown to a note's automatic release.
///
/// A gate time of zero is a tied note that never expires on its own. A
/// non-zero gate time counts down one sequencer tick at a time; [`Gate::tick`]
/// returns `true` exactly once, on the tick that reaches zero, and the caller
/// then starts its voice's release. Later ticks return `false`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Gate {
    Tied,
    TicksRemaining(u16),
    Expired,
}

impl Gate {
    pub(crate) fn new(gate_time: u16) -> Self {
        if gate_time == 0 {
            Self::Tied
        } else {
            Self::TicksRemaining(gate_time)
        }
    }

    pub(crate) fn tick(&mut self) -> bool {
        match *self {
            Self::TicksRemaining(1) => {
                *self = Self::Expired;
                true
            }
            Self::TicksRemaining(remaining) => {
                *self = Self::TicksRemaining(remaining - 1);
                false
            }
            Self::Tied | Self::Expired => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::Gate;
    use crate::cgb_envelope::CgbAdsr;
    use crate::cgb_voice::{CgbChannelNumber, CgbVoice};
    use crate::envelope::Adsr;
    use crate::pitch;
    use crate::sample::WaveData;
    use crate::voice::Voice;

    #[test]
    fn tied_gate_never_expires() {
        let mut gate = Gate::new(0);
        assert!(!gate.tick());
        assert!(!gate.tick());
    }

    #[test]
    fn gate_expires_once_after_its_ticks() {
        let mut gate = Gate::new(2);
        assert!(!gate.tick());
        assert!(gate.tick());
        assert!(!gate.tick());
        assert!(Gate::new(1).tick());
    }

    #[test]
    fn gate_expiry_releases_the_envelope() {
        let mut voice = Voice::new(
            Arc::new(WaveData::one_shot(0, vec![50; 4])),
            Adsr::flat(),
            (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ,
            u8::MAX,
            u8::MAX,
            127,
            2,
            60,
            0,
            0,
            0,
        );
        assert!(!voice.is_stopping());
        voice.tick_gate();
        assert!(!voice.is_stopping());
        voice.tick_gate();
        assert!(voice.is_stopping());
    }

    #[test]
    fn cgb_gate_expiry_releases_the_envelope() {
        let mut voice = CgbVoice::square(
            CgbChannelNumber::Square1,
            2,
            None,
            CgbAdsr::flat(),
            60,
            0,
            u8::MAX,
            u8::MAX,
            127,
            2,
            60,
            0,
            0,
            0,
            0,
        );
        voice.begin_frame(false);
        assert!(!voice.is_stopping());
        voice.tick_gate();
        assert!(!voice.is_stopping());
        voice.tick_gate();
        assert!(voice.is_stopping());
    }
}
