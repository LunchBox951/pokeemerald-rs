use super::super::*;
use super::shared::{sample_cgb_envelope, MIDDLE_C};

const FIRST_NOISE_PERIOD_BYTE: usize = 4;

fn noise_with_period(period: u8) -> VoiceEntry {
    VoiceEntry::Noise(NoiseVoice {
        base_key: MIDDLE_C,
        length: 0,
        period,
        envelope: sample_cgb_envelope(),
        fixed_rate: false,
    })
}

#[test]
fn noise_period_boundaries_are_accepted_by_the_constructor_and_decode() {
    for period in [0u8, 1] {
        let group = VoiceGroup::new(vec![noise_with_period(period)]).unwrap();
        assert_eq!(VoiceGroup::decode(&group.encode()).unwrap(), group);
    }
}

#[test]
fn noise_period_out_of_range_is_rejected_at_construction() {
    for period in [2u8, 255] {
        assert_eq!(
            VoiceGroup::new(vec![noise_with_period(period)]).unwrap_err(),
            AudioError::NoisePeriodOutOfRange(period)
        );
    }
}

/// Upstream `_voice_noise` stores `period & 1` (`asm/macros/music_voice.inc`) and the
/// runtime reads only bit 0 (`audio::cgb_voice::noise_control_byte`), so a period above
/// `1` aliases an in-domain selector and both `new` and `decode` must reject it.
#[test]
fn new_and_decode_reject_out_of_range_noise_period() {
    assert_eq!(
        VoiceGroup::new(vec![noise_with_period(2)]).unwrap_err(),
        AudioError::NoisePeriodOutOfRange(2)
    );

    let mut bytes = VoiceGroup::new(vec![noise_with_period(1)])
        .unwrap()
        .encode();
    bytes[FIRST_NOISE_PERIOD_BYTE] = 3;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::NoisePeriodOutOfRange(3))
    );
}
